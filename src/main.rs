use anyhow::Result;
use std::env;
use std::process::exit;

mod container;
mod image;
fn main() -> Result<()> {
    image::pull_image("alpine")?;
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <rootfs-dir> <command> [args...]", args[0]);
        exit(1);
    }

    let rootfs = args[1].clone();
    let command = args[2..].to_vec();

    match container::run(&rootfs, &command) {
        Ok(code) => exit(code),
        Err(e) => {
            eprintln!("container error: {}", e);
            exit(1);
        }
    }
}
