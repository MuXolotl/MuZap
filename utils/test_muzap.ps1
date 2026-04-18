$hasErrors = $false

$rootDir = Split-Path $PSScriptRoot
$listsDir = Join-Path $rootDir "lists"
$utilsDir = Join-Path $rootDir "utils"
$resultsDir = Join-Path $utilsDir "test results"
if (-not (Test-Path $resultsDir)) { New-Item -ItemType Directory -Path $resultsDir | Out-Null }

# Define functions early
function Get-IpsetStatus {
    $listFile = Join-Path $listsDir "ipset-all.txt"
    if (-not (Test-Path $listFile)) { return "none" }
    $lineCount = (Get-Content $listFile | Measure-Object -Line).Lines
    if ($lineCount -eq 0) { return "any" }
    $hasDummy = Get-Content $listFile | Select-String -Pattern "203\.0\.113\.113/32" -Quiet
    if ($hasDummy) { return "none" } else { return "loaded" }
}

function Set-IpsetMode {
    param([string]$mode)
    $listFile = Join-Path $listsDir "ipset-all.txt"
    $backupFile = Join-Path $listsDir "ipset-all.test-backup.txt"
    if ($mode -eq "any") {
        if (Test-Path $listFile) {
            Copy-Item $listFile $backupFile -Force
        } else {
            "" | Out-File $backupFile -Encoding UTF8
        }
        "" | Out-File $listFile -Encoding UTF8
    } elseif ($mode -eq "restore") {
        if (Test-Path $backupFile) {
            Move-Item $backupFile $listFile -Force
        }
    }
}

trap {
    Write-Host "[ERROR] Script interrupted. Restoring ipset..." -ForegroundColor Red
    if ((Get-Variable -Name 'originalIpsetStatus' -ErrorAction SilentlyContinue) -and $originalIpsetStatus -ne "any") {
        Set-IpsetMode -mode "restore"
    }
    if (Get-Variable -Name 'ipsetFlagFile' -ErrorAction SilentlyContinue) {
        Remove-Item -Path $ipsetFlagFile -ErrorAction SilentlyContinue
    }
    break
}

function New-OrderedDict { New-Object System.Collections.Specialized.OrderedDictionary }
function Add-OrSet {
    param($dict, $key, $val)
    if ($dict.Contains($key)) { $dict[$key] = $val } else { $dict.Add($key, $val) }
}

# Convert raw target value to structured target (supports PING:ip for ping-only targets)
function Convert-Target {
    param(
        [string]$Name,
        [string]$Value
    )

    if ($Value -like "PING:*") {
        $ping = $Value -replace '^PING:\s*', ''
        $url = $null
        $pingTarget = $ping
    } else {
        $url = $Value
        $pingTarget = $url -replace "^https?://", "" -replace "/.*$", ""
    }

    return (New-Object PSObject -Property @{
        Name       = $Name
        Url        = $url
        PingTarget = $pingTarget
    })
}

# DPI checker defaults
$dpiTimeoutSeconds = 5
$dpiRangeBytes = 65536
$dpiMaxParallel = 8
$dpiCustomHost = $env:MONITOR_HOST
if ($env:MONITOR_TIMEOUT) { [int]$dpiTimeoutSeconds = $env:MONITOR_TIMEOUT }
if ($env:MONITOR_RANGE) { [int]$dpiRangeBytes = $env:MONITOR_RANGE }
if ($env:MONITOR_MAX_PARALLEL) { [int]$dpiMaxParallel = $env:MONITOR_MAX_PARALLEL }

