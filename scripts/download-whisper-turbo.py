#!/usr/bin/env python3
"""Install the full Turbo Core ML model for FnPilot's offline comparison.

Model weights are pinned and checked against Hugging Face's SHA-256 metadata.
This does not change FnPilot's selected model or record any microphone audio.
"""
import concurrent.futures
import hashlib
import json
import pathlib
import time
import urllib.request

REPO = "argmaxinc/whisperkit-coreml"
REVISION = "0f63a7800b00dd0226abd051b906c246e1907482"
VARIANT = "openai_whisper-large-v3_turbo"
BASE = pathlib.Path.home() / "Library/Application Support/FnPilot/Models/models"


def checksum(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def download(entry):
    relative = pathlib.PurePosixPath(entry["path"])
    if relative.is_absolute() or ".." in relative.parts or relative.parts[0] != VARIANT:
        raise ValueError("Unexpected model file path")
    target = BASE / REPO / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    expected_hash = entry.get("lfs", {}).get("oid")
    if target.exists() and target.stat().st_size == entry["size"]:
        if not expected_hash or checksum(target) == expected_hash:
            return
    for attempt in range(3):
        try:
            temporary = target.with_name(target.name + ".part")
            url = f"https://huggingface.co/{REPO}/resolve/{REVISION}/{relative}?download=true"
            with urllib.request.urlopen(url, timeout=120) as response, temporary.open("wb") as output:
                while chunk := response.read(1024 * 1024):
                    output.write(chunk)
            if temporary.stat().st_size != entry["size"]:
                raise ValueError("Model file size mismatch")
            if expected_hash and checksum(temporary) != expected_hash:
                raise ValueError("Model file checksum mismatch")
            temporary.replace(target)
            print("Ready", relative, flush=True)
            return
        except Exception:
            if attempt == 2:
                raise
            time.sleep(2)


def main():
    url = f"https://huggingface.co/api/models/{REPO}/tree/{REVISION}/{VARIANT}?recursive=true"
    with urllib.request.urlopen(url, timeout=60) as response:
        files = [entry for entry in json.load(response) if entry["type"] == "file"]
    print(f"Checking/downloading {len(files)} files ({sum(x['size'] for x in files)/1e9:.2f} GB)", flush=True)
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(download, files))
    # Resolve once so all tokenizer files come from the same revision.
    tokenizer_repo = "openai/whisper-large-v3"
    with urllib.request.urlopen(f"https://huggingface.co/api/models/{tokenizer_repo}", timeout=60) as response:
        tokenizer_revision = json.load(response)["sha"]
    tokenizer = BASE / tokenizer_repo
    tokenizer.mkdir(parents=True, exist_ok=True)
    for name in ["tokenizer.json", "tokenizer_config.json", "config.json", "generation_config.json"]:
        temporary = tokenizer / (name + ".part")
        urllib.request.urlretrieve(f"https://huggingface.co/{tokenizer_repo}/resolve/{tokenizer_revision}/{name}", temporary)
        temporary.replace(tokenizer / name)
    provenance = {"modelRepo": REPO, "modelRevision": REVISION,
                  "tokenizerRepo": tokenizer_repo, "tokenizerRevision": tokenizer_revision}
    (BASE / REPO / VARIANT / "fnpilot-download.json").write_text(json.dumps(provenance, indent=2) + "\n")
    print("Turbo installed. FnPilot's selected model is unchanged.")


if __name__ == "__main__":
    main()
