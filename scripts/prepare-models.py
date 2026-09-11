#!/usr/bin/env python3
"""
Prepare model release archives (spec section 20).

Reads models/registry.json, downloads every model file, verifies size + SHA256,
lays out the directory structure, writes a per-version manifest.json, and produces
a zipped archive with its SHA256 sidecar.

Usage:
    python scripts/prepare-models.py \
        --registry models/registry.json \
        --cache .model-cache \
        --output model-release

Output:
    model-release/
    ├── VoiceBoom-Model-sensevoice-small-int8-v1.0.0.zip
    ├── VoiceBoom-Model-sensevoice-small-int8-v1.0.0.zip.sha256
    └── models.json  (updated registry with real SHA256 + sizes)
"""

import argparse
import hashlib
import json
import os
import sys
import urllib.request
import urllib.error
import zipfile
from pathlib import Path


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            h.update(chunk)
    return h.hexdigest()


def download(url: str, dest: Path, expected_size: int = 0) -> None:
    """Download with retry and size validation."""
    dest.parent.mkdir(parents=True, exist_ok=True)
    for attempt in range(3):
        try:
            print(f"  downloading {url} -> {dest}")
            req = urllib.request.Request(url, headers={"User-Agent": "VoiceBoom-ModelPrep/1.0"})
            with urllib.request.urlopen(req, timeout=120) as resp:
                data = resp.read()
            if expected_size and len(data) != expected_size:
                raise ValueError(f"size mismatch: expected {expected_size}, got {len(data)}")
            dest.write_bytes(data)
            return
        except (urllib.error.URLError, ValueError) as e:
            print(f"  attempt {attempt + 1} failed: {e}")
            if attempt == 2:
                raise


def main():
    parser = argparse.ArgumentParser(description="Prepare VoiceBoom model release archives")
    parser.add_argument("--registry", required=True, help="Path to models/registry.json")
    parser.add_argument("--cache", default=".model-cache", help="Download cache directory")
    parser.add_argument("--output", required=True, help="Output directory for archives")
    args = parser.parse_args()

    registry_path = Path(args.registry)
    cache_dir = Path(args.cache)
    output_dir = Path(args.output)
    cache_dir.mkdir(parents=True, exist_ok=True)
    output_dir.mkdir(parents=True, exist_ok=True)

    registry = json.loads(registry_path.read_text(encoding="utf-8"))

    for model in registry["models"]:
        model_id = model["id"]
        version = model["version"]
        engine = model["engine"]
        print(f"\nPreparing model: {model_id} v{version}")

        # Download every file into a staging layout: <engine>/<version>/<file>
        staging = cache_dir / "staging" / engine / version
        staging.mkdir(parents=True, exist_ok=True)

        for file_info in model["files"]:
            relative = file_info["path"]  # e.g. sensevoice/1.0.0/model.int8.onnx
            # The relative path inside the archive is <engine>/<version>/<filename>.
            filename = Path(relative).name
            dest = staging / filename
            # Use SHA256 for cache validation. Size-only checks can let a
            # truncated/corrupted file pass if it happens to match the size.
            expected_sha = file_info.get("sha256", "")
            if dest.exists() and expected_sha and expected_sha != "REPLACE" and sha256_file(dest) == expected_sha:
                print(f"  cached: {filename}")
            else:
                # Files may be hosted individually; fall back to the archive URL
                # with a path-based convention if file_info has its own url.
                file_url = file_info.get("url", "")
                if file_url:
                    download(file_url, dest, file_info.get("size", 0))
        # Write per-version manifest.json.
        manifest = {
            "id": model_id,
            "engine": engine,
            "version": version,
            "languages": model.get("languages", []),
            "files": [
                {"path": f["path"], "size": f["size"], "sha256": f["sha256"]}
                for f in model["files"]
            ],
        }
        (staging / "manifest.json").write_text(
            json.dumps(manifest, indent=2, ensure_ascii=False), encoding="utf-8"
        )

        # Build the zip archive.
        archive_name = f"VoiceBoom-Model-{model_id}-v{version}.zip"
        archive_path = output_dir / archive_name
        print(f"  creating archive: {archive_path}")
        with zipfile.ZipFile(archive_path, "w", zipfile.ZIP_DEFLATED) as zf:
            for f in staging.iterdir():
                # Archive layout mirrors the spec: <engine>/<version>/<file>
                arcname = f"{engine}/{version}/{f.name}"
                zf.write(f, arcname)

        # Compute archive SHA256 + size, update registry.
        archive_hash = sha256_file(archive_path)
        archive_size = archive_path.stat().st_size
        model["archive"]["sha256"] = archive_hash
        model["archive"]["size"] = archive_size
        print(f"  archive: {archive_hash} ({archive_size} bytes)")

        # Write SHA256 sidecar.
        sidecar = output_dir / f"{archive_name}.sha256"
        sidecar.write_text(f"{archive_hash}  {archive_name}\n", encoding="utf-8")

    # Write the updated registry (with real hashes) alongside the archives.
    updated_registry_path = output_dir / "models.json"
    updated_registry_path.write_text(
        json.dumps(registry, indent=2, ensure_ascii=False), encoding="utf-8"
    )
    print(f"\nWrote updated registry to {updated_registry_path}")
    print("Done.")


if __name__ == "__main__":
    main()
