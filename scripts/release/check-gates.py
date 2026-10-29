#!/usr/bin/env python3
"""Require maintainer-reviewed native receipts for the exact immutable release revision."""
import json
import os


def check(receipt, revision):
    if receipt.get("source_revision") != revision or not receipt.get("reviewer") or not receipt.get("reviewed_at"):
        raise ValueError("Missing exact-revision maintainer acceptance")
    for name in ["macos", "windows", "performance", "upgrade"]:
        evidence = receipt.get("receipts", {}).get(name, {})
        digest = evidence.get("sha256", "")
        if evidence.get("status") != "passed" or not evidence.get("reference") or len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
            raise ValueError(f"Missing reviewed {name} acceptance receipt")
    return True


if __name__ == "__main__":
    if os.environ.get("GITHUB_REF_TYPE") != "tag":
        raise SystemExit("Official builds require a reviewed immutable tag")
    check(json.loads(os.environ["ACCEPTANCE_RECEIPTS_JSON"]), os.environ["GITHUB_SHA"])
    print("Maintainer acceptance covers this exact tagged revision.")
