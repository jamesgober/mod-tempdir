//! Minimal example: create a temp directory, write a file, observe auto-cleanup.
//!
//! Run with: `cargo run --example basic`

use mod_tempdir::TempDir;

fn main() -> std::io::Result<()> {
    let dir = TempDir::new()?;
    println!("Created: {}", dir.path().display());

    let file = dir.path().join("greeting.txt");
    std::fs::write(&file, b"Hello from mod-tempdir!\n")?;
    println!("Wrote:   {}", file.display());

    println!("Contents: {}", std::fs::read_to_string(&file)?);
    println!("\n(directory will be deleted automatically when this program exits)");

    Ok(())
}