function Get-DpiSuite {
    $url = "https://hyperion-cs.github.io/dpi-checkers/ru/tcp-16-20/suite.v2.json"

    try {
        (Invoke-RestMethod -Uri $url -TimeoutSec $dpiTimeoutSeconds) |
            Select-Object `
                @{n='Id';       e={$_.id}},
                @{n='Provider'; e={$_.provider}},
                @{n='Country';  e={$_.country}},
                @{n='Host';     e={$_.host}}
    }
    catch {
        Write-Host "[WARN] Fetch dpi suite failed." -ForegroundColor Yellow
        @()
    }
}

function Build-DpiTargets {
    param(
        [string]$CustomHost
    )

    $suite = Get-DpiSuite
    $targets = @()

    if ($CustomHost) {
        $targets += @{ Id = "CUSTOM"; Provider = "Custom"; Country = "[!]"; Host = $CustomHost }
    } else {
        foreach ($entry in $suite) {
            $targets += @{ Id = $entry.Id; Country = $entry.Country; Provider = $entry.Provider; Host = $entry.Host }
        }
    }

    return $targets
}

function Invoke-DpiSuite {
    param(
        [array]$Targets,
        [int]$TimeoutSeconds,
        [int]$RangeBytes,
        [int]$MaxParallel
    )

    $tests = @(
        @{ Label = "HTTP";   Args = @("--http1.1") },
        @{ Label = "TLS1.2"; Args = @("--tlsv1.2", "--tls-max", "1.2") },
        @{ Label = "TLS1.3"; Args = @("--tlsv1.3", "--tls-max", "1.3") }
    )

    $rangeSpec = "0-$($RangeBytes - 1)"
    $warnDetected = $false

    Write-Host "[INFO] Targets: $($Targets.Count) (custom URL overrides suite). Range: $rangeSpec bytes; Timeout: $($TimeoutSeconds)s" -ForegroundColor Cyan
    Write-Host "[INFO] Starting DPI TCP 16-20 checks (parallel: $MaxParallel)..." -ForegroundColor DarkGray

    $runspacePool = [runspacefactory]::CreateRunspacePool(1, $MaxParallel)
    $runspacePool.Open()

    $payload = New-Object byte[] $RangeBytes
    [System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($payload)

    $payloadFile = New-TemporaryFile
    [IO.File]::WriteAllBytes($payloadFile, $payload)

    $scriptBlock = {
        param($payloadFile, $target, $tests, $rangeSpec, $TimeoutSeconds)

        $warned = $false
        $lines = @()

        foreach ($test in $tests) {
            $curlArgs = @(
                "--range", $rangeSpec,
                "-m", $TimeoutSeconds,
                "-w", "%{http_code} %{size_upload} %{size_download} %{time_total}",
                "-o", "NUL",
                "-X", "POST",
                "--data-binary", "@$payloadFile",
                "-s"
            ) + $test.Args + @("https://$($target.Host)")

            $output = $payload | curl.exe @curlArgs 2>&1
            $exit = $LASTEXITCODE
            $text = ($output | Out-String).Trim()

            $code = "NA"
            $upBytes = 0
            $downBytes = 0
            $time = -1

            if ($text -match '^(?<code>\d{3})\s+(?<up>\d+)\s+(?<down>\d+)\s+(?<time>[\d\.]+)$') {
                $code = $matches['code']
                $upBytes = [int64]$matches['up']
                $downBytes = [int64]$matches['down']
                $time = [double]$matches['time']
            } elseif (($exit -eq 35) -or ($text -match "not supported|does not support|protocol\s+'+.+'+\s+not\s+supported|unsupported protocol|TLS.not supported|Unrecognized option|Unknown option|unsupported option|unsupported feature|schannel|SSL")) {
                $code = "UNSUP"
            } elseif ($text) {
                $code = "ERR"
            }

            $upKB = [math]::Round($upBytes / 1024, 1)
            $downKB = [math]::Round($downBytes / 1024, 1)
            $status = "OK"
            $color = "Green"

            if ($code -eq "UNSUP") {
                $status = "UNSUPPORTED"
                $color = "Yellow"
            } elseif ($exit -ne 0 -or $code -eq "ERR" -or $code -eq "NA") {
                $status = "FAIL"
                $color = "Red"
            }

            if (($upBytes -gt 0) -and ($downBytes -eq 0) -and ($time -ge $TimeoutSeconds) -and ($exit -ne 0)) {
                $status = "LIKELY_BLOCKED"
                $color = "Yellow"
                $warned = $true
            }

            $lines += [PSCustomObject]@{
                TestLabel = $test.Label
                Code      = $code
                UpBytes   = $upBytes
                UpKB      = $upKB
                DownBytes = $downBytes
                DownKB    = $downKB
                Time      = $time
                Status    = $status
                Color     = $color
                Warned    = $warned
            }
        }

        return [PSCustomObject]@{
            TargetId = $target.Id
            Provider = $target.Provider
            Country  = $target.Country
            Lines    = $lines
            Warned   = $warned
        }
    }

    $runspaces = @()
    foreach ($target in $Targets) {
        $powershell = [powershell]::Create().AddScript($scriptBlock)
        [void]$powershell.AddArgument($payloadFile)
        [void]$powershell.AddArgument($target)
        [void]$powershell.AddArgument($tests)
        [void]$powershell.AddArgument($rangeSpec)
        [void]$powershell.AddArgument($TimeoutSeconds)
        $powershell.RunspacePool = $runspacePool

        $runspaces += [PSCustomObject]@{
            Powershell = $powershell
            Handle     = $powershell.BeginInvoke()
            Target     = $target
        }
    }

    $results = @()
    foreach ($rs in $runspaces) {
        try {
            $waitMs = ([int]$TimeoutSeconds + 5) * 1000
            $handle = $rs.Handle
            if ($handle -and $handle.AsyncWaitHandle) {
                $completed = $handle.AsyncWaitHandle.WaitOne($waitMs)
                if (-not $completed) {
                    Write-Host "[WARN] Runspace for [$($rs.Target.Id)] timed out after $waitMs ms; stopping runspace..." -ForegroundColor Yellow
                    try { $rs.Powershell.Stop() } catch {}
                }
            }
        } catch {}

        try {
            $res = $rs.Powershell.EndInvoke($rs.Handle)
            $results += $res

            Write-Host "`n=== [$($res.Country)][$($res.Provider)] $($res.TargetId) ===" -ForegroundColor DarkCyan
            foreach ($line in $res.Lines) {
                $msg = "[{0}] code={1} buf_up={2} bytes ({3} KB) buf_down={4} bytes ({5} KB) time={6}s status={7}" -f $line.TestLabel, $line.Code, $line.UpBytes, $line.UpKB, $line.DownBytes, $line.DownKB, $line.Time, $line.Status
                Write-Host $msg -ForegroundColor $line.Color
                if ($line.Status -eq "LIKELY_BLOCKED") {
                    Write-Host "  Pattern matches 16-20KB freeze; censor likely cutting this strategy." -ForegroundColor Yellow
                }
            }

            if ($res.Warned) {
                $warnDetected = $true
            } else {
                Write-Host "  No 16-20KB freeze pattern for this target." -ForegroundColor Green
            }
        } catch {
            Write-Host "[WARN] EndInvoke failed for [$($rs.Target.Id)]; treating as failure." -ForegroundColor Yellow
            $failedLines = @(
                [PSCustomObject]@{ TestLabel = 'HTTP';   Code = 'ERR'; UpBytes = 0; UpKB = 0; DownBytes = 0; DownKB = 0; Time = -1; Status = 'FAIL'; Color = 'Red'; Warned = $false },
                [PSCustomObject]@{ TestLabel = 'TLS1.2'; Code = 'ERR'; UpBytes = 0; UpKB = 0; DownBytes = 0; DownKB = 0; Time = -1; Status = 'FAIL'; Color = 'Red'; Warned = $false },
                [PSCustomObject]@{ TestLabel = 'TLS1.3'; Code = 'ERR'; UpBytes = 0; UpKB = 0; DownBytes = 0; DownKB = 0; Time = -1; Status = 'FAIL'; Color = 'Red'; Warned = $false }
            )
            $results += [PSCustomObject]@{
                TargetId = $rs.Target.Id
                Provider = $rs.Target.Provider
                Country  = $rs.Target.Country
                Lines    = $failedLines
                Warned   = $false
            }
        }
        $rs.Powershell.Dispose()
    }
    $runspacePool.Close()
    $runspacePool.Dispose()

    if ($warnDetected) {
        Write-Host ""
        Write-Host "[WARNING] Detected possible DPI TCP 16-20 blocking on one or more targets. Consider changing strategy/SNI/IP." -ForegroundColor Red
    } else {
        Write-Host ""
        Write-Host "[OK] No 16-20KB freeze pattern detected across targets." -ForegroundColor Green
    }

    return $results
}

