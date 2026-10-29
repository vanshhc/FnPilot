#!/usr/bin/env python3
"""Create a hash inventory outside a completed distribution directory."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def inventory(directory, revision, official):
    files = []
    for path in sorted(directory.rglob("*")):
        if path.is_symlink():
            raise ValueError("Distributions must contain regular files, not symlinks")
        if not path.is_file():
            continue
        name = path.relative_to(directory).as_posix()
        role = "app"
        if name.startswith("cli/"): role = "cli"
        elif name.startswith("runtime/"): role = "runtime"
        elif name.startswith("licenses/"): role = "notices"
        elif name == "dependencies.json": role = "sbom"
        files.append({"path": name, "role": role, "size_bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    return {"schema_version": 1, "source_revision": revision, "official": official,
            "engine_revisions": {"llama.cpp": json.loads((ROOT / "scripts/runtime-manifest.json").read_text())["revision"], "WhisperKit": json.loads((ROOT / "native/macos/patches/offline-tokenizer.json").read_text())["revision"]},
            "dependency_inventory": "dependencies.json", "files": files,
            "acceptance_receipts": {key: "passed" for key in ["macos", "windows", "performance", "upgrade"]} if official else {}}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--official", action="store_true")
    args = parser.parse_args()
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    if args.official:
        import importlib.util, os
        spec = importlib.util.spec_from_file_location("gates", ROOT / "scripts/release/check-gates.py")
        gates = importlib.util.module_from_spec(spec); spec.loader.exec_module(gates)
        gates.check(json.loads(os.environ["ACCEPTANCE_RECEIPTS_JSON"]), revision)
    args.manifest.write_text(json.dumps(inventory(args.directory, revision, args.official), indent=2) + "\n")
