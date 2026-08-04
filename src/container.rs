use std::error::Error;
use std::ffi::CString;
use std::fs;
use std::process::exit;

use nix::mount::{MntFlags, MsFlags, mount, umount2};
use nix::sched::{CloneFlags, unshare};
use nix::sys::prctl;
use nix::sys::signal::Signal;
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, chdir, execvp, fork, pivot_root, sethostname};

fn setup_rootfs(rootfs: &str) -> Result<(), Box<dyn Error>> {
    // Make sure mount propagation events do not escape to the host.
    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_REC | MsFlags::MS_PRIVATE,
        None::<&str>,
    )?;

    // Bind-mount rootfs onto itself so it becomes a valid mount point
    // for pivot_root.
    mount(
        Some(rootfs),
        rootfs,
        None::<&str>,
        MsFlags::MS_BIND | MsFlags::MS_REC,
        None::<&str>,
    )?;

    let old_root = format!("{}/.old_root", rootfs);
    fs::create_dir_all(&old_root)?;

    chdir(rootfs)?;
    pivot_root(".", ".old_root")?;
    chdir("/")?;

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
    umount2("/.old_root", MntFlags::MNT_DETACH)?;
    fs::remove_dir("/.old_root").ok();

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

pub fn run(rootfs: &str, command: &[String]) -> Result<i32, Box<dyn Error>> {
    //
    unshare(
        CloneFlags::CLONE_NEWNS
            | CloneFlags::CLONE_NEWUTS
            | CloneFlags::CLONE_NEWPID
            | CloneFlags::CLONE_NEWIPC,
    )?;

    match unsafe { fork()? } {
        ForkResult::Parent { child } => {
            // Blocking wait guarantees we reap the child and observe
            // its exit even if it crashed — namespace teardown then
            // happens automatically once both processes are gone.
            match waitpid(child, None)? {
                WaitStatus::Exited(_, code) => Ok(code),
                WaitStatus::Signaled(_, sig, _) => {
                    eprintln!("container process killed by signal {:?}", sig);
                    Ok(128)
                }
                _ => Ok(1),
            }
        }
        ForkResult::Child => {
            println!("rootfs: {rootfs:#?}, cmd: {command:#?}");
            if let Err(e) = child_main(rootfs, command) {
                eprintln!("container setup failed: {}", e);
                exit(127);
            }
            unreachable!();
        }
    }
}
