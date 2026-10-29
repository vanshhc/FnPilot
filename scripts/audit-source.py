#!/usr/bin/env python3
"""Bounded candidate/history audit. Emits categories and paths, never matched secrets."""
import argparse
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PATTERNS = {
    "possible_private_key": re.compile(rb"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
    "possible_github_credential": re.compile(rb"(?:gh[opusr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,})"),
    "possible_cloud_key": re.compile(rb"(?:AKIA[0-9A-Z]{16}|sk-proj-[A-Za-z0-9_-]{40,})"),
    "personal_absolute_path": re.compile(rb"(?:/Users/[^/\s\"']+/|[A-Z]:\\Users\\[^\\\s]+\\)"),
}


def scan(data):
    return [name for name, pattern in PATTERNS.items() if pattern.search(data)]


def audit():
    listed = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT)
    files = sorted({p.decode() for p in listed.split(b"\0") if p})
    findings = []
    for name in files:
        path = ROOT / name
        if not path.is_file() or path.is_symlink():
            continue
        size = path.stat().st_size
        categories = scan(path.read_bytes()) if size < 8_000_000 else ["large_file_requires_review"]
        if path.suffix.lower() in [".p12", ".pfx", ".gguf", ".wav", ".mp3", ".dylib", ".exe", ".dll"]:
            categories.append("binary_or_media_requires_provenance")
        if categories:
            findings.append({"scope": "candidate", "path": name, "categories": categories})
    objects = subprocess.check_output(["git", "rev-list", "--objects", "--all"], cwd=ROOT, text=True).splitlines()
    blobs = 0
    for line in objects:
        sha, _, name = line.partition(" ")
        if subprocess.check_output(["git", "cat-file", "-t", sha], cwd=ROOT).strip() != b"blob":
            continue
        blobs += 1
        size = int(subprocess.check_output(["git", "cat-file", "-s", sha], cwd=ROOT))
        categories = scan(subprocess.check_output(["git", "cat-file", "blob", sha], cwd=ROOT)) if size < 8_000_000 else ["large_historical_blob_requires_review"]
        if categories:
            findings.append({"scope": "reachable_history", "object": sha, "path": name, "categories": categories})
    return {"schema_version": 1, "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(), "candidate_files": len(files), "historical_blobs": blobs, "findings": findings,
            "publication_approved": False, "limitations": "Pattern scan only; manual ownership, asset, credential and intended-history review remains required. Matches are never printed."}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(audit(), indent=2) + "\n")
