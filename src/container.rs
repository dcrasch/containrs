use std::error::Error;
use std::ffi::CString;
use std::process::exit;

use nix::sched::{CloneFlags, clone};
use nix::sys::prctl;
use nix::sys::signal::Signal;
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{execvp, getpid, sethostname};

use crate::mounts::{ChangeKind, OverlayPaths, changed_files, setup_rootfs};

const STACK_SIZE: usize = 1024 * 1024;

fn child_main(paths: &OverlayPaths, command: &[String]) -> Result<(), Box<dyn Error>> {
    // If the parent dies unexpectedly, kill this child too.
    prctl::set_pdeathsig(Signal::SIGKILL)?;
    sethostname("container")?;
    setup_rootfs(paths)?;

    let cmd = CString::new(command[0].as_str())?;
    let cargs: Vec<CString> = command
        .iter()
        .map(|a| CString::new(a.as_str()).unwrap())
        .collect();

    // execvp replaces this process image; stdio fds are inherited
    // automatically, so stdin/stdout/stderr just flow through.
    execvp(&cmd, &cargs)?;
    unreachable!("execvp only returns on error");
}

/// Runs `command` inside a fresh set of namespaces, with the rootfs
/// built as an overlayfs on top of `base_layer` (read-only). All
/// writes the container makes are captured in a per-run change layer
/// under `state_dir`, which is what we diff once the container exits.
///
/// Safe to call repeatedly (sequentially): every invocation gets its
/// own stack, its own child, fresh namespaces, and its own change
/// layer under `state_dir`.
pub fn run(base_layer: &str, state_dir: &str, command: &[String]) -> Result<i32, Box<dyn Error>> {
    let flags = CloneFlags::CLONE_NEWNS
        | CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWPID
        | CloneFlags::CLONE_NEWIPC;

    // Unique per-run state dir so concurrent/previous runs never
    // collide, keyed on our own pid at call time.
    let run_dir = format!("{}/run-{}", state_dir, getpid());
    let paths = OverlayPaths::new(base_layer, &run_dir);

    // Fresh stack per call — never shared/reused across invocations.
    let mut stack = vec![0u8; STACK_SIZE];

    let command_owned = command.to_vec();
    let paths_for_child = OverlayPaths::new(paths.lower.clone(), &run_dir);

    let child_fn = Box::new(move || -> isize {
        println!(
            "base layer: {:?}, change layer: {:?}, cmd: {:?}",
            paths_for_child.lower, paths_for_child.upper, command_owned
        );
        if let Err(e) = child_main(&paths_for_child, &command_owned) {
            eprintln!("container setup failed: {}", e);
            exit(127);
        }
        unreachable!();
    });

    // SAFETY: the child only runs `child_fn`, the stack buffer is
    // freshly allocated for this call and lives until after waitpid,
    // and no other clone()d child is concurrently using it.
    let child = unsafe { clone(child_fn, &mut stack, flags, Some(Signal::SIGCHLD as i32))? };

    let exit_code = match waitpid(child, None)? {
        WaitStatus::Exited(_, code) => code,
        WaitStatus::Signaled(_, sig, _) => {
            eprintln!("container process killed by signal {:?}", sig);
            128
        }
        _ => 1,
    };

    // The change layer lives on the host filesystem outside the
    // container's mount namespace, so it's still readable here now
    // that the child has exited.
    report_changes(&paths.upper)?;

    Ok(exit_code)
}

fn report_changes(upper: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let changes = changed_files(upper)?;
    if changes.is_empty() {
        println!("no files changed");
        return Ok(());
    }

    println!("files changed ({}):", changes.len());
    for change in changes {
        let marker = match change.kind {
            ChangeKind::AddedOrModified => "+",
            ChangeKind::Removed => "-",
        };
        println!("  {} {}", marker, change.path.display());
    }
    Ok(())
}