function Test-MuZapServiceConflict {
    return [bool](Get-Service -Name "MuZap" -ErrorAction SilentlyContinue)
}

# Parse strategies.ini
function Get-StrategiesFromIni {
    param([string]$IniPath)

    $strategies = @()
    $currentSection = ""

    Get-Content $IniPath | ForEach-Object {
        $line = $_.Trim()
        if ($line -match '^\[(.+)\]$') {
            $currentSection = $Matches[1]
            $strategies += [PSCustomObject]@{ Name = $currentSection; Description = ""; Params = "" }
        } elseif ($currentSection -and $line -match '^Description\s*=\s*(.+)$') {
            $desc = $Matches[1].Trim()
            ($strategies | Where-Object { $_.Name -eq $currentSection }).Description = $desc
        } elseif ($currentSection -and $line -match '^Params\s*=\s*(.+)$') {
            $params = $Matches[1].Trim()
            ($strategies | Where-Object { $_.Name -eq $currentSection }).Params = $params
        }
    }

    return $strategies
}

# Read game filter mode from muzap.ini
function Get-GameFilterPorts {
    $iniFile = Join-Path $rootDir "muzap.ini"
    $tcp = "12"
    $udp = "12"

    if (Test-Path $iniFile) {
        $inFeatures = $false
        Get-Content $iniFile | ForEach-Object {
            $line = $_.Trim()
            if ($line -match '^\[(.+)\]$') {
                $inFeatures = ($matches[1] -ieq "Features")
            } elseif ($inFeatures -and $line -match '^GameFilterMode\s*=\s*(.+)$') {
                $mode = $matches[1].Trim().ToLower()
                if     ($mode -eq "all") { $tcp = "1024-65535"; $udp = "1024-65535" }
                elseif ($mode -eq "tcp") { $tcp = "1024-65535" }
                elseif ($mode -eq "udp") { $udp = "1024-65535" }
            }
        }
    }

    return @{ TCP = $tcp; UDP = $udp }
}

# Print a summary comparison table for all tested strategies
function Write-SummaryTable {
    param([hashtable]$Analytics)

    if ($Analytics.Count -eq 0) { return }

    $firstKey = ($Analytics.Keys | Select-Object -First 1)
    $isStandard = $Analytics[$firstKey].ContainsKey('PingOK')

    if ($isStandard) {
        $headers   = @("Strategy", "OK", "ERR", "UNSUP", "PingOK", "PingFail")
        $colWidths = @(32, 5, 5, 7, 8, 9)
    } else {
        $headers   = @("Strategy", "OK", "FAIL", "UNSUP", "BLOCKED")
        $colWidths = @(32, 5, 6, 7, 9)
    }

    $totalWidth = ($colWidths | Measure-Object -Sum).Sum
    $separator  = "-" * $totalWidth

    $headerLine = ""
    for ($i = 0; $i -lt $headers.Count; $i++) {
        $headerLine += $headers[$i].PadRight($colWidths[$i])
    }

    $maxScore = ($Analytics.Values | ForEach-Object { $_.OK } | Measure-Object -Maximum).Maximum

    Write-Host ""
    Write-Host "=== SUMMARY TABLE ===" -ForegroundColor Cyan
    Write-Host $separator -ForegroundColor DarkGray
    Write-Host $headerLine -ForegroundColor White
    Write-Host $separator -ForegroundColor DarkGray

    foreach ($config in $Analytics.Keys) {
        $a = $Analytics[$config]

        $rowColor = if ($a.OK -eq $maxScore -and $maxScore -gt 0) { "Green" } else { "Gray" }

        $name = $config
        if ($name.Length -gt ($colWidths[0] - 1)) {
            $name = $name.Substring(0, $colWidths[0] - 4) + "..."
        }

        $row = $name.PadRight($colWidths[0])

        if ($isStandard) {
            $row += $a.OK.ToString().PadRight($colWidths[1])
            $row += $a.ERROR.ToString().PadRight($colWidths[2])
            $row += $a.UNSUP.ToString().PadRight($colWidths[3])
            $row += $a.PingOK.ToString().PadRight($colWidths[4])
            $row += $a.PingFail.ToString().PadRight($colWidths[5])
        } else {
            $row += $a.OK.ToString().PadRight($colWidths[1])
            $row += $a.FAIL.ToString().PadRight($colWidths[2])
            $row += $a.UNSUPPORTED.ToString().PadRight($colWidths[3])
            $row += $a.LIKELY_BLOCKED.ToString().PadRight($colWidths[4])
        }

        Write-Host $row -ForegroundColor $rowColor
    }

    Write-Host $separator -ForegroundColor DarkGray
}

# Check Admin
$currentPrincipal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Host "[ERROR] Run as Administrator to execute tests" -ForegroundColor Red
    $hasErrors = $true
} else {
    Write-Host "[OK] Administrator rights detected" -ForegroundColor Green
}

# Check curl
if (-not (Get-Command "curl.exe" -ErrorAction SilentlyContinue)) {
    Write-Host "[ERROR] curl.exe not found" -ForegroundColor Red
    Write-Host "Install curl or add it to PATH" -ForegroundColor Yellow
    $hasErrors = $true
} else {
    Write-Host "[OK] curl.exe found" -ForegroundColor Green
}

# Check for leftover ipset flag from a previous interrupted run
$ipsetFlagFile = Join-Path $rootDir "ipset_switched.flag"
if (Test-Path $ipsetFlagFile) {
    Write-Host "[INFO] Detected leftover ipset switch flag. Restoring ipset..." -ForegroundColor Yellow
    Set-IpsetMode -mode "restore"
    Remove-Item -Path $ipsetFlagFile -ErrorAction SilentlyContinue
}

# Get original ipset status early
$originalIpsetStatus = Get-IpsetStatus

# Warn about ipset switching
if ($originalIpsetStatus -ne "any") {
    Write-Host "[INFO] Current ipset status: $originalIpsetStatus" -ForegroundColor Cyan
    Write-Host "[WARNING] Ipset will be switched to 'any' for accurate DPI tests." -ForegroundColor Yellow
    Write-Host "[WARNING] If you close the window with the X button, ipset will NOT restore immediately." -ForegroundColor Yellow
    Write-Host "[WARNING] It will be restored automatically on the next script run." -ForegroundColor Yellow
}

