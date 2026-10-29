#!/usr/bin/env python3
"""Prepare pinned WhisperKit code with a fail-closed offline tokenizer patch. No weights."""
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def prepare():
    patch = json.loads((ROOT / "native/macos/patches/offline-tokenizer.json").read_text())
    destination = ROOT / "native/macos/.build/offline-whisperkit"
    marker = destination / ".fnpilot-patch.json"
    if marker.exists() and json.loads(marker.read_text()) == patch:
        return
    cache = ROOT / "native/macos/.build/whisperkit-source"
    if not (cache / ".git").exists():
        cache.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "clone", "--no-checkout", "https://github.com/argmaxinc/WhisperKit.git", str(cache)], check=True)
    subprocess.run(["git", "-C", str(cache), "checkout", "--detach", patch["revision"]], check=True)
    revision = subprocess.check_output(["git", "-C", str(cache), "rev-parse", "HEAD"], text=True).strip()
    if revision != patch["revision"]:
        raise RuntimeError("WhisperKit source revision mismatch")
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(cache, destination, ignore=shutil.ignore_patterns(".git", ".build"))
    path = destination / patch["path"]
    source = path.read_text()
    if source.count(patch["old"]) != 1:
        raise RuntimeError("Offline tokenizer patch no longer matches the pinned source")
    path.write_text(source.replace(patch["old"], patch["new"]))
    marker.write_text(json.dumps(patch))


if __name__ == "__main__":
    prepare()
