use std::error::Error;
use std::ffi::CString;
use std::fs;
use std::process::exit;

use nix::mount::{MntFlags, MsFlags, mount, umount2};
use nix::sched::{CloneFlags, clone};
use nix::sys::prctl;
use nix::sys::signal::Signal;
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{chdir, execvp, getpid, pivot_root, sethostname};

const STACK_SIZE: usize = 1024 * 1024;

fn setup_rootfs(rootfs: &str) -> Result<(), Box<dyn Error>> {
    // Make sure mount propagation events do not escape to the host.
    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_REC | MsFlags::MS_PRIVATE,
        None::<&str>,
    );

    // Bind-mount rootfs onto itself so it becomes a valid mount point
    // for pivot_root.
    mount(
        Some(rootfs),
        rootfs,
        None::<&str>,
        MsFlags::MS_BIND | MsFlags::MS_REC,
        None::<&str>,
    )?;

    // Unique scratch dir name per-run (keyed on our own pid, which is
    // unique within the new PID namespace *and* distinct across runs
    // since each run gets a brand new namespace/process). Avoids
    // collisions with leftovers from a previous failed run.
    let old_root = format!("{}/.old_root.{}", rootfs, getpid());
    fs::create_dir_all(&old_root)?;

    chdir(rootfs)?;
    let old_root_rel = old_root
        .strip_prefix(&format!("{}/", rootfs))
        .unwrap_or(".old_root");
    pivot_root(".", old_root_rel)?;
    chdir("/")?;

    let old_root_abs = format!("/{}", old_root_rel);

    fs::create_dir_all("/proc").ok();
    mount(
        Some("proc"),
        "/proc",
        Some("proc"),
        MsFlags::empty(),
        None::<&str>,
    )?;

    fs::create_dir_all("/sys").ok();
    let _ = mount(
        Some("sysfs"),
        "/sys",
        Some("sysfs"),
        MsFlags::empty(),
        None::<&str>,
    );

    fs::create_dir_all("/dev").ok();
    let _ = mount(
        Some("tmpfs"),
        "/dev",
        Some("tmpfs"),
        MsFlags::empty(),
        None::<&str>,
    );

    // Detach the old root — this is what actually "restores" the host
    // view for this process tree; nothing leaks outside the namespace.
    umount2(old_root_abs.as_str(), MntFlags::MNT_DETACH)?;
    fs::remove_dir(&old_root_abs).ok();
    Ok(())
}

fn child_main(rootfs: &str, command: &[String]) -> Result<(), Box<dyn Error>> {
    // If the parent dies unexpectedly, kill this child too.
    prctl::set_pdeathsig(Signal::SIGKILL)?;
    sethostname("container")?;
    setup_rootfs(rootfs)?;

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

/// Runs `command` inside a fresh set of namespaces rooted at `rootfs`.
/// Safe to call this repeatedly (sequentially) within the same
/// program — every invocation allocates its own stack, spawns its own
/// child, and gets entirely fresh namespaces with no shared state
/// carried over from a previous call.
pub fn run(rootfs: &str, command: &[String]) -> Result<i32, Box<dyn Error>> {
    let flags = CloneFlags::CLONE_NEWNS
        | CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWPID
        | CloneFlags::CLONE_NEWIPC;

    // Fresh stack per call — never shared/reused across invocations.
    let mut stack = vec![0u8; STACK_SIZE];

    let rootfs_owned = rootfs.to_string();
    let command_owned = command.to_vec();

    let child_fn = Box::new(move || -> isize {
        println!("rootfs: {rootfs_owned:#?}, cmd: {command_owned:#?}");
        if let Err(e) = child_main(&rootfs_owned, &command_owned) {
            eprintln!("container setup failed: {}", e);
            exit(127);
        }
        unreachable!();
    });

    // SAFETY: the child only runs `child_fn`, the stack buffer is
    // freshly allocated for this call and lives until after waitpid,
    // and no other clone()d child is concurrently using it.
    let child = unsafe { clone(child_fn, &mut stack, flags, Some(Signal::SIGCHLD as i32))? };

    match waitpid(child, None)? {
        WaitStatus::Exited(_, code) => Ok(code),
        WaitStatus::Signaled(_, sig, _) => {
            eprintln!("container process killed by signal {:?}", sig);
            Ok(128)
        }
        _ => Ok(1),
    }
}
