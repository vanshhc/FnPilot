#!/usr/bin/env python3
"""Convert an explicitly provisioned, checksummed corpus to portable WAV without downloads."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("output_directory", type=Path)
    args = parser.parse_args()
    original = args.manifest.read_bytes()
    clips = json.loads(original)
    output = args.output_directory.resolve()
    output.mkdir(parents=True, exist_ok=False)
    converted = []
    for clip in clips:
        source = args.manifest.parent / clip["path"]
        digest = hashlib.sha256(source.read_bytes()).hexdigest()
        if digest != clip["sha256"]:
            raise ValueError("Source audio checksum mismatch")
        destination = output / f"{digest}.wav"
        if not destination.exists():
            if sys.platform == "darwin":
                subprocess.run(["afconvert", str(source), str(destination), "-f", "WAVE", "-d", "LEI16@16000", "-c", "1"], check=True)
            elif shutil.which("ffmpeg"):
                subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-i", str(source), "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", str(destination)], check=True)
            else:
                raise RuntimeError("Provision an audio converter explicitly: macOS afconvert or an existing ffmpeg installation")
        converted.append({**clip, "path": destination.name, "sourceSHA256": digest, "sha256": hashlib.sha256(destination.read_bytes()).hexdigest()})
    (output / "manifest.json").write_text(json.dumps(converted, indent=2) + "\n")
    (output / "provenance.json").write_text(json.dumps({"sourceManifestSHA256": hashlib.sha256(original).hexdigest(), "conversion": "16 kHz mono PCM16 WAV", "downloads": False}, indent=2) + "\n")
