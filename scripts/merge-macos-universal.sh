#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# scripts/merge-macos-universal.sh
#
# Merges two macOS .app bundles (arm64 + x86_64) into a single Universal
# bundle.  Reads CFBundleExecutable from Info.plist, lipos the main
# executable, scans for other Mach-O files, and validates the result.
#
# Usage:
#   ./scripts/merge-macos-universal.sh <arm64.app> <x64.app> <output.app>
#
# Contract:
#   - Both inputs must be valid .app bundles with Contents/Info.plist
#   - CFBundleExecutable must match in both bundles
#   - Output directory must not exist yet (will be created)
# ---------------------------------------------------------------------------
set -euo pipefail

ARM_APP="${1:?Usage: $0 <arm64.app> <x64.app> <output.app>}"
X64_APP="${2:?Usage: $0 <arm64.app> <x64.app> <output.app>}"
OUT_APP="${3:?Usage: $0 <arm64.app> <x64.app> <output.app>}"

echo "=== macOS Universal Merge ==="
echo "ARM64:  $ARM_APP"
echo "X86_64: $X64_APP"
echo "Output: $OUT_APP"

# ── 1. Validate both bundles exist ──────────────────────────────────────
for APP in "$ARM_APP" "$X64_APP"; do
    if [[ ! -d "$APP" ]]; then
        echo "ERROR: Bundle not found: $APP"
        exit 1
    fi
    if [[ ! -f "$APP/Contents/Info.plist" ]]; then
        echo "ERROR: Missing Info.plist in $APP"
        exit 1
    fi
    if [[ ! -d "$APP/Contents/MacOS" ]]; then
        echo "ERROR: Missing Contents/MacOS in $APP"
        exit 1
    fi
done

# ── 2. Read CFBundleExecutable ─────────────────────────────────────────
ARM_EXEC=$(/usr/libexec/PlistBuddy -c "Print :CFBundleExecutable" \
    "$ARM_APP/Contents/Info.plist" 2>/dev/null)
X64_EXEC=$(/usr/libexec/PlistBuddy -c "Print :CFBundleExecutable" \
    "$X64_APP/Contents/Info.plist" 2>/dev/null)

if [[ -z "$ARM_EXEC" ]]; then
    echo "ERROR: CFBundleExecutable not found in arm64 Info.plist"
    exit 1
fi
if [[ -z "$X64_EXEC" ]]; then
    echo "ERROR: CFBundleExecutable not found in x86_64 Info.plist"
    exit 1
fi
if [[ "$ARM_EXEC" != "$X64_EXEC" ]]; then
    echo "ERROR: Executable name mismatch: arm64=$ARM_EXEC x86_64=$X64_EXEC"
    exit 1
fi

EXECUTABLE="$ARM_EXEC"
echo "Executable: $EXECUTABLE"

ARM_BIN="$ARM_APP/Contents/MacOS/$EXECUTABLE"
X64_BIN="$X64_APP/Contents/MacOS/$EXECUTABLE"

if [[ ! -f "$ARM_BIN" ]]; then
    echo "ERROR: arm64 binary not found: $ARM_BIN"
    exit 1
fi
if [[ ! -f "$X64_BIN" ]]; then
    echo "ERROR: x86_64 binary not found: $X64_BIN"
    exit 1
fi

# ── 3. Verify thin architectures ───────────────────────────────────────
echo "=== ARM64 binary ==="
file "$ARM_BIN"
lipo -info "$ARM_BIN" || true

echo "=== X86_64 binary ==="
file "$X64_BIN"
lipo -info "$X64_BIN" || true

# ── 4. Create output bundle ────────────────────────────────────────────
if [[ -e "$OUT_APP" ]]; then
    echo "ERROR: Output already exists: $OUT_APP"
    exit 1
fi

cp -R "$ARM_APP" "$OUT_APP"
OUT_BIN="$OUT_APP/Contents/MacOS/$EXECUTABLE"

# ── 5. Lipo main executable ────────────────────────────────────────────
lipo -create "$ARM_BIN" "$X64_BIN" -output "$OUT_BIN"

echo "=== Universal binary ==="
file "$OUT_BIN"
lipo -info "$OUT_BIN"

# ── 6. Scan for other Mach-O files and lipo pairs ──────────────────────
echo "=== Scanning for additional Mach-O files ==="
while IFS= read -r -d '' ARM_FILE; do
    # Compute relative path from .app root
    REL="${ARM_FILE#$ARM_APP/}"
    X64_FILE="$X64_APP/$REL"
    OUT_FILE="$OUT_APP/$REL"

    # Skip the main executable (already handled)
    if [[ "$OUT_FILE" == "$OUT_BIN" ]]; then
        continue
    fi

    if [[ -f "$X64_FILE" ]]; then
        FILE_TYPE=$(file "$ARM_FILE")
        if echo "$FILE_TYPE" | grep -q "Mach-O"; then
            echo "Merging: $REL"
            lipo -create "$ARM_FILE" "$X64_FILE" -output "$OUT_FILE"
        fi
    fi
done < <(find "$ARM_APP" -type f -print0)

# ── 7. Validate output ─────────────────────────────────────────────────
echo "=== Validation ==="
if [[ ! -f "$OUT_BIN" ]]; then
    echo "ERROR: Universal binary missing"
    exit 1
fi

ARCHES=$(lipo -archs "$OUT_BIN")
echo "Architectures: $ARCHES"

if ! echo "$ARCHES" | grep -qw "arm64"; then
    echo "ERROR: arm64 architecture missing from universal binary"
    exit 1
fi
if ! echo "$ARCHES" | grep -qw "x86_64"; then
    echo "ERROR: x86_64 architecture missing from universal binary"
    exit 1
fi

echo "=== macOS Universal Merge Complete ==="
echo "Output: $OUT_APP"
echo "Architectures: $ARCHES"
