# prepare-sherpa-onnx.ps1
# Unified sherpa-onnx native library bootstrap for VoiceBoom
# Downloads, verifies, and caches sherpa-onnx archives for local development

param(
    [switch]$Offline,
    [string]$CacheDir = ".cache/sherpa-onnx",
    [string]$ConfigFile = "config/sherpa-onnx.json"
)

$ErrorActionPreference = "Stop"

function Write-Status($msg) { Write-Host "[sherpa] $msg" }
function Write-Error($msg) { Write-Host "[sherpa] ERROR: $msg" -ForegroundColor Red }
function Write-Warn($msg) { Write-Host "[sherpa] WARN: $msg" -ForegroundColor Yellow }

# Read configuration
if (-not (Test-Path $ConfigFile)) {
    Write-Error "Configuration file not found: $ConfigFile"
    exit 1
}

$config = Get-Content $ConfigFile -Raw | ConvertFrom-Json
$version = $config.version

# Detect platform and architecture
$os = "windows"
$arch = "x64"
$platformKey = "$os-$arch"

Write-Status "Platform: $platformKey"
Write-Status "Sherpa-onnx version: $version"

# Get platform config
$platformConfig = $config.platforms.$platformKey
if (-not $platformConfig) {
    Write-Error "Platform '$platformKey' not found in configuration"
    exit 1
}

$archiveName = $platformConfig.archive
$expectedSha256 = $platformConfig.sha256
$sources = $platformConfig.sources

Write-Status "Archive: $archiveName"

# Check SHERPA_ONNX_ARCHIVE_DIR first
if ($env:SHERPA_ONNX_ARCHIVE_DIR) {
    $archivePath = Join-Path $env:SHERPA_ONNX_ARCHIVE_DIR $archiveName
    if (Test-Path $archivePath) {
        Write-Status "Using SHERPA_ONNX_ARCHIVE_DIR: $($env:SHERPA_ONNX_ARCHIVE_DIR)"
        exit 0
    }
}

# Check local cache
$cacheSubDir = Join-Path $CacheDir $version $platformKey
$cachePath = Join-Path $cacheSubDir $archiveName

if (Test-Path $cachePath) {
    Write-Status "Cache hit: $cachePath"

    # Verify hash
    if ($expectedSha256 -and $expectedSha256 -ne "PLACEHOLDER_WIN_SHA256") {
        $actualHash = (Get-FileHash $cachePath -Algorithm SHA256).Hash.ToLower()
        if ($actualHash -ne $expectedSha256.ToLower()) {
            Write-Warn "Cache hash mismatch, re-downloading..."
            Remove-Item $cachePath -Force
        } else {
            Write-Status "Hash verified"
            $env:SHERPA_ONNX_ARCHIVE_DIR = $cacheSubDir
            exit 0
        }
    } else {
        $env:SHERPA_ONNX_ARCHIVE_DIR = $cacheSubDir
        exit 0
    }
}

if ($Offline) {
    Write-Error "Archive not found in local cache and offline mode is enabled"
    Write-Error "Run this script on a network-enabled machine first"
    exit 1
}

# Download from sources
$downloaded = $false
$attempt = 0
$maxAttempts = 3
$retryDelays = @(2, 5, 10)

foreach ($source in $sources) {
    for ($i = 0; $i -lt $maxAttempts; $i++) {
        $attempt++
        Write-Status "Download attempt $attempt from: $source"

        try {
            $tempFile = "$cachePath.part"

            # Ensure cache directory exists
            New-Item -ItemType Directory -Force -Path $cacheSubDir | Out-Null

            # Download with progress
            $ProgressPreference = 'SilentlyContinue'
            Invoke-WebRequest -Uri $source -OutFile $tempFile -UseBasicParsing -TimeoutSec 300

            if (-not (Test-Path $tempFile)) {
                throw "Download failed: file not created"
            }

            # Verify hash
            if ($expectedSha256 -and $expectedSha256 -ne "PLACEHOLDER_WIN_SHA256") {
                $actualHash = (Get-FileHash $tempFile -Algorithm SHA256).Hash.ToLower()
                if ($actualHash -ne $expectedSha256.ToLower()) {
                    Write-Warn "Hash mismatch: expected $expectedSha256, got $actualHash"
                    Remove-Item $tempFile -Force
                    throw "Hash verification failed"
                }
                Write-Status "Hash verified: $actualHash"
            }

            # Atomic move
            Move-Item -Path $tempFile -Destination $cachePath -Force
            $downloaded = $true
            Write-Status "Downloaded to: $cachePath"
            break
        } catch {
            Write-Warn "Download failed: $_"
            if (Test-Path $tempFile) { Remove-Item $tempFile -Force }

            if ($i -lt $maxAttempts - 1) {
                $delay = $retryDelays[$i]
                Write-Status "Retrying in ${delay}s..."
                Start-Sleep -Seconds $delay
            }
        }
    }

    if ($downloaded) { break }
}

if (-not $downloaded) {
    Write-Error "All download sources failed"
    exit 1
}

# Set environment variable for current session
$env:SHERPA_ONNX_ARCHIVE_DIR = $cacheSubDir
Write-Status "SHERPA_ONNX_ARCHIVE_DIR=$cacheSubDir"

# Output for GitHub Actions
if ($env:GITHUB_ENV) {
    Add-Content -Path $env:GITHUB_ENV -Value "SHERPA_ONNX_ARCHIVE_DIR=$cacheSubDir"
}

Write-Status "Done"
