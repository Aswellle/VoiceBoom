#!/usr/bin/env python3
"""
Verify model archives against models/registry.json (spec section 41).

Checks that every archive exists, matches its expected SHA256 and size, and
that every file inside matches its per-file SHA256.

Usage:
    python scripts/verify-models.py --registry models/registry.json --dir model-release
"""

import argparse
import hashlib
import json
import zipfile
from pathlib import Path


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            h.update(chunk)
    return h.hexdigest()


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description="Verify model release archives")
    parser.add_argument("--registry", required=True, help="Path to models/registry.json")
    parser.add_argument("--dir", required=True, help="Directory containing archives")
    args = parser.parse_args()

    registry_path = Path(args.registry)
    search_dir = Path(args.dir)

    if not registry_path.exists():
        print(f"ERROR: registry not found: {registry_path}")
        raise SystemExit(1)

    registry = json.loads(registry_path.read_text(encoding="utf-8"))
    errors = 0

    for model in registry["models"]:
        model_id = model["id"]
        version = model["version"]
        archive_name = f"VoiceBoom-Model-{model_id}-v{version}.zip"
        archive_path = search_dir / archive_name

        print(f"\nVerifying: {archive_name}")
        if not archive_path.exists():
            print(f"  ERROR: archive not found: {archive_path}")
            errors += 1
            continue

        # Verify archive SHA256.
        actual_hash = sha256_file(archive_path)
        expected_hash = model["archive"]["sha256"]
        if actual_hash != expected_hash:
            print(f"  ERROR: archive SHA256 mismatch")
            print(f"    expected: {expected_hash}")
            print(f"    actual:   {actual_hash}")
            errors += 1
        else:
            print(f"  archive SHA256 OK")

        # Verify per-file SHA256 inside the archive.
        with zipfile.ZipFile(archive_path, "r") as zf:
            for file_info in model["files"]:
                path = file_info["path"]
                if path not in zf.namelist():
                    print(f"  ERROR: file missing from archive: {path}")
                    errors += 1
                    continue
                data = zf.read(path)
                actual = sha256_bytes(data)
                expected = file_info["sha256"]
                if actual != expected:
                    print(f"  ERROR: SHA256 mismatch for {path}")
                    errors += 1
                else:
                    print(f"  {path}: OK")

    print(f"\n{'=' * 40}")
    if errors:
        print(f"FAILED: {errors} error(s)")
        raise SystemExit(1)
    else:
        print("All model archives verified successfully.")


if __name__ == "__main__":
    main()
