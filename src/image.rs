use anyhow::{Context, Result};
use std::fs;
use std::io;
use std::path::Path;
use flate2::read::GzDecoder;
use tar::Archive;

pub fn pull_image(distro: &str) -> Result<()> {
    fs::create_dir_all("./downloads")?;
    let url = "https://dl-cdn.alpinelinux.org/alpine/v3.24/releases/x86_64/alpine-minirootfs-3.24.1-x86_64.tar.gz";
    let mut response = reqwest::blocking::get(url).context("Failed to download image")?;
    let cache_path = "./downloads/alpine-minirootfs-3.24.1-x86_64.tar.gz";
    if Path::new(&cache_path).exists() {
        println!("Using cached image.")
    }
    else {
        let mut file = fs::File::create(&cache_path).context("Failed to create cache file")?;
        io::copy(&mut response, &mut file).context("Failed to save image to cache")?;
    }


    let target_dir = "./images/alpine";
    if Path::new(&target_dir).exists() {
        println!("Image already unpacked");
        return Ok(());
    }
    fs::create_dir_all(&target_dir).context("Failed to create image directory")?;
    let file = fs::File::open("./downloads/alpine-minirootfs-3.24.1-x86_64.tar.gz").context("Failed to open download")?;
    let tar = GzDecoder::new(file);
    let mut archive = Archive::new(tar);
    archive.unpack(&target_dir).context("Failed to unpack tar.gz")?;
    Ok(())
}
