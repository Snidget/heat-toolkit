use std::{env, error::Error, fs, path::Path};

use heat3_povorotnik::step_export::write_step;

fn write_fixture(directory: &Path, name: &str, box_: [f64; 6]) -> Result<(), Box<dyn Error>> {
    let path = directory.join(name);
    write_step(&path, &[box_])?;
    println!("Wrote STEP fixture: {}", path.display());
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let output_directory = env::args_os()
        .nth(1)
        .ok_or("usage: step_import_fixtures <output-directory>")?;
    let output_directory = Path::new(&output_directory);
    fs::create_dir_all(output_directory)?;

    // HEAT3 lengths are metres. A conforming STEP consumer should report mm.
    write_fixture(
        output_directory,
        "building-scale.step",
        [0.0, 0.0, 0.0, 1.0, 0.5, 0.1],
    )?;
    // Keep a 0.1 mm feature distinguishable at a 1,000,000 mm x offset.
    write_fixture(
        output_directory,
        "thin-feature-large-offset.step",
        [1000.0, 2.0, 3.0, 1000.0001, 2.1, 3.1],
    )?;
    Ok(())
}
