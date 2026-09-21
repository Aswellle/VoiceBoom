#!/usr/bin/env python3
"""Fetch the sherpa-onnx native archive for the current platform.

Resolves the archive from config/sherpa-onnx.json, so the version, file name
and download sources live in exactly one place — shared with
scripts/prepare-sherpa-onnx.ps1.

On success the archive is present in the cache and its absolute directory is
printed (also exported to $GITHUB_ENV when running in CI). Consumers set
SHERPA_ONNX_ARCHIVE_DIR to that path; sherpa-onnx-sys then extracts and
resolves the native libraries itself, so nothing here depends on the archive's
internal layout.

Only the Python standard library is used.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_CONFIG = REPO_ROOT / "config" / "sherpa-onnx.json"
DEFAULT_CACHE = REPO_ROOT / ".cache" / "sherpa-onnx"
RETRY_DELAYS = (2, 5, 10)


def log(msg: str) -> None:
    print(f"[sherpa] {msg}")


def fail(msg: str) -> None:
    print(f"[sherpa] ERROR: {msg}", file=sys.stderr)
    sys.exit(1)


def detect_platform() -> str:
    """Map the host to a key in config/sherpa-onnx.json."""
    system = platform.system().lower()
    machine = platform.machine().lower()
    arch = "arm64" if machine in ("arm64", "aarch64") else "x64"

    if system == "windows":
        return f"windows-{arch}"
    if system == "darwin":
        return f"macos-{arch}"
    return f"linux-{arch}"


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def is_valid_sha256(value: str) -> bool:
    """True only for a real 64-hex-digit digest.

    Placeholder markers such as PLACEHOLDER_* or UNVERIFIED must not be
    treated as an expected checksum.
    """
    return len(value) == 64 and all(c in "0123456789abcdef" for c in value.lower())


def publish(archive_dir: Path) -> None:
    """Export the archive directory for this process and any later CI step."""
    resolved = str(archive_dir.resolve())
    os.environ["SHERPA_ONNX_ARCHIVE_DIR"] = resolved

    github_env = os.environ.get("GITHUB_ENV")
    if github_env:
        with open(github_env, "a", encoding="utf-8") as handle:
            handle.write(f"SHERPA_ONNX_ARCHIVE_DIR={resolved}\n")

    log(f"SHERPA_ONNX_ARCHIVE_DIR={resolved}")
    print(resolved)


def download(url: str, dest: Path) -> None:
    """Download url to dest, writing through a .part file."""
    part = dest.with_suffix(dest.suffix + ".part")
    with urllib.request.urlopen(url, timeout=300) as response, part.open("wb") as out:
        while True:
            chunk = response.read(1 << 20)
            if not chunk:
                break
            out.write(chunk)
    part.replace(dest)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", default="", help="override detected platform key")
    parser.add_argument("--cache-dir", default=str(DEFAULT_CACHE))
    parser.add_argument("--config", default=str(DEFAULT_CONFIG))
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()

    config_path = Path(args.config)
    if not config_path.is_file():
        fail(f"configuration file not found: {config_path}")

    config = json.loads(config_path.read_text(encoding="utf-8"))
    version = config["version"]

    platform_key = args.platform or detect_platform()
    platform_config = config.get("platforms", {}).get(platform_key)
    if not platform_config:
        fail(f"platform '{platform_key}' not found in {config_path}")

    archive_name = platform_config["archive"]
    expected = platform_config.get("sha256", "")
    sources = platform_config.get("sources", [])
    has_hash = is_valid_sha256(expected)

    log(f"Platform: {platform_key}")
    log(f"Sherpa-onnx version: {version}")
    log(f"Archive: {archive_name}")

    archive_dir = Path(args.cache_dir).resolve() / version / platform_key
    archive_path = archive_dir / archive_name

    # 1. An externally supplied directory wins.
    external = os.environ.get("SHERPA_ONNX_ARCHIVE_DIR")
    if external and (Path(external) / archive_name).is_file():
        log("Using existing SHERPA_ONNX_ARCHIVE_DIR")
        publish(Path(external))
        return 0

    # 2. Local cache.
    if archive_path.is_file():
        if not has_hash:
            log("Cache hit (no expected hash configured)")
            publish(archive_dir)
            return 0
        if sha256_of(archive_path) == expected.lower():
            log("Cache hit (hash verified)")
            publish(archive_dir)
            return 0
        log("Cached archive failed hash check; discarding")
        archive_path.unlink()

    # 3. Offline guard.
    offline = args.offline or os.environ.get("VOICEBOOM_OFFLINE") == "1"
    if offline:
        fail(
            "Required sherpa-onnx archive is missing from the local cache and "
            "offline mode is enabled.\n"
            f"Expected: {archive_path}\n"
            "Run the bootstrap once on a network-enabled machine to populate it."
        )

    if not sources:
        fail(f"no download sources configured for '{platform_key}'")

    archive_dir.mkdir(parents=True, exist_ok=True)

    failures: list[str] = []
    for source in sources:
        for attempt, delay in enumerate((0, *RETRY_DELAYS), start=1):
            if delay:
                time.sleep(delay)
            log(f"Download attempt {attempt} from {source}")
            try:
                download(source, archive_path)

                if has_hash:
                    actual = sha256_of(archive_path)
                    if actual != expected.lower():
                        archive_path.unlink()
                        raise ValueError(
                            f"SHA-256 mismatch (expected {expected}, got {actual})"
                        )
                    log(f"Hash verified: {actual}")

                publish(archive_dir)
                return 0
            except (urllib.error.URLError, OSError, ValueError) as exc:
                log(f"Failed: {exc}")
                archive_path.unlink(missing_ok=True)

        failures.append(f"{source} -> last error above")

    fail("All download sources failed:\n" + "\n".join(failures))
    return 1


if __name__ == "__main__":
    sys.exit(main())
