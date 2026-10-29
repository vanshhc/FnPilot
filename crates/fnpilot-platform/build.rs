fn main() {
    println!("cargo:rerun-if-changed=../../native/macos/Sources");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    for file in [
        "native/macos/Package.swift",
        "native/macos/Package.resolved",
        "native/macos/patches/offline-tokenizer.json",
        "scripts/prepare-whisperkit.py",
    ] {
        println!("cargo:rerun-if-changed=../../{file}");
    }
    let prepared = std::process::Command::new("python3")
        .arg(root.join("scripts/prepare-whisperkit.py"))
        .status()
        .expect("Python 3 is required to prepare offline WhisperKit");
    assert!(
        prepared.success(),
        "Could not prepare the pinned offline speech backend"
    );
    let status = std::process::Command::new("swift")
        .args(["build", "--package-path"])
        .arg(root.join("native/macos"))
        .args([
            "--configuration",
            "release",
            "--product",
            "FnPilotSpeech",
            "--force-resolved-versions",
        ])
        .status()
        .expect("Install Xcode command line tools for the WhisperKit speech binding");
    assert!(
        status.success(),
        "WhisperKit speech binding compilation failed"
    );
    std::fs::copy(
        root.join("native/macos/.build/release/libFnPilotSpeech.a"),
        out.join("libFnPilotSpeech.a"),
    )
    .unwrap();
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=FnPilotSpeech");
    let swift = std::process::Command::new("xcrun")
        .args(["--find", "swiftc"])
        .output()
        .unwrap();
    let path = std::path::PathBuf::from(String::from_utf8(swift.stdout).unwrap().trim());
    let runtime = path.parent().unwrap().join("../lib/swift/macosx");
    println!("cargo:rustc-link-search=native={}", runtime.display());
    let mut runtime_paths = vec![runtime.clone(), std::path::PathBuf::from("/usr/lib/swift")];
    // New Xcode versions ship back-deployed concurrency runtimes in versioned
    // directories, which Swift's linker driver normally adds automatically.
    let library_root = path.parent().unwrap().join("../lib");
    for entry in std::fs::read_dir(library_root).unwrap().flatten() {
        if entry.file_name().to_string_lossy().starts_with("swift-") {
            let candidate = entry.path().join("macosx");
            if candidate.is_dir() {
                runtime_paths.push(candidate);
            }
        }
    }
    for runtime_path in &runtime_paths {
        println!("cargo:rustc-link-search=native={}", runtime_path.display());
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", runtime_path.display());
    }
    println!(
        "cargo::metadata=swift_runtime_paths={}",
        runtime_paths
            .iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>()
            .join(";")
    );
    println!("cargo:rustc-link-lib=dylib=swiftCore");
    for framework in [
        "AppKit",
        "AVFoundation",
        "ApplicationServices",
        "CoreAudio",
        "CoreML",
        "Accelerate",
        "Foundation",
        "CoreGraphics",
        "ServiceManagement",
    ] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}