# Check if MuZap service is installed (conflicts with tests)
if (Test-MuZapServiceConflict) {
    Write-Host "[ERROR] Windows service 'MuZap' is installed" -ForegroundColor Red
    Write-Host "         Remove the service before running tests" -ForegroundColor Yellow
    Write-Host "         Open MuZap.bat and choose 'Remove Services'" -ForegroundColor Yellow
    $hasErrors = $true
}

if ($hasErrors) {
    Write-Host ""
    Write-Host "Fix the errors above and rerun." -ForegroundColor Yellow
    Write-Host "Press any key to exit..." -ForegroundColor Yellow
    [void][System.Console]::ReadKey($true)
    exit 1
}

$dpiTargets = Build-DpiTargets -CustomHost $dpiCustomHost

# Load INI strategies
$iniFile = Join-Path $rootDir "strategies.ini"
if (-not (Test-Path $iniFile)) {
    Write-Host "[ERROR] strategies.ini not found in root directory!" -ForegroundColor Red
    Write-Host "Press any key to exit..." -ForegroundColor Yellow
    [void][System.Console]::ReadKey($true)
    exit 1
}

$allStrategies = Get-StrategiesFromIni -IniPath $iniFile
$binPath       = Join-Path $rootDir "bin\"
$winwsExe      = Join-Path $binPath "winws.exe"
$listsPath     = Join-Path $rootDir "lists\"
$gamePorts     = Get-GameFilterPorts

$globalResults = @()

# Select top-level test type
function Read-TestType {
    while ($true) {
        Write-Host ""
        Write-Host "Select test type:" -ForegroundColor Cyan
        Write-Host "  [1] Standard tests (HTTP/ping)" -ForegroundColor Gray
        Write-Host "  [2] DPI checkers (TCP 16-20 freeze)" -ForegroundColor Gray
        $choice = Read-Host "Enter 1 or 2"
        switch ($choice) {
            '1' { return 'standard' }
            '2' { return 'dpi' }
            default { Write-Host "Incorrect input. Please try again." -ForegroundColor Yellow }
        }
    }
}

# Select test mode
function Read-ModeSelection {
    while ($true) {
        Write-Host ""
        Write-Host "Select test run mode:" -ForegroundColor Cyan
        Write-Host "  [1] All configs" -ForegroundColor Gray
        Write-Host "  [2] Selected configs" -ForegroundColor Gray
        $choice = Read-Host "Enter 1 or 2"
        switch ($choice) {
            '1' { return 'all' }
            '2' { return 'select' }
            default { Write-Host "Incorrect input. Please try again." -ForegroundColor Yellow }
        }
    }
}

function Read-ConfigSelection {
    param([array]$allConfigs)

    while ($true) {
        Write-Host ""
        Write-Host "Available configs:" -ForegroundColor Cyan
        for ($i = 0; $i -lt $allConfigs.Count; $i++) {
            $idx = $i + 1
            Write-Host "  [$idx] $($allConfigs[$i].Name) - $($allConfigs[$i].Description)" -ForegroundColor Gray
        }

        $selectionInput = Read-Host "Enter numbers (e.g. 1,3,5), ranges (e.g. 2-7), or mixed (e.g. 1,5-10,12). '0' for all"
        $trimmed = $selectionInput.Trim()

        if ($trimmed -eq '0') {
            return $allConfigs
        }

        $parts = $selectionInput -split '[,\s]+' | Where-Object { $_ -match '^\d+(-\d+)?$' }
        if ($parts.Count -eq 0) {
            Write-Host ""
            Write-Host "Invalid input format. Use numbers, ranges (1-5), or combinations (1,3-7,10). Try again." -ForegroundColor Yellow
            continue
        }

        $selectedIndices = @()
        $local:selectionHasWarnings = $false

        foreach ($part in $parts) {
            if ($part -match '^(\d+)-(\d+)$') {
                $start = [int]$matches[1]
                $end   = [int]$matches[2]

                if ($start -gt $end) {
                    Write-Host "  [WARN] Invalid range '$part' (start > end). Skipping." -ForegroundColor Yellow
                    $local:selectionHasWarnings = $true
                    continue
                }

                if ($start -lt 1 -or $end -gt $allConfigs.Count) {
                    Write-Host "  [WARN] Range '$part' out of bounds (valid: 1-$($allConfigs.Count)). Skipping invalid parts." -ForegroundColor Yellow
                    $local:selectionHasWarnings = $true
                    $start = [Math]::Max($start, 1)
                    $end   = [Math]::Min($end, $allConfigs.Count)
                }

                for ($i = $start; $i -le $end; $i++) {
                    $selectedIndices += $i
                }
            } else {
                $num = [int]$part
                if ($num -ge 1 -and $num -le $allConfigs.Count) {
                    $selectedIndices += $num
                } else {
                    Write-Host "  [WARN] Number '$num' out of bounds (valid: 1-$($allConfigs.Count)). Skipping." -ForegroundColor Yellow
                    $local:selectionHasWarnings = $true
                }
            }
        }

        $valid = $selectedIndices | Sort-Object -Unique | Where-Object { $_ -ge 1 -and $_ -le $allConfigs.Count }
        if ($valid.Count -eq 0) {
            Write-Host ""
            Write-Host "No valid configs selected. Try again." -ForegroundColor Yellow
            continue
        }

        Write-Host "Selected configs: $($valid -join ', ')" -ForegroundColor Green
        if ($local:selectionHasWarnings) {
            Write-Host "Some entries were skipped due to errors (see warnings above)." -ForegroundColor Yellow
        }

        return $valid | ForEach-Object { $allConfigs[$_ - 1] }
    }
}

