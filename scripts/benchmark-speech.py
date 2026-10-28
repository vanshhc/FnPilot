#!/usr/bin/env python3
"""Prepare a pinned corpus or score FnPilot's headless speech benchmark reports."""
import argparse
import array
import concurrent.futures
import hashlib
import json
import math
import pathlib
import random
import re
import statistics
import subprocess
import sys
import urllib.request
import wave

DATASET = "argmaxinc/whisperkit-test-data"
REVISION = "df439ffa58323ef5d36d3f4caa3dd26708d834bd"
COMMANDS = [
    "Open Calculator", "Switch to Google Chrome", "Open TextEdit",
    "Quit Safari", "Minimize the window", "Restore the window",
    "Full screen the video", "Volume down", "Brightness up",
    "Open Notes and type Buy milk and eggs",
    "Open Calculator then volume up", "Type Please send the report tomorrow",
]
VOICES = ["Samantha", "Daniel", "Rishi"]


def fetch_json(url):
    with urllib.request.urlopen(url, timeout=60) as response:
        return json.load(response)


def prepare(directory):
    directory.mkdir(parents=True, exist_ok=True)
    audio_dir = directory / "audio"
    audio_dir.mkdir(exist_ok=True)
    root = f"https://huggingface.co/datasets/{DATASET}/resolve/{REVISION}"
    metadata = fetch_json(f"{root}/librispeech-10mins/metadata.json")
    # Two short clips from each of the first twelve speakers in the frozen metadata.
    selected = {}
    for item in metadata:
        speaker = item["audio"].split("-")[0]
        if speaker not in selected and len(selected) == 12:
            continue
        if 1.5 <= item["duration"] <= 8:
            group = selected.setdefault(speaker, [])
            if len(group) < 2:
                group.append(item)
    assert len(selected) == 12 and all(len(group) == 2 for group in selected.values())
    humans = [item for group in selected.values() for item in group]

    def download(item):
        name = pathlib.Path(item["audio"]).stem + ".mp3"
        target = audio_dir / name
        if not target.exists():
            urllib.request.urlretrieve(f"{root}/librispeech/{name}", target)
        return {"id": pathlib.Path(name).stem, "path": f"audio/{name}",
                "reference": item["text"], "category": "human"}

    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        clips = list(pool.map(download, humans))
    for voice in VOICES:
        for index, command in enumerate(COMMANDS):
            name = f"{voice.lower()}-{index:02d}"
            target = audio_dir / f"{name}.wav"
            if not target.exists():
                aiff = audio_dir / f"{name}.aiff"
                subprocess.run(["say", "-v", voice, "-r", "175", "-o", str(aiff), command], check=True)
                subprocess.run(["afconvert", str(aiff), str(target), "-f", "WAVE", "-d", "LEI16@16000", "-c", "1"], check=True)
                aiff.unlink()
            clips.append({"id": name, "path": f"audio/{name}.wav", "reference": command,
                          "category": f"commands-{voice.lower()}"})
            if voice == "Rishi":
                with wave.open(str(target), "rb") as audio:
                    assert audio.getnchannels() == 1 and audio.getsampwidth() == 2 and audio.getframerate() == 16000
                    samples = array.array("h", audio.readframes(audio.getnframes()))
                if sys.byteorder != "little":
                    samples.byteswap()
                rms = math.sqrt(sum(x*x for x in samples) / len(samples))
                rng = random.Random(index)
                # Deterministic 12 dB SNR white noise stress test, not a real room simulation.
                noise_sd = rms / (10 ** (12 / 20))
                noisy = array.array("h", (max(-32768, min(32767, round(x + rng.gauss(0, noise_sd)))) for x in samples))
                if sys.byteorder != "little":
                    noisy.byteswap()
                noise_name = name + "-noise"
                with wave.open(str(audio_dir / f"{noise_name}.wav"), "wb") as audio:
                    audio.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
                    audio.writeframes(noisy.tobytes())
                clips.append({"id": noise_name, "path": f"audio/{noise_name}.wav", "reference": command,
                              "category": "commands-rishi-noise"})
    for clip in clips:
        clip["sha256"] = hashlib.sha256((directory / clip["path"]).read_bytes()).hexdigest()
    (directory / "manifest.json").write_text(json.dumps(clips, indent=2) + "\n")
    (directory / "provenance.json").write_text(json.dumps({"dataset": DATASET, "revision": REVISION,
        "humanSpeakers": list(selected), "voices": VOICES, "noiseSNRdB": 12,
        "limitations": "Public read speech and generated commands; no user voice or microphone capture."}, indent=2) + "\n")
    print(f"Prepared {len(clips)} clips in {directory}")


