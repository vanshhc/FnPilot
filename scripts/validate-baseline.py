#!/usr/bin/env python3
"""Validate frozen migration report provenance without rebuilding the retired Swift app."""
import argparse
import hashlib
import json
from pathlib import Path


def validate_report(report, corpus):
    if report.get("corpus_sha256") != hashlib.sha256(corpus).hexdigest():
        raise ValueError("corpus hash mismatch")
    if not report.get("revision") or not report.get("configuration"):
        raise ValueError("missing provenance")
    if report.get("measurement_status") == "measured":
        if report.get("paired_runs") != [1, 2, 3] or not report.get("hardware"):
            raise ValueError("three paired runs and hardware are required")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("corpus", type=Path)
    args = parser.parse_args()
    validate_report(json.loads(args.report.read_text()), args.corpus.read_bytes())
