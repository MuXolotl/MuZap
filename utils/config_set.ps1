param(
    [Parameter(Mandatory = $true)]
    [string]$Path,

    [Parameter(Mandatory = $true)]
    [string]$Section,

    [Parameter(Mandatory = $true)]
    [string]$Key,

    [Parameter(Mandatory = $true)]
    [string]$Value
)

$ErrorActionPreference = "Stop"

if (-not $Section) { throw 'Section is empty' }
if (-not $Key)     { throw 'Key is empty' }

if (-not (Test-Path -LiteralPath $Path)) {
    New-Item -ItemType File -Path $Path -Force | Out-Null
}

$lines = @()
try { $lines = Get-Content -LiteralPath $Path -ErrorAction SilentlyContinue } catch { $lines = @() }

$out          = New-Object System.Collections.Generic.List[string]
$inSection    = $false
$sectionFound = $false
$keySet       = $false

foreach ($line in $lines) {
    $trim = $line.Trim()

    if ($trim -match '^\[(.+)\]$') {
        # If we were in the target section and never found the key, append it before closing
        if ($inSection -and -not $keySet) {
            $out.Add($Key + '=' + $Value)
            $keySet = $true
        }
        $cur = $matches[1]
        if ($cur -ieq $Section) { $inSection = $true; $sectionFound = $true } else { $inSection = $false }
        $out.Add($line)
        continue
    }

    if ($inSection -and ($trim -match '^(?<k>[^=]+)=(?<v>.*)$')) {
        $k = $matches['k'].Trim()
        if ($k -ieq $Key) {
            $out.Add($Key + '=' + $Value)
            $keySet = $true
            continue
        }
    }

    $out.Add($line)
}

# Key not found but section existed (section was last in file)
if ($sectionFound -and -not $keySet) {
    $out.Add($Key + '=' + $Value)
    $keySet = $true
}

# Section not found at all - append section + key at end
if (-not $sectionFound) {
    if ($out.Count -gt 0 -and $out[$out.Count - 1].Trim() -ne '') { $out.Add('') }
    $out.Add('[' + $Section + ']')
    $out.Add($Key + '=' + $Value)
}

$enc = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllLines($Path, $out.ToArray(), $enc)