def words(text):
    # Case and punctuation do not affect scoring. Contractions remain one word.
    return re.findall(r"[a-z0-9]+", text.lower().replace("’", "'").replace("'", ""))


def distance(reference, hypothesis):
    previous = list(range(len(hypothesis) + 1))
    for i, expected in enumerate(reference, 1):
        row = [i]
        for j, actual in enumerate(hypothesis, 1):
            row.append(min(row[-1] + 1, previous[j] + 1, previous[j-1] + (expected != actual)))
        previous = row
    return previous[-1]


def percentile(values, fraction):
    return sorted(values)[max(0, math.ceil(len(values) * fraction) - 1)]


def metrics(rows):
    errors = sum(distance(words(r["reference"]), words(r["transcript"])) for r in rows)
    word_count = sum(len(words(r["reference"])) for r in rows)
    times = [r["transcriptionSeconds"] for r in rows]
    return {"measurements": len(rows), "clips": len({r["id"] for r in rows}),
            "wordErrors": errors, "referenceWords": word_count, "wer": errors / word_count,
            "exactMatchRate": sum(words(r["reference"]) == words(r["transcript"]) for r in rows) / len(rows),
            "failures": sum(r.get("error") is not None for r in rows),
            "medianSeconds": statistics.median(times), "p95Seconds": percentile(times, .95),
            "maxSeconds": max(times)}


def summarize(paths):
    reports = [json.loads(p.read_text()) for p in paths]
    if len({r["model"] for r in reports}) != len(reports):
        raise ValueError("Use one report per model; keep separate trials separate.")
    expected = {(r["id"], r["repetition"], r["reference"], r["category"]) for r in reports[0]["measurements"]}
    summaries = {}
    for report in reports:
        if (report.get("padsShortAudio", False) != reports[0].get("padsShortAudio", False)
                or report["manifestSHA256"] != reports[0]["manifestSHA256"]
                or report["decodingOptions"] != reports[0]["decodingOptions"]):
            raise ValueError("Reports must use identical audio manifests and decoding options.")
        rows = report["measurements"]
        keys = {(r["id"], r["repetition"], r["reference"], r["category"]) for r in rows}
        if keys != expected or len(keys) != len(rows):
            raise ValueError("Reports must contain the same clips and repetitions, without duplicates.")
        if len(rows) != len({r["id"] for r in rows}) * report["repetitions"]:
            raise ValueError("Report is incomplete.")
        groups = {category: metrics([r for r in rows if r["category"] == category])
                  for category in sorted({r["category"] for r in rows})}
        groups["commands-all"] = metrics([r for r in rows if r["category"].startswith("commands-")])
        groups["all"] = metrics(rows)
        summaries[report["model"]] = {"loadSeconds": report["modelLoadSeconds"],
            "firstDecodeSeconds": report["firstDecodeSeconds"], "groups": groups}
    return summaries


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    prep = sub.add_parser("prepare")
    prep.add_argument("directory", type=pathlib.Path)
    score = sub.add_parser("summarize")
    score.add_argument("reports", type=pathlib.Path, nargs="+")
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args.directory)
    else:
        print(json.dumps(summarize(args.reports), indent=2))
