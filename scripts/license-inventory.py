#!/usr/bin/env python3
"""Inventory locked dependencies without serializing local registry/home paths."""
import argparse
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def inventory():
    target = next(line.split(":",1)[1].strip() for line in subprocess.check_output(["cargo", "-vV"], text=True).splitlines() if line.startswith("host:"))
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--offline", "--filter-platform", target, "--format-version", "1"], cwd=ROOT))
    cargo_lock = (ROOT / "Cargo.lock").read_text()
    rust = []
    for package in metadata["packages"]:
        rust.append({key: package.get(key) for key in ["name", "version", "license", "repository", "source"]})
    lock = json.loads((ROOT / "package-lock.json").read_text())
    node = [{"package": name, "version": value.get("version"), "license": value.get("license"), "integrity": value.get("integrity")} for name, value in lock["packages"].items() if "node_modules/" in name]
    import hashlib
    return {"schema_version": 1, "cargo_lock_sha256": hashlib.sha256(cargo_lock.encode()).hexdigest(), "rust": sorted(rust, key=lambda p: (p["name"], p["version"])), "node": node,
            "swift": {"manifest": "native/macos/Package.swift", "pins": json.loads((ROOT / "native/macos/Package.resolved").read_text())["pins"], "whisperkit_revision": json.loads((ROOT / "native/macos/patches/offline-tokenizer.json").read_text())["revision"], "offline_patch_sha256": hashlib.sha256((ROOT / "native/macos/patches/offline-tokenizer.json").read_bytes()).hexdigest(), "notices": "THIRD_PARTY_NOTICES.txt"},
            "native_runtime": json.loads((ROOT / "scripts/runtime-manifest.json").read_text()),
            "assets": [{"path": "apps/desktop/src-tauri/icons/icon.png", "origin": "Original geometric F icon created for FnPilot; no external asset", "license": "MIT"}, {"path": "docs/validation/mock-settings.png", "origin": "FnPilot mock UI screenshot containing a synthetic instruction", "license": "MIT"}],
            "model_weights": "Not distributed; approval and upstream model terms required separately", "audio_corpus": "No audio distributed; provenance manifest pending", "redistribution_review": "pending"}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    args.output.write_text(json.dumps(inventory(), indent=2) + "\n")
