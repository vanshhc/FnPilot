use fnpilot_speech::model_store::{ModelManifest, install};
use sha2::{Digest, Sha256};
#[test]
fn incomplete_cancelled_and_bad_hash_artifacts_never_install() {
    let folder = std::env::temp_dir().join(format!("fnpilot-model-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let bytes = b"lmggsynthetic-fixture-not-real-weights";
    let source = folder.join("source.bin");
    let destination = folder.join("model.bin");
    std::fs::write(&source, bytes).unwrap();
    let mut manifest = ModelManifest {
        format: "ggml-whisper".into(),
        upstream_revision: "fixture-v1".into(),
        sha256: format!("{:x}", Sha256::digest(bytes)),
        size_bytes: bytes.len() as u64,
        license: "synthetic-MIT".into(),
        architecture: "cpu".into(),
    };
    let staging = destination.with_extension("partial");
    std::fs::write(&staging, b"another setup's partial data").unwrap();
    assert!(install(&source, &destination, &manifest, || false).is_err());
    assert_eq!(
        std::fs::read(&staging).unwrap(),
        b"another setup's partial data"
    );
    std::fs::remove_file(&staging).unwrap();
    assert!(install(&source, &destination, &manifest, || true).is_err());
    assert!(!destination.exists());
    manifest.sha256 = "0".repeat(64);
    assert!(install(&source, &destination, &manifest, || false).is_err());
    assert!(!destination.exists());
    manifest.sha256 = format!("{:x}", Sha256::digest(bytes));
    install(&source, &destination, &manifest, || false).unwrap();
    assert!(manifest.verify(&destination).is_ok());
    assert!(install(&source, &destination, &manifest, || false).is_err());
    assert!(!destination.with_extension("partial").exists());
    std::fs::remove_dir_all(folder).unwrap();
}
