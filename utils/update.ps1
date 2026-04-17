param(
    [Parameter(Mandatory = $true)]
    [string]$RootDir
)

$ErrorActionPreference = "Stop"

# Normalize: strip any stray quotes and trailing backslashes that bat may inject
$RootDir = $RootDir.Trim('"').TrimEnd('\')

# Paths
$configSetScript = Join-Path $RootDir "utils\config_set.ps1"
$configFile      = Join-Path $RootDir "muzap.ini"
$serviceDir      = Join-Path $RootDir ".service"
$pendingBat      = Join-Path $serviceDir "MuZap.bat.pending"

# Files that must never be overwritten in-place during a running session.
# MuZap.bat is handled separately via .service\MuZap.bat.pending.
$protectedPaths = @(
    "MuZap.bat",
    "muzap.ini",
    "lists\ipset-exclude-user.txt",
    "lists\list-general-user.txt",
    "lists\list-exclude-user.txt"
)

function Write-Ok   { param([string]$msg) Write-Host "[OK] $msg"    -ForegroundColor Green  }
function Write-Err  { param([string]$msg) Write-Host "[ERROR] $msg" -ForegroundColor Red    }
function Write-Info { param([string]$msg) Write-Host "[INFO] $msg"  -ForegroundColor Cyan   }
function Write-Warn { param([string]$msg) Write-Host "[WARN] $msg"  -ForegroundColor Yellow }

$tempZip     = $null
$tempExtract = $null

function Cleanup {
    if ($tempZip     -and (Test-Path $tempZip))     { Remove-Item $tempZip     -Force         -ErrorAction SilentlyContinue }
    if ($tempExtract -and (Test-Path $tempExtract)) { Remove-Item $tempExtract -Recurse -Force -ErrorAction SilentlyContinue }
}

# Step 1: fetch latest release info from GitHub API
Write-Info "Fetching latest release info..."
try {
    $apiUrl  = "https://api.github.com/repos/MuXolotl/MuZap/releases/latest"
    $release = Invoke-RestMethod -Uri $apiUrl -Headers @{ 'User-Agent' = 'MuZap' } -TimeoutSec 15
} catch {
    Write-Err "Failed to fetch release info: $_"
    exit 1
}

$newVersion = $release.tag_name -replace '^v', ''
if (-not $newVersion) {
    Write-Err "Could not determine new version from API response."
    exit 1
}

$asset = $release.assets | Where-Object { $_.name -like "MuZap_*.zip" } | Select-Object -First 1
if (-not $asset) {
    Write-Err "No zip asset found in the latest release."
    exit 1
}

$downloadUrl = $asset.browser_download_url
Write-Info "New version : v$newVersion"
Write-Info "Download URL: $downloadUrl"

# Step 2: download zip
$tempZip     = Join-Path $env:TEMP "MuZap_update.zip"
$tempExtract = Join-Path $env:TEMP "MuZap_update_extract"

Write-Info "Downloading archive..."
try {
    $wc = New-Object System.Net.WebClient
    $wc.Headers.Add("User-Agent", "MuZap")
    $wc.DownloadFile($downloadUrl, $tempZip)
} catch {
    Write-Err "Download failed: $_"
    Cleanup
    exit 1
}

if (-not (Test-Path $tempZip) -or (Get-Item $tempZip).Length -eq 0) {
    Write-Err "Downloaded file is missing or empty."
    Cleanup
    exit 1
}

Write-Ok "Download complete."

# Step 3: extract zip
Write-Info "Extracting archive..."
if (Test-Path $tempExtract) {
    Remove-Item $tempExtract -Recurse -Force
}

try {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::ExtractToDirectory($tempZip, $tempExtract)
} catch {
    Write-Err "Extraction failed: $_"
    Cleanup
    exit 1
}

Write-Ok "Extraction complete."

# Step 4: find root of extracted archive
# Get-Item.FullName always returns the long path — safe for Substring()
$extractedItems = Get-ChildItem -Path $tempExtract
$extractedRoot  = (Get-Item $tempExtract).FullName

if ($extractedItems.Count -eq 1 -and $extractedItems[0].PSIsContainer) {
    $extractedRoot = $extractedItems[0].FullName
}

$extractedRootNorm = $extractedRoot.TrimEnd('\') + '\'

Write-Info "Archive root: $extractedRootNorm"

# Step 5: stop MuZap service before replacing files
$serviceWasRunning = $false
$svc = Get-Service -Name "MuZap" -ErrorAction SilentlyContinue
if ($svc -and $svc.Status -eq 'Running') {
    Write-Info "Stopping MuZap service..."
    try {
        Stop-Service -Name "MuZap" -Force -ErrorAction Stop
        $serviceWasRunning = $true
        Write-Ok "Service stopped."
    } catch {
        Write-Warn "Could not stop MuZap service: $_"
    }
}

# Kill standalone winws.exe if running
$winwsProc = Get-Process -Name "winws" -ErrorAction SilentlyContinue
if ($winwsProc) {
    Write-Info "Stopping winws.exe..."
    $winwsProc | Stop-Process -Force -ErrorAction SilentlyContinue
}

# Step 6: copy files, skipping protected paths
# MuZap.bat is copied separately to .service\MuZap.bat.pending
Write-Info "Applying update files..."

$allFiles = Get-ChildItem -Path $extractedRoot -Recurse -File

foreach ($file in $allFiles) {
    $relative = $file.FullName.Substring($extractedRootNorm.Length)

    $isProtected = $false
    foreach ($p in $protectedPaths) {
        if ($relative -ieq $p) {
            $isProtected = $true
            break
        }
    }

    if ($isProtected) {
        # Special handling: stage the new MuZap.bat as pending
        if ($relative -ieq "MuZap.bat") {
            if (-not (Test-Path -LiteralPath $serviceDir)) {
                New-Item -ItemType Directory -Path $serviceDir -Force | Out-Null
            }
            try {
                Copy-Item -LiteralPath $file.FullName -Destination $pendingBat -Force
                Write-Info "Staged for self-update: MuZap.bat -> .service\MuZap.bat.pending"
            } catch {
                Write-Warn "Could not stage MuZap.bat.pending: $_"
            }
        } else {
            Write-Info "Skipping protected: $relative"
        }
        continue
    }

    $dest    = Join-Path $RootDir $relative
    $destDir = Split-Path $dest -Parent

    if (-not (Test-Path -LiteralPath $destDir)) {
        New-Item -ItemType Directory -Path $destDir -Force | Out-Null
    }

    try {
        Copy-Item -LiteralPath $file.FullName -Destination $dest -Force
        Write-Info "Updated: $relative"
    } catch {
        Write-Warn "Could not copy ${relative}: $_"
    }
}

Write-Ok "Files applied."

# Step 7: write new version into muzap.ini via config_set.ps1
Write-Info "Updating version in muzap.ini..."
try {
    & $configSetScript -Path $configFile -Section "App" -Key "Version" -Value $newVersion
    Write-Ok "Version set to $newVersion in muzap.ini."
} catch {
    Write-Warn "Could not update version in muzap.ini: $_"
}

# Step 8: restart MuZap service if it was running before
if ($serviceWasRunning) {
    Write-Info "Restarting MuZap service..."
    try {
        Start-Service -Name "MuZap" -ErrorAction Stop
        Write-Ok "Service restarted."
    } catch {
        Write-Warn "Could not restart MuZap service: $_"
    }
}

Cleanup

Write-Ok "Update to v$newVersion complete."
exit 0