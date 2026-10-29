use fnpilot_core::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelManifest {
    pub format: String,
    pub upstream_revision: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub license: String,
    pub architecture: String,
}
impl ModelManifest {
    pub fn verify(&self, path: &Path) -> Result<()> {
        if self.format != "ggml-whisper"
            || self.upstream_revision.trim().is_empty()
            || self.license.trim().is_empty()
            || self.architecture.trim().is_empty()
            || self.sha256.len() != 64
            || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || self.size_bytes < 4
        {
            return Err("Incomplete model provenance or unsupported model format.".into());
        }
        let mut file = File::open(path)
            .map_err(|_| "Select a portable whisper.cpp model file, not a Core ML directory.")?;
        if file
            .metadata()
            .map_err(|_| "Could not inspect model.")?
            .len()
            != self.size_bytes
        {
            return Err("Model size mismatch.".into());
        }
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        let mut first = true;
        loop {
            let n = file
                .read(&mut buffer)
                .map_err(|_| "Could not read model.")?;
            if n == 0 {
                break;
            }
            if first && &buffer[..4] != b"lmgg" {
                return Err("The file is not a whisper.cpp GGML model.".into());
            }
            first = false;
            hash.update(&buffer[..n]);
        }
        if format!("{:x}", hash.finalize()) != self.sha256.to_lowercase() {
            return Err("Model checksum mismatch.".into());
        }
        Ok(())
    }
}
/// Explicit setup only: copy a user-selected artifact, verify it, then atomically install.
pub fn install(
    source: &Path,
    destination: &Path,
    manifest: &ModelManifest,
    cancelled: impl Fn() -> bool,
) -> Result<()> {
    if destination.exists() {
        return Err("A model already exists at that destination. Choose a new destination.".into());
    }
    let temporary = destination.with_extension("partial");
    let mut created = false;
    let result = (|| {
        let mut input = File::open(source).map_err(|_| "Could not open selected model.")?;
        let mut output = File::options()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| "Could not create model staging file.")?;
        created = true;
        let mut buffer = [0u8; 65536];
        loop {
            if cancelled() {
                return Err("Model setup cancelled.".into());
            }
            let n = input
                .read(&mut buffer)
                .map_err(|_| "Could not read selected model.")?;
            if n == 0 {
                break;
            }
            output
                .write_all(&buffer[..n])
                .map_err(|_| "Could not stage model.")?;
        }
        output.sync_all().map_err(|_| "Could not flush model.")?;
        drop(output);
        manifest.verify(&temporary)?;
        if cancelled() {
            return Err("Model setup cancelled.".into());
        }
        // Hard-link creation refuses to replace another installer/user's destination.
        fs::hard_link(&temporary, destination)
            .map_err(|_| "Could not atomically install model.")?;
        Ok(())
    })();
    if created {
        let _ = fs::remove_file(&temporary);
    }
    result
}
