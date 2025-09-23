use std::{fs::File, io::Write, path::Path};
use tracing::error;

use crate::constants::{AUTHORITY_CERT, AUTHORITY_PFX};

fn write_file(source: &[u8], destination: &Path) {
    let mut file = match File::create(&destination) {
        Ok(file) => file,
        Err(error) => {
            error!("Failed to create path \"{:?}\": {:?}", destination, error);
            return;
        }
    };
    if let Err(error) = file.write_all(source) {
        error!("Failed to write .pfx to \"{:?}\": {:?}", destination, error)
    }
}

pub fn write_pfx(path: &Path) {
    if path.exists() {
        error!("File already exists: {}", path.display());
        return;
    }
    write_file(&AUTHORITY_PFX, path);
}

pub fn write_pem(path: &Path) {
    if path.exists() {
        error!("File already exists: {}", path.display());
        return;
    }
    write_file(&AUTHORITY_CERT, path);
}
