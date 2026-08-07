//! All filesystem/mount plumbing for the container: overlayfs setup,
//! pivot_root, virtual filesystem mounts, and diffing the change
//! layer afterwards.

use std::error::Error;
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

use nix::mount::{MntFlags, MsFlags, mount, umount2};
use nix::unistd::{chdir, getpid, pivot_root};

/// Paths involved in an overlayfs-backed container root.
pub struct OverlayPaths {
    /// Read-only base image / layer (never modified).
    pub lower: PathBuf,
    /// Read-write "change" layer — everything the container writes
    /// lands here, and it's what we diff afterwards.
    pub upper: PathBuf,
    /// Scratch dir required by the kernel overlay driver.
    pub work: PathBuf,
    /// Combined lower+upper view the container actually runs in.
    pub merged: PathBuf,
}

impl OverlayPaths {
    /// Derives upper/work/merged dirs from a per-run state directory,
    /// keeping `lower` (the base layer) untouched and reusable across
    /// runs.
    pub fn new(lower: impl Into<PathBuf>, state_dir: impl AsRef<Path>) -> Self {
        let state_dir = state_dir.as_ref();
        Self {
            lower: lower.into(),
            upper: state_dir.join("upper"),
            work: state_dir.join("work"),
            merged: state_dir.join("merged"),
        }
    }

    fn options(&self) -> String {
        format!(
            "lowerdir={},upperdir={},workdir={}",
            self.lower.display(),
            self.upper.display(),
            self.work.display()
        )
    }
}

/// Makes sure mount propagation events do not escape to the host.
pub fn make_mount_private() -> Result<(), Box<dyn Error>> {
    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_REC | MsFlags::MS_PRIVATE,
        None::<&str>,
    )?;
    Ok(())
}

/// Mounts an overlayfs combining `paths.lower` (read-only base layer)
/// with `paths.upper` (read-write change layer), exposing the result
/// at `paths.merged`.
pub fn mount_overlay(paths: &OverlayPaths) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(&paths.upper)?;
    fs::create_dir_all(&paths.work)?;
    fs::create_dir_all(&paths.merged)?;

    mount(
        Some("overlay"),
        &paths.merged,
        Some("overlay"),
        MsFlags::empty(),
        Some(paths.options().as_str()),
    )?;
    Ok(())
}

/// pivot_root()s into `new_root`, stashing the old root under a
/// unique scratch dir keyed on our pid and then detaching it so
/// nothing leaks outside this mount namespace.
pub fn switch_root(new_root: &Path) -> Result<(), Box<dyn Error>> {
    // Bind-mount new_root onto itself so it's a valid mount point for
    // pivot_root.
    mount(
        Some(new_root),
        new_root,
        None::<&str>,
        MsFlags::MS_BIND | MsFlags::MS_REC,
        None::<&str>,
    )?;

    let old_root = new_root.join(format!(".old_root.{}", getpid()));
    fs::create_dir_all(&old_root)?;

    chdir(new_root)?;
    let old_root_rel = old_root
        .strip_prefix(new_root)
        .unwrap_or_else(|_| Path::new(".old_root"));
    pivot_root(".", old_root_rel)?;
    chdir("/")?;

    let old_root_abs = Path::new("/").join(old_root_rel);

    umount2(&old_root_abs, MntFlags::MNT_DETACH)?;
    fs::remove_dir(&old_root_abs).ok();
    Ok(())
}

/// Mounts the standard virtual filesystems (proc/sys/dev) inside the
/// new root. sys/dev are best-effort since some environments (e.g.
/// nested containers) may not permit them.
pub fn mount_virtual_fs() -> Result<(), Box<dyn Error>> {
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

    Ok(())
}

/// Full rootfs setup, run from inside the container's own namespaces:
/// mount overlay, pivot into it, mount proc/sys/dev.
pub fn setup_rootfs(paths: &OverlayPaths) -> Result<(), Box<dyn Error>> {
    make_mount_private()?;
    mount_overlay(paths)?;
    switch_root(&paths.merged)?;
    mount_virtual_fs()?;
    Ok(())
}

/// What happened to a given path in the change layer.
#[derive(Debug)]
pub enum ChangeKind {
    AddedOrModified,
    Removed,
}

#[derive(Debug)]
pub struct ChangedFile {
    pub path: PathBuf,
    pub kind: ChangeKind,
}

/// Whether a directory entry in an overlayfs upperdir is a
/// "whiteout" marker recording that a file was deleted in the upper
/// layer. The kernel represents these as char devices with major/
/// minor 0/0.
fn is_whiteout(meta: &fs::Metadata) -> bool {
    meta.file_type().is_char_device() && meta.rdev() == 0
}

/// Recursively walks `upper` (the overlay's change layer) — from
/// *outside* the container's mount namespace, so this is safe to call
/// from the parent process after the child has exited — and returns
/// every path that changed relative to the base layer.
pub fn changed_files(upper: &Path) -> Result<Vec<ChangedFile>, Box<dyn Error>> {
    let mut changes = Vec::new();
    if upper.exists() {
        walk(upper, upper, &mut changes)?;
    }
    Ok(changes)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<ChangedFile>) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let meta = entry.metadata()?;
        let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();

        if meta.is_dir() {
            walk(root, &path, out)?;
        } else if is_whiteout(&meta) {
            out.push(ChangedFile {
                path: rel,
                kind: ChangeKind::Removed,
            });
        } else {
            out.push(ChangedFile {
                path: rel,
                kind: ChangeKind::AddedOrModified,
            });
        }
    }
    Ok(())
}