while ($true) {
    $globalResults = @()
    $testType = Read-TestType
    $mode     = Read-ModeSelection

    $strategiesToTest = @($allStrategies)
    if ($mode -eq 'select') {
        $strategiesToTest = @(Read-ConfigSelection -allConfigs $allStrategies)
    }

    # Load targets once for standard mode
    $targetList = @()
    $maxNameLen = 10
    if ($testType -eq 'standard') {
        $targetsFile = Join-Path $utilsDir "targets.txt"
        $rawTargets  = New-OrderedDict
        if (Test-Path $targetsFile) {
            Get-Content $targetsFile | ForEach-Object {
                if ($_ -match '^\s*(\w+)\s*=\s*"(.+)"\s*$') {
                    Add-OrSet -dict $rawTargets -key $matches[1] -val $matches[2]
                }
            }
        }

        if ($rawTargets.Count -eq 0) {
            Write-Host "[INFO] targets.txt missing or empty. Using defaults." -ForegroundColor Gray
            Add-OrSet $rawTargets "Discord Main"           "https://discord.com"
            Add-OrSet $rawTargets "Discord Gateway"        "https://gateway.discord.gg"
            Add-OrSet $rawTargets "Discord CDN"            "https://cdn.discordapp.com"
            Add-OrSet $rawTargets "Discord Updates"        "https://updates.discord.com"
            Add-OrSet $rawTargets "YouTube Web"            "https://www.youtube.com"
            Add-OrSet $rawTargets "YouTube Short"          "https://youtu.be"
            Add-OrSet $rawTargets "YouTube Image"          "https://i.ytimg.com"
            Add-OrSet $rawTargets "YouTube Video Redirect" "https://redirector.googlevideo.com"
            Add-OrSet $rawTargets "Google Main"            "https://www.google.com"
            Add-OrSet $rawTargets "Google Gstatic"         "https://www.gstatic.com"
            Add-OrSet $rawTargets "Cloudflare Web"         "https://www.cloudflare.com"
            Add-OrSet $rawTargets "Cloudflare CDN"         "https://cdnjs.cloudflare.com"
            Add-OrSet $rawTargets "Telegram Main"          "https://telegram.org"
            Add-OrSet $rawTargets "Telegram Short"         "https://t.me"
            Add-OrSet $rawTargets "Telegram Web"           "https://web.telegram.org"
            Add-OrSet $rawTargets "Cloudflare DNS 1.1.1.1" "PING:1.1.1.1"
            Add-OrSet $rawTargets "Cloudflare DNS 1.0.0.1" "PING:1.0.0.1"
            Add-OrSet $rawTargets "Google DNS 8.8.8.8"     "PING:8.8.8.8"
            Add-OrSet $rawTargets "Google DNS 8.8.4.4"     "PING:8.8.4.4"
            Add-OrSet $rawTargets "Quad9 DNS 9.9.9.9"      "PING:9.9.9.9"
        } else {
            Write-Host ""
            Write-Host "[INFO] Loaded targets from targets.txt" -ForegroundColor Gray
            Write-Host "[INFO] Targets loaded: $($rawTargets.Count)" -ForegroundColor Gray
        }

        foreach ($key in $rawTargets.Keys) {
            $targetList += Convert-Target -Name $key -Value $rawTargets[$key]
        }

        $maxNameLen = ($targetList | ForEach-Object { $_.Name.Length } | Measure-Object -Maximum).Maximum
        if (-not $maxNameLen -or $maxNameLen -lt 10) { $maxNameLen = 10 }
    }

    # Ensure we have configs to run
    if (-not $strategiesToTest -or $strategiesToTest.Count -eq 0) {
        Write-Host "[ERROR] No strategies found or selected" -ForegroundColor Red
        Write-Host "Press any key to exit..." -ForegroundColor Yellow
        [void][System.Console]::ReadKey($true)
        exit 1
    }

    function Stop-Zapret {
        Get-Process -Name "winws" -ErrorAction SilentlyContinue | Stop-Process -Force
    }

    function Write-Progress-Title {
        param(
            [int]$Current,
            [int]$Total,
            [string]$ConfigName,
            [nullable[double]]$EtaSeconds
        )

        $percent = [math]::Round(($Current - 1) / [math]::Max($Total, 1) * 100)
        $filled  = [math]::Round($percent / 5)
        $empty   = 20 - $filled
        $bar     = "=" * $filled + ">" + " " * $empty

        $etaStr = ""
        if ($EtaSeconds -ne $null -and $EtaSeconds -gt 0) {
            $etaMin = [math]::Floor($EtaSeconds / 60)
            $etaSec = [math]::Round($EtaSeconds % 60)
            if ($etaMin -gt 0) {
                $etaStr = " | ETA: ${etaMin}m ${etaSec}s"
            } else {
                $etaStr = " | ETA: ${etaSec}s"
            }
        }

        $host.UI.RawUI.WindowTitle = "MuZap Tests [$bar] $Current/$Total$etaStr | $ConfigName"
    }

    function Get-WinwsSnapshot {
        try {
            return @(Get-CimInstance Win32_Process -Filter "Name='winws.exe'" |
                Select-Object ProcessId, CommandLine, ExecutablePath)
        } catch {
            return @()
        }
    }

    function Restore-WinwsSnapshot {
        param($snapshot)

        if (-not $snapshot -or $snapshot.Count -eq 0) { return }

        # Get command lines of currently running winws instances
        $currentLines = @()
        try {
            $currentLines = @(
                Get-WinwsSnapshot |
                    ForEach-Object { $_.CommandLine } |
                    Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
            )
        } catch {
            $currentLines = @()
        }

        Write-Host "[INFO] Restoring previously running winws instances..." -ForegroundColor DarkGray

        foreach ($p in $snapshot) {
            if (-not $p.ExecutablePath) { continue }

            # Skip if a process with the same command line is already running
            if ($currentLines.Count -gt 0 -and $currentLines -contains $p.CommandLine) {
                Write-Host "[INFO] Already running, skipping: $($p.CommandLine)" -ForegroundColor DarkGray
                continue
            }

            $exe = $p.ExecutablePath
            $processArgs = ""
            if ($p.CommandLine) {
                $quotedExe = '"' + $exe + '"'
                if ($p.CommandLine.StartsWith($quotedExe)) {
                    $processArgs = $p.CommandLine.Substring($quotedExe.Length).Trim()
                } elseif ($p.CommandLine.StartsWith($exe)) {
                    $processArgs = $p.CommandLine.Substring($exe.Length).Trim()
                }
            }

            try {
                Start-Process -FilePath $exe -ArgumentList $processArgs -WorkingDirectory (Split-Path $exe -Parent) -WindowStyle Minimized | Out-Null
                Write-Host "[INFO] Restored winws process." -ForegroundColor DarkGray
            } catch {
                Write-Host "[WARN] Failed to restore winws process: $_" -ForegroundColor Yellow
            }
        }
    }

    $env:NO_UPDATE_CHECK = "1"

    # Take snapshot immediately before tests start (not before menu selection)
    $originalWinws = Get-WinwsSnapshot

    Write-Host ""
    Write-Host "============================================================" -ForegroundColor Cyan
    Write-Host "                 MUZAP CONFIG TESTS" -ForegroundColor Cyan
    Write-Host "                 Mode: $($testType.ToUpper())" -ForegroundColor Cyan
    Write-Host "                 Total configs: $($strategiesToTest.Count.ToString().PadLeft(2))" -ForegroundColor Cyan
    Write-Host "============================================================" -ForegroundColor Cyan

    try {
        if (($originalIpsetStatus -ne "any") -and ($testType -eq 'dpi')) {
            Write-Host "[WARNING] Ipset is in '$originalIpsetStatus' mode. Switching to 'any' for accurate DPI tests..." -ForegroundColor Yellow
            Set-IpsetMode -mode "any"
            "" | Out-File -FilePath $ipsetFlagFile -Encoding UTF8
        }
        Write-Host "[WARNING] Tests may take several minutes to complete. Please wait..." -ForegroundColor Yellow

        $configNum      = 0
        $completedTimes = @()
        $currentEta     = $null

        foreach ($strategy in $strategiesToTest) {
            $configNum++
            $configStartTime = Get-Date

            Write-Progress-Title -Current $configNum -Total $strategiesToTest.Count -ConfigName $strategy.Name -EtaSeconds $currentEta

            Write-Host ""
            Write-Host "------------------------------------------------------------" -ForegroundColor DarkCyan
            Write-Host "  [$configNum/$($strategiesToTest.Count)] $($strategy.Name) - $($strategy.Description)" -ForegroundColor Yellow
            Write-Host "------------------------------------------------------------" -ForegroundColor DarkCyan

            Stop-Zapret

            $rawParams   = $strategy.Params
            $finalParams = $rawParams -replace '%BIN%',          $binPath `
                                      -replace '%LISTS%',        $listsPath `
                                      -replace '%GameFilterTCP%', $gamePorts.TCP `
                                      -replace '%GameFilterUDP%', $gamePorts.UDP

            Write-Host "  > Starting config..." -ForegroundColor Cyan
            $proc = Start-Process -FilePath $winwsExe -ArgumentList $finalParams -WorkingDirectory $binPath -PassThru -WindowStyle Minimized

            Start-Sleep -Seconds 5

            if ($testType -eq 'standard') {
                $curlTimeoutSeconds = 5
                $maxParallel        = 8
                $runspacePool       = [runspacefactory]::CreateRunspacePool(1, $maxParallel)
                $runspacePool.Open()

                $scriptBlock = {
                    param($t, $curlTimeoutSeconds)

                    $httpPieces = @()

                    if ($t.Url) {
                        $tests = @(
                            @{ Label = "HTTP";   Args = @("--http1.1") },
                            @{ Label = "TLS1.2"; Args = @("--tlsv1.2", "--tls-max", "1.2") },
                            @{ Label = "TLS1.3"; Args = @("--tlsv1.3", "--tls-max", "1.3") }
                        )

                        $baseArgs = @("-I", "-s", "-m", $curlTimeoutSeconds, "-o", "NUL", "-w", "%{http_code}", "--show-error")
                        foreach ($test in $tests) {
                            try {
                                $curlArgs = $baseArgs + $test.Args
                                $stderr   = $null
                                $output   = & curl.exe @curlArgs $t.Url 2>&1 | ForEach-Object {
                                    if ($_ -is [System.Management.Automation.ErrorRecord]) {
                                        $stderr += $_.Exception.Message + " "
                                    } else {
                                        $_
                                    }
                                }
                                $httpCode = ($output | Out-String).Trim()

                                $dnsHijack = ($stderr -match "Could not resolve host|certificate|SSL certificate problem|self[- ]?signed|certificate verify failed|unable to get local issuer certificate")
                                if ($dnsHijack) {
                                    $httpPieces += "$($test.Label):SSL  "
                                    continue
                                }

                                $unsupported = (($LASTEXITCODE -eq 35) -or ($stderr -match "does not support|not supported|protocol\s+'?.+'?\s+not\s+supported|unsupported protocol|TLS.*not supported|Unrecognized option|Unknown option|unsupported option|unsupported feature|schannel"))
                                if ($unsupported) {
                                    $httpPieces += "$($test.Label):UNSUP"
                                    continue
                                }

                                if ($LASTEXITCODE -eq 0) {
                                    $httpPieces += "$($test.Label):OK   "
                                } else {
                                    $httpPieces += "$($test.Label):ERROR"
                                }
                            } catch {
                                $httpPieces += "$($test.Label):ERROR"
                            }
                        }
                    }

                    $pingResult = "n/a"
                    if ($t.PingTarget) {
                        try {
                            $pings  = Test-Connection -ComputerName $t.PingTarget -Count 3 -ErrorAction Stop
                            $avg    = ($pings | Measure-Object -Property ResponseTime -Average).Average
                            $pingResult = "{0:N0} ms" -f $avg
                        } catch {
                            $pingResult = "Timeout"
                        }
                    }

                    return (New-Object PSObject -Property @{
                        Name       = $t.Name
                        HttpTokens = $httpPieces
                        PingResult = $pingResult
                        IsUrl      = [bool]$t.Url
                    })
                }

                $runspaces = @()
                foreach ($target in $targetList) {
                    $ps = [powershell]::Create().AddScript($scriptBlock)
                    [void]$ps.AddArgument($target)
                    [void]$ps.AddArgument($curlTimeoutSeconds)
                    $ps.RunspacePool = $runspacePool

                    $runspaces += [PSCustomObject]@{
                        Powershell = $ps
                        Handle     = $ps.BeginInvoke()
                        Target     = $target
                    }
                }

                Write-Host "  > Running tests..." -ForegroundColor DarkGray

                $targetResults = @()
                foreach ($rs in $runspaces) {
                    try {
                        $waitMs = ([int]$curlTimeoutSeconds + 5) * 1000
                        $handle = $rs.Handle
                        if ($handle -and $handle.AsyncWaitHandle) {
                            $completed = $handle.AsyncWaitHandle.WaitOne($waitMs)
                            if (-not $completed) {
                                try { $rs.Powershell.Stop() } catch {}
                            }
                        }
                    } catch {}

                    try {
                        $targetResults += $rs.Powershell.EndInvoke($rs.Handle)
                    } catch {
                        Write-Host "[WARN] EndInvoke failed for '$($rs.Target.Name)'; marking as ERROR." -ForegroundColor Yellow
                        $targetResults += [PSCustomObject]@{
                            Name       = $rs.Target.Name
                            HttpTokens = @('HTTP:ERROR', 'TLS1.2:ERROR', 'TLS1.3:ERROR')
                            PingResult = 'Timeout'
                            IsUrl      = [bool]$rs.Target.Url
                        }
                    }
                    $rs.Powershell.Dispose()
                }

                $runspacePool.Close()
                $runspacePool.Dispose()

                $targetLookup = @{}
                foreach ($res in $targetResults) { $targetLookup[$res.Name] = $res }

                foreach ($target in $targetList) {
                    $res = $targetLookup[$target.Name]
                    if (-not $res) { continue }

                    Write-Host "  $($target.Name.PadRight($maxNameLen))    " -NoNewline

                    if ($res.IsUrl -and $res.HttpTokens) {
                        foreach ($tok in $res.HttpTokens) {
                            $tokColor = "Green"
                            if ($tok -match "UNSUP") { $tokColor = "Yellow" }
                            elseif ($tok -match "SSL")  { $tokColor = "Red" }
                            elseif ($tok -match "ERR")  { $tokColor = "Red" }
                            Write-Host " $tok" -NoNewline -ForegroundColor $tokColor
                        }
                        Write-Host " | Ping: " -NoNewline -ForegroundColor DarkGray
                        $pingColor = if ($res.PingResult -eq "Timeout") { "Yellow" } else { "Cyan" }
                        Write-Host "$($res.PingResult)" -NoNewline -ForegroundColor $pingColor
                        Write-Host ""
                    } else {
                        Write-Host " Ping: " -NoNewline -ForegroundColor DarkGray
                        $pingColor = if ($res.PingResult -eq "Timeout") { "Red" } else { "Cyan" }
                        Write-Host "$($res.PingResult)" -ForegroundColor $pingColor
                    }
                }

                $globalResults += @{ Config = $strategy.Name; Type = 'standard'; Results = $targetResults }

            } else {
                Write-Host "  > Running DPI checkers..." -ForegroundColor DarkGray
                $dpiResults     = Invoke-DpiSuite -Targets $dpiTargets -TimeoutSeconds $dpiTimeoutSeconds -RangeBytes $dpiRangeBytes -MaxParallel $dpiMaxParallel
                $globalResults += @{ Config = $strategy.Name; Type = 'dpi'; Results = $dpiResults }
            }

            Stop-Zapret
            if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }

            $configEndTime  = Get-Date
            $configElapsed  = ($configEndTime - $configStartTime).TotalSeconds
            $completedTimes += $configElapsed
            $avgTime        = ($completedTimes | Measure-Object -Average).Average
            $remaining      = $strategiesToTest.Count - $configNum
            $currentEta     = $avgTime * $remaining

            Write-Host ""
            Write-Host "  Config finished in $([math]::Round($configElapsed, 1))s" -ForegroundColor DarkGray
            if ($remaining -gt 0) {
                $etaMin = [math]::Floor($currentEta / 60)
                $etaSec = [math]::Round($currentEta % 60)
                if ($etaMin -gt 0) {
                    Write-Host "  ETA for remaining $remaining config(s): ${etaMin}m ${etaSec}s" -ForegroundColor DarkGray
                } else {
                    Write-Host "  ETA for remaining $remaining config(s): ${etaSec}s" -ForegroundColor DarkGray
                }
            }
        }

        Write-Host ""
        Write-Host "All tests finished." -ForegroundColor Green

        # Build analytics
        $analytics = @{}
        foreach ($res in $globalResults) {
            if ($res.Type -eq 'standard') {
                foreach ($targetRes in $res.Results) {
                    $config = $res.Config
                    if (-not $analytics.ContainsKey($config)) {
                        $analytics[$config] = @{ OK = 0; ERROR = 0; UNSUP = 0; PingOK = 0; PingFail = 0 }
                    }
                    if ($targetRes.IsUrl) {
                        foreach ($tok in $targetRes.HttpTokens) {
                            if ($tok -match "OK")        { $analytics[$config].OK++ }
                            elseif ($tok -match "SSL")   { $analytics[$config].ERROR++ }
                            elseif ($tok -match "ERROR") { $analytics[$config].ERROR++ }
                            elseif ($tok -match "UNSUP") { $analytics[$config].UNSUP++ }
                        }
                    }
                    if ($targetRes.PingResult -ne "Timeout" -and $targetRes.PingResult -ne "n/a") {
                        $analytics[$config].PingOK++
                    } else {
                        $analytics[$config].PingFail++
                    }
                }
            } elseif ($res.Type -eq 'dpi') {
                foreach ($targetRes in $res.Results) {
                    $config = $res.Config
                    if (-not $analytics.ContainsKey($config)) {
                        $analytics[$config] = @{ OK = 0; FAIL = 0; UNSUPPORTED = 0; LIKELY_BLOCKED = 0 }
                    }
                    foreach ($line in $targetRes.Lines) {
                        if ($line.Status -eq "OK")             { $analytics[$config].OK++ }
                        elseif ($line.Status -eq "FAIL")           { $analytics[$config].FAIL++ }
                        elseif ($line.Status -eq "UNSUPPORTED")    { $analytics[$config].UNSUPPORTED++ }
                        elseif ($line.Status -eq "LIKELY_BLOCKED") { $analytics[$config].LIKELY_BLOCKED++ }
                    }
                }
            }
        }

        # Per-strategy analytics lines
        Write-Host ""
        Write-Host "=== ANALYTICS ===" -ForegroundColor Cyan
        foreach ($config in $analytics.Keys) {
            $a = $analytics[$config]
            if ($a.ContainsKey('PingOK')) {
                Write-Host "$config : HTTP OK: $($a.OK), ERR: $($a.ERROR), UNSUP: $($a.UNSUP), Ping OK: $($a.PingOK), Fail: $($a.PingFail)" -ForegroundColor Yellow
            } else {
                Write-Host "$config : OK: $($a.OK), FAIL: $($a.FAIL), UNSUP: $($a.UNSUPPORTED), BLOCKED: $($a.LIKELY_BLOCKED)" -ForegroundColor Yellow
            }
        }

        # Summary comparison table
        Write-SummaryTable -Analytics $analytics

        # Determine best strategy
        $bestConfig = $null
        $maxScore   = 0
        $maxPing    = -1
        foreach ($config in $analytics.Keys) {
            $a         = $analytics[$config]
            $score     = $a.OK
            $pingScore = if ($a.ContainsKey('PingOK')) { $a.PingOK } else { 0 }
            if ($score -gt $maxScore) {
                $maxScore   = $score
                $maxPing    = $pingScore
                $bestConfig = $config
            } elseif ($score -eq $maxScore -and $pingScore -gt $maxPing) {
                $maxPing    = $pingScore
                $bestConfig = $config
            }
        }
        Write-Host ""
        Write-Host "Best config: $bestConfig" -ForegroundColor Green
        Write-Host ""

        # Save results to file
        $dateStr    = Get-Date -Format "yyyy-MM-dd_HH-mm-ss"
        $resultFile = Join-Path $resultsDir "test_results_$dateStr.txt"
        "" | Out-File $resultFile -Encoding UTF8

        foreach ($res in $globalResults) {
            $config  = $res.Config
            $type    = $res.Type
            $results = $res.Results
            Add-Content $resultFile "Config: $config (Type: $type)"
            if ($type -eq 'standard') {
                foreach ($targetRes in $results) {
                    $name = $targetRes.Name
                    $http = $targetRes.HttpTokens -join ' '
                    $ping = $targetRes.PingResult
                    Add-Content $resultFile "  $name : $http | Ping: $ping"
                }
            } elseif ($type -eq 'dpi') {
                foreach ($targetRes in $results) {
                    $id       = $targetRes.TargetId
                    $provider = $targetRes.Provider
                    Add-Content $resultFile "  Target: $id ($provider)"
                    foreach ($line in $targetRes.Lines) {
                        $test   = $line.TestLabel
                        $code   = $line.Code
                        $size   = "$($line.UpKB)up/$($line.DownKB)down"
                        $status = $line.Status
                        Add-Content $resultFile "    ${test}: code=${code} buf=${size} KB status=${status}"
                    }
                }
            }
            Add-Content $resultFile ""
        }

        # Analytics to file
        Add-Content $resultFile "=== ANALYTICS ==="
        foreach ($config in $analytics.Keys) {
            $a = $analytics[$config]
            if ($a.ContainsKey('PingOK')) {
                Add-Content $resultFile "$config : HTTP OK: $($a.OK), ERR: $($a.ERROR), UNSUP: $($a.UNSUP), Ping OK: $($a.PingOK), Fail: $($a.PingFail)"
            } else {
                Add-Content $resultFile "$config : OK: $($a.OK), FAIL: $($a.FAIL), UNSUP: $($a.UNSUPPORTED), BLOCKED: $($a.LIKELY_BLOCKED)"
            }
        }
        Add-Content $resultFile ""
        Add-Content $resultFile "=== SUMMARY TABLE ==="

        $firstKey   = ($analytics.Keys | Select-Object -First 1)
        $isStandard = $analytics[$firstKey].ContainsKey('PingOK')
        if ($isStandard) {
            Add-Content $resultFile ("Strategy".PadRight(32) + "OK".PadRight(5) + "ERR".PadRight(5) + "UNSUP".PadRight(7) + "PingOK".PadRight(8) + "PingFail")
        } else {
            Add-Content $resultFile ("Strategy".PadRight(32) + "OK".PadRight(5) + "FAIL".PadRight(6) + "UNSUP".PadRight(7) + "BLOCKED")
        }
        Add-Content $resultFile ("-" * 64)
        foreach ($config in $analytics.Keys) {
            $a    = $analytics[$config]
            $name = $config
            if ($name.Length -gt 31) { $name = $name.Substring(0, 28) + "..." }
            if ($isStandard) {
                Add-Content $resultFile ($name.PadRight(32) + $a.OK.ToString().PadRight(5) + $a.ERROR.ToString().PadRight(5) + $a.UNSUP.ToString().PadRight(7) + $a.PingOK.ToString().PadRight(8) + $a.PingFail.ToString())
            } else {
                Add-Content $resultFile ($name.PadRight(32) + $a.OK.ToString().PadRight(5) + $a.FAIL.ToString().PadRight(6) + $a.UNSUPPORTED.ToString().PadRight(7) + $a.LIKELY_BLOCKED.ToString())
            }
        }

        Add-Content $resultFile ""
        Add-Content $resultFile "Best strategy: $bestConfig"

        Write-Host "Results saved to $resultFile" -ForegroundColor Green

    } catch {
        Write-Host "[ERROR] An error occurred during tests. Restoring ipset..." -ForegroundColor Red
        if ($originalIpsetStatus -and $originalIpsetStatus -ne "any") {
            Set-IpsetMode -mode "restore"
        }
        Remove-Item -Path $ipsetFlagFile -ErrorAction SilentlyContinue
    } finally {
        Stop-Zapret
        Restore-WinwsSnapshot -snapshot $originalWinws
        if ($originalIpsetStatus -ne "any") {
            Write-Host "[INFO] Restoring original ipset mode..." -ForegroundColor DarkGray
            Set-IpsetMode -mode "restore"
        }
        Remove-Item -Path $ipsetFlagFile -ErrorAction SilentlyContinue
    }

    Write-Host "Press any key to close..." -ForegroundColor Yellow
    [void][System.Console]::ReadKey($true)
    exit
}