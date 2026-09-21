#!/usr/bin/env python3
"""
Install a prepared model pack into the source tree for offline builds (spec section 15).

Extracts a model archive into src-tauri/resources/asr-bundle/ so that
`tauri.offline.conf.json` can bundle it into a full offline installer.

Usage:
    python scripts/install-model-pack.py \\
        --archive model-release/VoiceBoom-Model-sensevoice-small-int8-v1.0.0.zip \\
        --output src-tauri/resources/asr-bundle \\
        --archive-sha256 <hash-from-registry>
"""

import argparse
import hashlib
import json
import zipfile
from pathlib import Path


def sha256_file(path: Path) -> str:
    """Compute SHA256 hex digest of a file."""
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def safe_extract(zip_file: zipfile.ZipFile, output_dir: Path) -> None:
    """
    Extract all members from zip_file into output_dir, rejecting any
    entry whose resolved path escapes the output directory (Zip Slip).
    """
    output_resolved = output_dir.resolve()

    for member in zip_file.infolist():
        # Resolve the target path and ensure it stays under output_dir
        target = (output_resolved / member.filename).resolve()

        try:
            target.relative_to(output_resolved)
        except ValueError:
            print(f"ERROR: zip entry escapes output directory: {member.filename}")
            raise SystemExit(1)

        if member.is_dir():
            target.mkdir(parents=True, exist_ok=True)
        else:
            target.parent.mkdir(parents=True, exist_ok=True)
            with zip_file.open(member) as src, target.open("wb") as dst:
                for chunk in iter(lambda: src.read(65536), b""):
                    dst.write(chunk)


def main():
    parser = argparse.ArgumentParser(description="Install model pack for offline build")
    parser.add_argument("--archive", required=True, help="Path to the model zip archive")
    parser.add_argument("--output", required=True, help="Output directory (e.g. src-tauri/resources/asr-bundle)")
    parser.add_argument("--archive-sha256", default=None, help="Expected SHA256 of the archive (from registry)")
    args = parser.parse_args()

    archive_path = Path(args.archive)
    output_dir = Path(args.output)

    if not archive_path.exists():
        print(f"ERROR: archive not found: {archive_path}")
        raise SystemExit(1)

    # Verify the archive's own SHA256 if provided (e.g. from registry).
    if args.archive_sha256:
        actual = sha256_file(archive_path)
        if actual != args.archive_sha256:
            print("ERROR: archive SHA256 mismatch")
            print(f"  expected: {args.archive_sha256}")
            print(f"  actual:   {actual}")
            raise SystemExit(1)
        print(f"  archive SHA256 verified: {actual}")

    # Read the manifest from the archive to learn the layout.
    with zipfile.ZipFile(archive_path, "r") as zf:
        names = zf.namelist()
        # Find manifest.json (at any depth).
        manifest_name = next((n for n in names if n.endswith("manifest.json")), None)
        if manifest_name:
            manifest = json.loads(zf.read(manifest_name))
            print(f"Manifest: {manifest['id']} v{manifest['version']}")
        else:
            manifest = None
            print("WARNING: no manifest.json found in archive")

        # Safely extract with Zip Slip protection.
        output_dir.mkdir(parents=True, exist_ok=True)
        safe_extract(zf, output_dir)
        print(f"Extracted {len(names)} files to {output_dir}")

    # Verify every expected file exists and matches its SHA256.
    if manifest:
        for f in manifest.get("files", []):
            installed = output_dir / f["path"]
            if not installed.exists():
                print(f"ERROR: expected file missing: {installed}")
                raise SystemExit(1)
            actual_sha = sha256_file(installed)
            expected_sha = f.get("sha256")
            if expected_sha and actual_sha != expected_sha:
                print(f"ERROR: SHA256 mismatch for {f['path']}")
                print(f"  expected: {expected_sha}")
                print(f"  actual:   {actual_sha}")
                raise SystemExit(1)
            print(f"  verified: {f['path']}")

    print("Model pack installed successfully.")


if __name__ == "__main__":
    main()
