use anyhow::{Context, Result};
mod image;

fn main() -> Result<()> {
    image::pull_image("alpine")?;
    Ok(())
}
