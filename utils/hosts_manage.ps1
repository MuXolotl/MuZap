param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Update", "Remove")]
    [string]$Action,

    [Parameter(Mandatory = $true)]
    [string]$HostsFile,

    [Parameter(Mandatory = $false)]
    [string]$SourceFile,

    [Parameter(Mandatory = $false)]
    [string]$MarkerName = "MuZap",

    [Parameter(Mandatory = $false)]
    [string]$BackupMode = "once"
)

$ErrorActionPreference = "Stop"

# Normalize BackupMode: if empty or invalid value - fall back to "once"
$validModes = @("off", "once", "single", "timestamp")
if ([string]::IsNullOrWhiteSpace($BackupMode) -or ($validModes -notcontains $BackupMode.ToLower())) {
    Write-Host "[WARN] Invalid or empty BackupMode '$BackupMode', defaulting to 'once'." -ForegroundColor Yellow
    $BackupMode = "once"
}
$BackupMode = $BackupMode.ToLower()

function Get-FileEncoding {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return [System.Text.Encoding]::Default
    }

    $bytes = [System.IO.File]::ReadAllBytes($Path)

    # UTF-8 BOM
    if ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF) {
        return New-Object System.Text.UTF8Encoding($true)
    }

    # UTF-16 LE BOM
    if ($bytes.Length -ge 2 -and $bytes[0] -eq 0xFF -and $bytes[1] -eq 0xFE) {
        return [System.Text.Encoding]::Unicode
    }

    # UTF-16 BE BOM
    if ($bytes.Length -ge 2 -and $bytes[0] -eq 0xFE -and $bytes[1] -eq 0xFF) {
        return [System.Text.Encoding]::BigEndianUnicode
    }

    return [System.Text.Encoding]::Default
}

function Read-AllLinesSafe {
    param([string]$Path, [System.Text.Encoding]$Encoding)
    if (-not (Test-Path -LiteralPath $Path)) { return @() }
    return [System.IO.File]::ReadAllLines($Path, $Encoding)
}

function Write-AllLinesSafe {
    param([string]$Path, [string[]]$Lines, [System.Text.Encoding]$Encoding)
    [System.IO.File]::WriteAllLines($Path, $Lines, $Encoding)
}

function Backup-File {
    param(
        [string]$Path,
        [string]$MarkerName,
        [string]$Mode
    )

    if ($Mode -eq "off") { return $null }
    if (-not (Test-Path -LiteralPath $Path)) { return $null }

    if ($Mode -eq "once") {
        $bak = "$Path.$MarkerName.original.bak"
        if (-not (Test-Path -LiteralPath $bak)) {
            Copy-Item -LiteralPath $Path -Destination $bak -Force
            return $bak
        }
        return $bak
    }

    if ($Mode -eq "single") {
        $bak = "$Path.$MarkerName.last.bak"
        Copy-Item -LiteralPath $Path -Destination $bak -Force
        return $bak
    }

    if ($Mode -eq "timestamp") {
        $ts = Get-Date -Format "yyyy-MM-dd_HH-mm-ss"
        $bak = "$Path.$MarkerName.bak_$ts"
        Copy-Item -LiteralPath $Path -Destination $bak -Force
        return $bak
    }

    return $null
}

$begin = "# --- $MarkerName BEGIN ---"
$end   = "# --- $MarkerName END ---"

if ($Action -eq "Update" -and (-not $SourceFile)) {
    throw "SourceFile is required for Action=Update"
}

$enc = Get-FileEncoding -Path $HostsFile
$hostsLines = Read-AllLinesSafe -Path $HostsFile -Encoding $enc

# Remove existing managed block (if present)
$clean = New-Object System.Collections.Generic.List[string]
$inBlock = $false
$blockFound = $false

foreach ($line in $hostsLines) {
    $t = $line.Trim()
    if ($t -eq $begin) { $inBlock = $true; $blockFound = $true; continue }
    if ($inBlock) {
        if ($t -eq $end) { $inBlock = $false; continue }
        continue
    }
    $clean.Add($line)
}

$bak = Backup-File -Path $HostsFile -MarkerName $MarkerName -Mode $BackupMode
if ($bak) {
    Write-Host "[OK] Hosts backup: $bak (mode=$BackupMode)" -ForegroundColor DarkGray
} else {
    Write-Host "[INFO] Hosts backup skipped (mode=$BackupMode or file missing)." -ForegroundColor DarkGray
}

if ($Action -eq "Remove") {
    Write-AllLinesSafe -Path $HostsFile -Lines $clean.ToArray() -Encoding $enc
    if ($blockFound) {
        Write-Host "[OK] Removed $MarkerName section from hosts." -ForegroundColor Green
    } else {
        Write-Host "[OK] No $MarkerName section found in hosts (nothing to remove)." -ForegroundColor Green
    }
    exit 0
}

# Action = Update
$srcEnc = Get-FileEncoding -Path $SourceFile
$srcLines = Read-AllLinesSafe -Path $SourceFile -Encoding $srcEnc

# Normalize source lines: keep empty lines, skip comment-only lines, trim trailing spaces
$normalized = New-Object System.Collections.Generic.List[string]
foreach ($l in $srcLines) {
    $line = $l.TrimEnd()
    if ($line -eq "") { $normalized.Add(""); continue }
    if ($line.TrimStart().StartsWith("#")) { continue }
    $normalized.Add($line)
}

# Ensure at least one empty line before our section
if ($clean.Count -gt 0) {
    $last = $clean[$clean.Count - 1]
    if ($last -ne $null -and $last.Trim() -ne "") {
        $clean.Add("")
    }
}

$clean.Add($begin)
foreach ($l in $normalized) { $clean.Add($l) }
$clean.Add($end)

Write-AllLinesSafe -Path $HostsFile -Lines $clean.ToArray() -Encoding $enc
Write-Host "[OK] Updated $MarkerName section in hosts." -ForegroundColor Green
exit 0