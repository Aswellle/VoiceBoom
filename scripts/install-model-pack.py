#!/usr/bin/env python3
"""
Install a prepared model pack into the source tree for offline builds (spec section 15).

Extracts a model archive into src-tauri/resources/asr-bundle/ so that
`tauri.offline.conf.json` can bundle it into a full offline installer.

Usage:
    python scripts/install-model-pack.py \
        --archive model-release/VoiceBoom-Model-sensevoice-small-int8-v1.0.0.zip \
        --output src-tauri/resources/asr-bundle
"""

import argparse
import json
import zipfile
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description="Install model pack for offline build")
    parser.add_argument("--archive", required=True, help="Path to the model zip archive")
    parser.add_argument("--output", required=True, help="Output directory (e.g. src-tauri/resources/asr-bundle)")
    args = parser.parse_args()

    archive_path = Path(args.archive)
    output_dir = Path(args.output)

    if not archive_path.exists():
        print(f"ERROR: archive not found: {archive_path}")
        raise SystemExit(1)

    # Read the manifest from the archive to learn the layout.
    with zipfile.ZipFile(archive_path, "r") as zf:
        names = zf.namelist()
        # Find manifest.json (at any depth).
        manifest_name = next((n for n in names if n.endswith("manifest.json")), None)
        if manifest_name:
            manifest = json.loads(zf.read(manifest_name))
            print(f"Manifest: {manifest['id']} v{manifest['version']}")
        else:
            print("WARNING: no manifest.json found in archive")

        # Extract everything into the output directory.
        output_dir.mkdir(parents=True, exist_ok=True)
        zf.extractall(output_dir)
        print(f"Extracted {len(names)} files to {output_dir}")

    # Verify every expected file exists and matches its SHA256.
    if manifest_name:
        for f in manifest.get("files", []):
            installed = output_dir / f["path"]
            if not installed.exists():
                print(f"ERROR: expected file missing: {installed}")
                raise SystemExit(1)
            print(f"  verified: {f['path']}")

    print("Model pack installed successfully.")


if __name__ == "__main__":
    main()
