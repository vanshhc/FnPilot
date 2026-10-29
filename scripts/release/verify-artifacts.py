#!/usr/bin/env python3
"""Verify an exact release inventory. Signature verification is platform-native."""
import argparse
import hashlib
import json
from pathlib import Path


def verify(manifest, root, revision):
    if manifest.get("schema_version") != 1 or manifest.get("source_revision") != revision:
        raise ValueError("unsupported manifest or source revision mismatch")
    if not manifest.get("engine_revisions") or not manifest.get("dependency_inventory"):
        raise ValueError("missing engine or dependency provenance")
    entries = manifest.get("files", [])
    names = [item["path"] for item in entries]
    if len(names) != len(set(names)) or not entries:
        raise ValueError("empty or duplicate artifact inventory")
    actual = {str(path.relative_to(root)).replace("\\", "/") for path in root.rglob("*") if path.is_file()}
    if actual != set(names):
        raise ValueError("missing or unexpected artifact file")
    roles = {item.get("role") for item in entries}
    if not {"app", "cli", "runtime", "notices", "sbom"}.issubset(roles):
        raise ValueError("missing required app, CLI, helper, notice or SBOM artifact")
    for item in entries:
        relative = Path(item["path"])
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError("artifact path escapes staging")
        path = root / relative
        if path.is_symlink() or not path.resolve().is_relative_to(root.resolve()):
            raise ValueError("artifact symlink or path escape")
        if path.stat().st_size != item.get("size_bytes") or hashlib.sha256(path.read_bytes()).hexdigest() != item.get("sha256"):
            raise ValueError("artifact size or checksum mismatch")
    if manifest.get("official") and manifest.get("acceptance_receipts") != {"macos": "passed", "windows": "passed", "performance": "passed", "upgrade": "passed"}:
        raise ValueError("official release requires native, performance and upgrade receipts")
    return True


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("artifact_directory", type=Path)
    parser.add_argument("--revision", required=True)
    args = parser.parse_args()
    verify(json.loads(args.manifest.read_text()), args.artifact_directory, args.revision)
    print("Artifact inventory and hashes verified. Verify native signatures separately.")
