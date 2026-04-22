# =============================================================
# MuZap Telemetry Sender
# Compatible with PowerShell 5.1+
# Dot-source this file, then call Send-MuZapTelemetry
# =============================================================

$TELEMETRY_URL   = "https://script.google.com/macros/s/AKfycbzfFdg38vAx6T3kR1_ynZ4io7NpDC2t-hXo0cVR_LCYY9jkOC9sQGw4l2XHJDHioQm0/exec"
$TELEMETRY_TOKEN = "mzTelemetry_k9x2p7"

function Send-MuZapTelemetry {
    param(
        # Analytics hashtable from test_muzap.ps1:
        # key = strategy name, value = @{ OK; ERROR; UNSUP; PingOK; PingFail }
        [Parameter(Mandatory = $true)]
        [hashtable]$Analytics,

        [Parameter(Mandatory = $true)]
        [string]$Version
    )

    Write-Host ""
    Write-Host "[Telemetry] Collecting geo data..." -ForegroundColor DarkGray

    # --- Geo lookup — no IP is stored anywhere ---
    $isp     = "Unknown"
    $region  = "Unknown"
    $country = "Unknown"

    try {
        $geo = Invoke-RestMethod `
            -Uri         "http://ip-api.com/json/?fields=status,isp,regionName,countryCode" `
            -TimeoutSec  5 `
            -ErrorAction Stop

        if ($geo -and $geo.status -eq "success") {
            if ($geo.isp)         { $isp     = $geo.isp }
            if ($geo.regionName)  { $region  = $geo.regionName }
            if ($geo.countryCode) { $country = $geo.countryCode }
        }
    } catch {
        Write-Host "[Telemetry] Geo lookup failed, sending without location." -ForegroundColor DarkGray
    }

    # --- Build results object ---
    # Only standard-mode analytics (has PingOK key) are included.
    # No hardcoded strategy list — send whatever was actually tested.
    $results = @{}

    foreach ($key in $Analytics.Keys) {
        $a = $Analytics[$key]

        # Skip DPI-mode analytics entries (they have FAIL key instead of PingOK)
        if (-not $a.ContainsKey('PingOK')) { continue }

        $okVal       = if ($a.OK       -is [int]) { $a.OK }       else { 0 }
        $errVal      = if ($a.ERROR    -is [int]) { $a.ERROR }    else { 0 }
        $unsupVal    = if ($a.UNSUP    -is [int]) { $a.UNSUP }    else { 0 }
        $pingOkVal   = if ($a.PingOK   -is [int]) { $a.PingOK }   else { 0 }
        $pingFailVal = if ($a.PingFail -is [int]) { $a.PingFail } else { 0 }

        $results[$key] = @{
            ok       = $okVal
            err      = $errVal
            unsup    = $unsupVal
            pingOk   = $pingOkVal
            pingFail = $pingFailVal
        }
    }

    if ($results.Count -eq 0) {
        Write-Host "[Telemetry] No standard-mode results to send. Skipping." -ForegroundColor DarkGray
        return
    }

    # --- Serialize ---
    $payload = $null
    try {
        $bodyObj = @{
            token   = $TELEMETRY_TOKEN
            version = $Version
            country = $country
            region  = $region
            isp     = $isp
            results = $results
        }
        $payload = $bodyObj | ConvertTo-Json -Depth 5 -Compress
    } catch {
        Write-Host "[Telemetry] Failed to serialize payload: $_" -ForegroundColor Yellow
        return
    }

    # --- Send ---
    Write-Host "[Telemetry] Sending results ($($results.Count) strategies, ISP: $isp, $region, $country)..." -ForegroundColor DarkGray

    try {
        $response = Invoke-RestMethod `
            -Uri         $TELEMETRY_URL `
            -Method      POST `
            -ContentType "application/json" `
            -Body        $payload `
            -TimeoutSec  15 `
            -ErrorAction Stop

        if ($response -and $response.status -eq 200) {
            Write-Host "[Telemetry] Sent successfully. Thank you!" -ForegroundColor Green
        } else {
            if ($response) {
                $statusVal = $response.status
                $msgVal    = $response.message
                Write-Host "[Telemetry] Server responded: $statusVal - $msgVal" -ForegroundColor Yellow
            } else {
                Write-Host "[Telemetry] Server returned empty response." -ForegroundColor Yellow
            }
        }
    } catch {
        Write-Host "[Telemetry] Failed to send: $_" -ForegroundColor Yellow
    }
}