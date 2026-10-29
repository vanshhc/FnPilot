#!/usr/bin/env python3
"""Opt-in five-minute attributed process measurement; never executes desktop actions."""
import argparse
import json
import time
from pathlib import Path


def measure(pids, seconds, interval, attribution_complete=False, models_unloaded=False, settings_closed=False, hardware=None):
    try:
        import psutil
    except ImportError:
        raise SystemExit("Install psutil in an explicit benchmark environment first; nothing is downloaded automatically.")
    roots = [psutil.Process(pid) for pid in pids]
    tracked = {}
    samples = []
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        processes = {p.pid: p for root in roots for p in [root, *root.children(recursive=True)]}
        resident = 0
        cpu = 0
        for pid, process in processes.items():
            if pid not in tracked:
                process.cpu_percent(None)
                tracked[pid] = process
            resident += process.memory_info().rss
            cpu += tracked[pid].cpu_percent(None)
        samples.append({"monotonic_seconds": time.monotonic(), "resident_bytes": resident, "cpu_percent_one_logical_core": cpu, "process_count": len(processes)})
        time.sleep(min(interval, max(0, deadline - time.monotonic())))
    qualified = seconds >= 300 and len(samples) > 1 and attribution_complete and models_unloaded and settings_closed and bool(hardware)
    # The first CPU sample establishes the measurement interval and cannot be a pass.
    average_cpu = sum(s["cpu_percent_one_logical_core"] for s in samples[1:]) / max(1, len(samples) - 1)
    peak_memory = max((s["resident_bytes"] for s in samples), default=None)
    return {"schema_version": 1, "duration_seconds": seconds, "idle_budget_qualified": qualified,
            "hardware": hardware, "attribution_complete": attribution_complete, "models_unloaded": models_unloaded, "settings_closed": settings_closed,
            "passes_sc4": qualified and peak_memory is not None and peak_memory <= 150 * 1024 * 1024 and average_cpu <= 1,
            "peak_resident_bytes": peak_memory, "average_cpu_percent_one_logical_core": average_cpu,
            "attribution_blind_spots": "Explicitly include attributed WebView PIDs. Shared Core ML/OS service allocations may be missing; record those separately.",
            "samples": samples}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pid", type=int, action="append", required=True)
    parser.add_argument("--seconds", type=float, default=300)
    parser.add_argument("--interval", type=float, default=1)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--attribution-complete", action="store_true")
    parser.add_argument("--models-unloaded", action="store_true")
    parser.add_argument("--settings-closed", action="store_true")
    parser.add_argument("--hardware")
    args = parser.parse_args()
    if args.seconds <= 0 or args.interval <= 0:
        parser.error("durations must be positive")
    args.output.write_text(json.dumps(measure(args.pid, args.seconds, args.interval, args.attribution_complete, args.models_unloaded, args.settings_closed, args.hardware), indent=2) + "\n")
