# prepare-sherpa-onnx.ps1
# Unified sherpa-onnx native library bootstrap for VoiceBoom.
#
# Resolves the platform archive from config/sherpa-onnx.json, ensures it is
# present in the local cache (downloading and verifying if needed), and
# publishes SHERPA_ONNX_ARCHIVE_DIR for sherpa-onnx-sys' build script.
#
# The published path is ALWAYS absolute: consumers such as `cargo clippy`
# run from src-tauri/, so a relative path would resolve against the wrong
# directory.

param(
    [switch]$Offline,
    [string]$CacheDir = ".cache/sherpa-onnx",
    [string]$ConfigFile = "config/sherpa-onnx.json",
    [string]$Platform = ""
)

$ErrorActionPreference = "Stop"

function Write-Status($msg) { Write-Host "[sherpa] $msg" }
function Fail($msg) {
    Write-Host "[sherpa] ERROR: $msg" -ForegroundColor Red
    exit 1
}
function Write-Warn($msg) { Write-Host "[sherpa] WARN: $msg" -ForegroundColor Yellow }

# Publish the archive directory to this process AND to any subsequent CI step.
function Publish-ArchiveDir([string]$dir) {
    $env:SHERPA_ONNX_ARCHIVE_DIR = $dir
    if ($env:GITHUB_ENV) {
        Add-Content -Path $env:GITHUB_ENV -Value "SHERPA_ONNX_ARCHIVE_DIR=$dir"
    }
    Write-Status "SHERPA_ONNX_ARCHIVE_DIR=$dir"
}

# --- Resolve inputs -------------------------------------------------------

$repoRoot = Split-Path -Parent $PSScriptRoot
$configPath = if ([System.IO.Path]::IsPathRooted($ConfigFile)) {
    $ConfigFile
} else {
    Join-Path $repoRoot $ConfigFile
}

if (-not (Test-Path $configPath)) {
    Fail "Configuration file not found: $configPath"
}

$cacheRoot = if ([System.IO.Path]::IsPathRooted($CacheDir)) {
    $CacheDir
} else {
    Join-Path $repoRoot $CacheDir
}

$config = Get-Content $configPath -Raw | ConvertFrom-Json
$version = $config.version

# --- Resolve platform -----------------------------------------------------

if (-not $Platform) {
    $arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "arm64" } else { "x64" }
    $Platform = "windows-$arch"
}

Write-Status "Platform: $Platform"
Write-Status "Sherpa-onnx version: $version"

$platformConfig = $config.platforms.$Platform
if (-not $platformConfig) {
    Fail "Platform '$Platform' not found in $configPath"
}

$archiveName = $platformConfig.archive
$expectedSha256 = $platformConfig.sha256
$sources = @($platformConfig.sources)
$hasHash = $expectedSha256 -and ($expectedSha256 -notlike "PLACEHOLDER*")

Write-Status "Archive: $archiveName"

# --- Resolve the cache location (absolute) --------------------------------

$cacheSubDir = Join-Path (Join-Path $cacheRoot $version) $Platform
$cachePath = Join-Path $cacheSubDir $archiveName

# 1. An externally supplied archive directory wins.
if ($env:SHERPA_ONNX_ARCHIVE_DIR) {
    $external = Join-Path $env:SHERPA_ONNX_ARCHIVE_DIR $archiveName
    if (Test-Path $external) {
        Write-Status "Using existing SHERPA_ONNX_ARCHIVE_DIR"
        Publish-ArchiveDir ([System.IO.Path]::GetFullPath($env:SHERPA_ONNX_ARCHIVE_DIR))
        exit 0
    }
}

# 2. Local cache.
if (Test-Path $cachePath) {
    if ($hasHash) {
        $actual = (Get-FileHash $cachePath -Algorithm SHA256).Hash.ToLower()
        if ($actual -eq $expectedSha256.ToLower()) {
            Write-Status "Cache hit (hash verified)"
            Publish-ArchiveDir $cacheSubDir
            exit 0
        }
        Write-Warn "Cached archive failed hash check; discarding"
        Remove-Item $cachePath -Force
    } else {
        Write-Status "Cache hit (no expected hash configured)"
        Publish-ArchiveDir $cacheSubDir
        exit 0
    }
}

# --- Offline guard --------------------------------------------------------

$offlineMode = $Offline -or ($env:VOICEBOOM_OFFLINE -eq "1")
if ($offlineMode) {
    Fail @"
Required sherpa-onnx archive is missing from the local cache and offline
mode is enabled.
Expected: $cachePath
Run the bootstrap once on a network-enabled machine to populate the cache.
"@
}

# --- Download -------------------------------------------------------------

if ($sources.Count -eq 0) {
    Fail "No download sources configured for '$Platform'"
}

New-Item -ItemType Directory -Force -Path $cacheSubDir | Out-Null
$tempPath = "$cachePath.part"

$downloaded = $false
$failures = @()

foreach ($source in $sources) {
    for ($attempt = 1; $attempt -le 3; $attempt++) {
        Write-Status "Download attempt $attempt from $source"

        try {
            $ProgressPreference = 'SilentlyContinue'
            Invoke-WebRequest -Uri $source -OutFile $tempPath -UseBasicParsing -TimeoutSec 300

            if (-not (Test-Path $tempPath)) {
                throw "download produced no file"
            }

            if ($hasHash) {
                $actual = (Get-FileHash $tempPath -Algorithm SHA256).Hash.ToLower()
                if ($actual -ne $expectedSha256.ToLower()) {
                    throw "SHA-256 mismatch (expected $expectedSha256, got $actual)"
                }
                Write-Status "Hash verified: $actual"
            }

            # Atomic publish: only a verified archive becomes the real file.
            Move-Item -Path $tempPath -Destination $cachePath -Force
            $downloaded = $true
            break
        } catch {
            Write-Warn "Failed: $_"
            if (Test-Path $tempPath) { Remove-Item $tempPath -Force }

            if ($attempt -lt 3) {
                Start-Sleep -Seconds (2 * $attempt)
            } else {
                $failures += "$source -> $_"
            }
        }
    }

    if ($downloaded) { break }
}

if (-not $downloaded) {
    Fail ("All download sources failed:`n" + ($failures -join "`n"))
}

Write-Status "Downloaded to $cachePath"
Publish-ArchiveDir $cacheSubDir
Write-Status "Done"
