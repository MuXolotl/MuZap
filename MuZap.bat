@echo off
set "CONFIG_FILE=%~dp0muzap.ini"

:: ANSI color codes (Win10/11 only)
for /f %%a in ('echo prompt $E^|cmd') do set "ESC=%%a"
set "GREEN=%ESC%[92m"
set "RED=%ESC%[91m"
set "YELLOW=%ESC%[93m"
set "RESET=%ESC%[0m"

call :config_bootstrap
call :config_load

set "LOCAL_VERSION=%CFG_Version%"

:: External commands
if "%~1"=="status_zapret" (
    call :test_service MuZap soft
    call :tcp_enable
    exit /b
)

if "%~1"=="load_game_filter" (
    call :config_load
    call :game_switch_status
    exit /b
)

if "%~1"=="load_user_lists" (
    call :load_user_lists
    exit /b
)

if "%1"=="admin" (
    call :check_command chcp
    call :check_command find
    call :check_command findstr
    call :check_command netsh

    call :load_user_lists

    echo Started with admin rights
) else (
    call :check_extracted
    call :check_command powershell

    echo Requesting admin rights...
    powershell -NoProfile -Command "Start-Process 'cmd.exe' -ArgumentList '/c \"\"%~f0\" admin\"' -Verb RunAs"
    exit
)


:: MAIN MENU ===========================
setlocal EnableDelayedExpansion
:menu
cls

call :config_load
set "LOCAL_VERSION=!CFG_Version!"

call :ipset_switch_status
call :game_switch_status
call :current_strategy_status
call :telemetry_status

set "menu_choice=null"

echo.
echo           MUZAP SERVICE MANAGER v!LOCAL_VERSION!
echo ================================================
echo.
echo   1.  Service   ^| Strategy: [!CurrentStrategy!]
echo   2.  Settings  ^| Game: [!GameFilterStatus!] / IPSet: [!IPsetStatus!]
echo   3.  Updates
echo   4.  Tools
echo ------------------------------------------------
echo   0.  Exit
echo.

set /p menu_choice=   Select (0-4):

if "%menu_choice%"=="1" goto menu_service
if "%menu_choice%"=="2" goto menu_settings
if "%menu_choice%"=="3" goto menu_updates
if "%menu_choice%"=="4" goto menu_tools
if "%menu_choice%"=="0" exit /b
goto menu


:: SUBMENU: SERVICE ====================
:menu_service
cls

call :current_strategy_status

set "menu_choice=null"

echo.
echo                SERVICE MANAGEMENT
echo ================================================
echo.
echo   1.  Install  ^| Change Strategy  [!CurrentStrategy!]
echo   2.  Restart
echo   3.  Remove
echo   4.  Status
echo ------------------------------------------------
echo   0.  Back
echo.

set /p menu_choice=   Select (0-4):

if "%menu_choice%"=="1" goto service_install
if "%menu_choice%"=="2" goto service_restart
if "%menu_choice%"=="3" goto service_remove
if "%menu_choice%"=="4" goto service_status
if "%menu_choice%"=="0" goto menu
goto menu_service


:: SUBMENU: SETTINGS ===================
:menu_settings
cls

call :ipset_switch_status
call :game_switch_status
call :telemetry_status

set "menu_choice=null"

echo.
echo                     SETTINGS
echo ================================================
echo.
echo   1.  Game Filter     [!GameFilterStatus!]
echo   2.  IPSet Filter    [!IPsetStatus!]
echo   3.  Telemetry       [!TelemetryStatus!]
echo ------------------------------------------------
echo   0.  Back
echo.

set /p menu_choice=   Select (0-3):

if "%menu_choice%"=="1" goto game_switch
if "%menu_choice%"=="2" goto ipset_switch
if "%menu_choice%"=="3" goto telemetry_switch
if "%menu_choice%"=="0" goto menu
goto menu_settings


:: SUBMENU: UPDATES ====================
:menu_updates
cls

set "menu_choice=null"

echo.
echo                     UPDATES
echo ================================================
echo.
echo   1.  Update IPSet List
echo   2.  Update Hosts File
echo   3.  Remove Hosts Entries
echo   4.  Check for Updates
echo ------------------------------------------------
echo   0.  Back
echo.

set /p menu_choice=   Select (0-4):

if "%menu_choice%"=="1" goto ipset_update
if "%menu_choice%"=="2" goto hosts_update
if "%menu_choice%"=="3" goto hosts_remove
if "%menu_choice%"=="4" goto service_check_updates
if "%menu_choice%"=="0" goto menu
goto menu_updates


:: SUBMENU: TOOLS ======================
:menu_tools
cls

set "menu_choice=null"

echo.
echo                      TOOLS
echo ================================================
echo.
echo   1.  Run Diagnostics
echo   2.  Run Tests
echo ------------------------------------------------
echo   0.  Back
echo.

set /p menu_choice=   Select (0-2):

if "%menu_choice%"=="1" goto service_diagnostics
if "%menu_choice%"=="2" goto run_tests
if "%menu_choice%"=="0" goto menu
goto menu_tools


:: LOAD USER LISTS =====================
:load_user_lists
set "LISTS_PATH=%~dp0lists\"

if not exist "%LISTS_PATH%ipset-exclude-user.txt" (
    echo 203.0.113.113/32>"%LISTS_PATH%ipset-exclude-user.txt"
)
if not exist "%LISTS_PATH%list-general-user.txt" (
    echo domain.example.abc>"%LISTS_PATH%list-general-user.txt"
)
if not exist "%LISTS_PATH%list-exclude-user.txt" (
    echo domain.example.abc>"%LISTS_PATH%list-exclude-user.txt"
)

exit /b


:: TCP ENABLE ==========================
:tcp_enable
netsh interface tcp show global | findstr /i "timestamps" | findstr /i "enabled" > nul || netsh interface tcp set global timestamps=enabled > nul 2>&1
exit /b


:: STATUS ==============================
:service_status
cls

sc query "MuZap" >nul 2>&1
if !errorlevel!==0 (
    for /f "tokens=2*" %%A in ('reg query "HKLM\System\CurrentControlSet\Services\MuZap" /v MuZap-strategy 2^>nul') do echo Service strategy installed from "%%B"
)

call :test_service MuZap
call :test_service WinDivert

set "BIN_PATH=%~dp0bin\"
if not exist "%BIN_PATH%\*.sys" (
    call :PrintRed "WinDivert64.sys file NOT found."
)
echo:

tasklist /FI "IMAGENAME eq winws.exe" | find /I "winws.exe" > nul
if !errorlevel!==0 (
    call :PrintGreen "Bypass (winws.exe) is RUNNING."
) else (
    call :PrintRed "Bypass (winws.exe) is NOT running."
)

pause
goto menu_service

:test_service
set "ServiceName=%~1"
set "ServiceStatus="

for /f "tokens=3 delims=: " %%A in ('sc query "%ServiceName%" ^| findstr /i "STATE"') do set "ServiceStatus=%%A"
set "ServiceStatus=%ServiceStatus: =%"

if "%ServiceStatus%"=="RUNNING" (
    if "%~2"=="soft" (
        echo "%ServiceName%" is ALREADY RUNNING as service, use "service.bat" and choose "Remove Services" first if you want to run standalone.
        pause
        exit /b
    ) else (
        echo "%ServiceName%" service is RUNNING.
    )
) else if "%ServiceStatus%"=="STOP_PENDING" (
    call :PrintYellow "%ServiceName% is STOP_PENDING, that may be caused by a conflict with another bypass. Run Diagnostics to try to fix conflicts"
) else if not "%~2"=="soft" (
    echo "%ServiceName%" service is NOT running.
)

exit /b


:: REMOVE ==============================
:service_remove
cls

set SRVCNAME=MuZap
sc query "!SRVCNAME!" >nul 2>&1
if !errorlevel!==0 (
    net stop %SRVCNAME%
    sc delete %SRVCNAME%
) else (
    echo Service "%SRVCNAME%" is not installed.
)

tasklist /FI "IMAGENAME eq winws.exe" | find /I "winws.exe" > nul
if !errorlevel!==0 (
    taskkill /IM winws.exe /F > nul
)

sc query "WinDivert" >nul 2>&1
if !errorlevel!==0 (
    net stop "WinDivert"

    sc query "WinDivert" >nul 2>&1
    if !errorlevel!==0 (
        sc delete "WinDivert"
    )
)
net stop "WinDivert14" >nul 2>&1
sc delete "WinDivert14" >nul 2>&1

pause
goto menu_service


:: RESTART =============================
:service_restart
cls

sc query "MuZap" >nul 2>&1
if !errorlevel! neq 0 (
    call :PrintRed "Service MuZap is not installed. Use Install Service first."
    pause
    goto menu_service
)

echo Restarting MuZap service...
net stop MuZap >nul 2>&1
net start MuZap >nul 2>&1

if !errorlevel!==0 (
    call :PrintGreen "Service MuZap restarted successfully."
) else (
    call :PrintRed "Failed to restart MuZap service."
)

pause
goto menu_service


:: INSTALL =============================
:service_install
cls

call :config_load

:: Main
cd /d "%~dp0"
set "BIN_PATH=%~dp0bin\"
set "LISTS_PATH=%~dp0lists\"
set "INI_FILE=%~dp0strategies.ini"

if not exist "%INI_FILE%" (
    call :PrintRed "strategies.ini NOT found in root directory."
    pause
    goto menu_service
)

:: Load game filter variables first so they can be substituted
call :game_switch_status

echo Pick one of the strategies from strategies.ini:
set "count=0"
set "CUR_SEC="

:: Single pass: collect names, descriptions and params together
for /f "usebackq tokens=1,* delims==" %%A in ("%INI_FILE%") do (
    set "KEY=%%A"
    for /f "tokens=* delims= " %%a in ("!KEY!") do set "KEY=%%a"

    if "!KEY:~0,1!"=="[" (
        set /a count+=1
        set "CUR_SEC=!KEY:~1,-1!"
        set "section!count!=!CUR_SEC!"
        set "desc!count!=No description"
        set "params!count!="
    ) else if defined CUR_SEC (
        if /i "!KEY!"=="Description" (
            set "desc!count!=%%B"
        ) else if /i "!KEY!"=="Params" (
            set "params!count!=%%B"
        )
    )
)

for /l %%i in (1,1,!count!) do (
    echo %%i. [!section%%i!] - !desc%%i!
)

:: Choosing strategy
set "choice="
set /p "choice=Input strategy index (number): "
if "!choice!"=="" (
    echo The choice is empty, exiting...
    pause
    goto menu_service
)

set "selectedSection=!section%choice%!"
if not defined selectedSection (
    echo Invalid choice, exiting...
    pause
    goto menu_service
)

set "STRATEGY_PARAMS=!params%choice%!"
if not defined STRATEGY_PARAMS (
    call :PrintRed "Failed to parse Params for [!selectedSection!]"
    pause
    goto menu_service
)

:: Substitute variables in STRATEGY_PARAMS
set "ARGS=!STRATEGY_PARAMS:%%BIN%%=%BIN_PATH%!"
set "ARGS=!ARGS:%%LISTS%%=%LISTS_PATH%!"
set "ARGS=!ARGS:%%GameFilterTCP%%=%GameFilterTCP%!"
set "ARGS=!ARGS:%%GameFilterUDP%%=%GameFilterUDP%!"

:: Creating service with parsed args
call :tcp_enable

set SRVCNAME=MuZap

net stop %SRVCNAME% >nul 2>&1
sc delete %SRVCNAME% >nul 2>&1

:: 1. Create service with minimal path to avoid sc.exe length truncation
sc create %SRVCNAME% binPath= "\"%BIN_PATH%winws.exe\"" DisplayName= "MuZap" start= auto >nul 2>&1
sc description %SRVCNAME% "MuZap DPI bypass software" >nul 2>&1

:: 2. Overwrite ImagePath directly in registry to bypass sc.exe length limits and safely handle EXCL_MARK
setlocal disabledelayedexpansion
set "ESCAPED_ARGS=%ARGS:EXCL_MARK=^!%"
set "ESCAPED_ARGS=%ESCAPED_ARGS:"=\"%"

reg add "HKLM\System\CurrentControlSet\Services\MuZap" /v ImagePath /t REG_EXPAND_SZ /d "\"%BIN_PATH%winws.exe\" %ESCAPED_ARGS%" /f >nul 2>&1

:: Build debug string
set "DEBUG_ARGS=%ARGS:EXCL_MARK=!%"

echo:
echo [DEBUG] Final command line for service:
echo "%BIN_PATH%winws.exe" %DEBUG_ARGS%
echo:
echo If the service stopped immediately, copy the debug line above and run it manually in cmd to see the error.
endlocal & set "selectedSection_=%selectedSection%"

:: 3. Save strategy name
reg add "HKLM\System\CurrentControlSet\Services\MuZap" /v MuZap-strategy /t REG_SZ /d "%selectedSection_%" /f >nul 2>&1

:: Start the service
sc start %SRVCNAME%

pause
goto menu_service


:: SILENT REINSTALL ====================
:service_reinstall_silent

set "REINSTALL_STRATEGY="
for /f "tokens=2*" %%A in ('reg query "HKLM\System\CurrentControlSet\Services\MuZap" /v MuZap-strategy 2^>nul') do set "REINSTALL_STRATEGY=%%B"

if not defined REINSTALL_STRATEGY (
    call :PrintYellow "Cannot auto-apply: service not installed or strategy not saved in registry."
    call :PrintYellow "Please reinstall the strategy manually from the Service menu."
    exit /b 1
)

set "BIN_PATH=%~dp0bin\"
set "LISTS_PATH=%~dp0lists\"
set "INI_FILE=%~dp0strategies.ini"

if not exist "%INI_FILE%" (
    call :PrintRed "strategies.ini not found. Cannot auto-reinstall."
    exit /b 1
)

:: Single pass: find params for saved strategy
set "SR_PARAMS="
set "SR_READING=0"
for /f "usebackq tokens=1,* delims==" %%A in ("%INI_FILE%") do (
    set "KEY=%%A"
    for /f "tokens=* delims= " %%a in ("!KEY!") do set "KEY=%%a"

    if "!KEY:~0,1!"=="[" (
        set "SR_SEC=!KEY:~1,-1!"
        if /i "!SR_SEC!"=="!REINSTALL_STRATEGY!" (
            set "SR_READING=1"
        ) else if "!SR_READING!"=="1" (
            set "SR_READING=0"
        )
    ) else if "!SR_READING!"=="1" (
        if /i "!KEY!"=="Params" (
            set "SR_PARAMS=%%B"
            set "SR_READING=0"
        )
    )
)

if not defined SR_PARAMS (
    call :PrintRed "Could not find params for strategy [!REINSTALL_STRATEGY!] in strategies.ini."
    exit /b 1
)

:: Substitute
set "SR_ARGS=!SR_PARAMS:%%BIN%%=%BIN_PATH%!"
set "SR_ARGS=!SR_ARGS:%%LISTS%%=%LISTS_PATH%!"
set "SR_ARGS=!SR_ARGS:%%GameFilterTCP%%=%GameFilterTCP%!"
set "SR_ARGS=!SR_ARGS:%%GameFilterUDP%%=%GameFilterUDP%!"

call :tcp_enable

net stop MuZap >nul 2>&1
sc delete MuZap >nul 2>&1

sc create MuZap binPath= "\"%BIN_PATH%winws.exe\"" DisplayName= "MuZap" start= auto >nul 2>&1
sc description MuZap "MuZap DPI bypass software" >nul 2>&1

setlocal disabledelayedexpansion
set "SR_ESCAPED=%SR_ARGS:EXCL_MARK=^!%"
set "SR_ESCAPED=%SR_ESCAPED:"=\"%"
reg add "HKLM\System\CurrentControlSet\Services\MuZap" /v ImagePath /t REG_EXPAND_SZ /d "\"%BIN_PATH%winws.exe\" %SR_ESCAPED%" /f >nul 2>&1
endlocal & set "REINSTALL_STRATEGY_=%REINSTALL_STRATEGY%"

reg add "HKLM\System\CurrentControlSet\Services\MuZap" /v MuZap-strategy /t REG_SZ /d "%REINSTALL_STRATEGY_%" /f >nul 2>&1

sc start MuZap >nul 2>&1
if !errorlevel!==0 (
    call :PrintGreen "Service MuZap reinstalled and started with new settings [!REINSTALL_STRATEGY_!]."
) else (
    call :PrintRed "Service reinstalled but failed to start. Check the strategy manually."
)

exit /b 0


:: CHECK UPDATES =======================
:service_check_updates
cls

call :config_load
set "LOCAL_VERSION=!CFG_Version!"

set "GITHUB_API_URL=https://api.github.com/repos/MuXolotl/MuZap/releases/latest"
set "GITHUB_RELEASE_BASE_URL=https://github.com/MuXolotl/MuZap/releases/tag/"

for /f "delims=" %%A in ('powershell -NoProfile -Command ^
    "try { $r = Invoke-RestMethod -Uri '%GITHUB_API_URL%' -Headers @{'User-Agent'='MuZap'} -TimeoutSec 10; $r.tag_name -replace '^v','' } catch { '' }" 2^>nul') do set "GITHUB_VERSION=%%A"

if not defined GITHUB_VERSION (
    call :PrintRed "Warning: failed to fetch the latest version. Check your internet connection."
    pause
    goto menu_updates
)

if "!LOCAL_VERSION!"=="!GITHUB_VERSION!" (
    call :PrintGreen "Latest version is already installed: !LOCAL_VERSION!"
    pause
    goto menu_updates
)

echo New version available: !GITHUB_VERSION! (current: !LOCAL_VERSION!)
echo Release page: %GITHUB_RELEASE_BASE_URL%!GITHUB_VERSION!
echo.

set "UPDATE_CHOICE="
set /p "UPDATE_CHOICE=Do you want to update now? (Y/N, default: Y): "
if "!UPDATE_CHOICE!"=="" set "UPDATE_CHOICE=Y"
if /i "!UPDATE_CHOICE!"=="y" set "UPDATE_CHOICE=Y"

if /i "!UPDATE_CHOICE!" neq "Y" (
    echo Update cancelled.
    pause
    goto menu_updates
)

set "UPDATE_SCRIPT=%~dp0utils\update.ps1"
if not exist "%UPDATE_SCRIPT%" (
    call :PrintRed "update.ps1 not found in utils. Cannot update MuZap."
    pause
    goto menu_updates
)

echo.
echo Running MuZap updater...
set "MUZAP_ROOT=%~dp0"
if "!MUZAP_ROOT:~-1!"=="\" set "MUZAP_ROOT=!MUZAP_ROOT:~0,-1!"
powershell -NoProfile -ExecutionPolicy Bypass -File "%UPDATE_SCRIPT%" -RootDir "!MUZAP_ROOT!"

if !errorlevel!==0 (
    echo.
    call :PrintGreen "MuZap updated successfully."
    call :PrintYellow "Restarting to apply update..."
    timeout /t 1 /nobreak > nul

    set "PENDING=%~dp0.service\MuZap.bat.pending"
    set "MAINBAT=%~f0"
    set "BAT_HELPER=%TEMP%\muzap_apply.bat"

    (
        echo @echo off
        echo timeout /t 2 /nobreak ^> nul
        echo if exist "!PENDING!" move /y "!PENDING!" "!MAINBAT!"
        echo start "" "!MAINBAT!"
        echo del /f /q "%%~f0"
    ) > "!BAT_HELPER!"

    start "" /b cmd /c "!BAT_HELPER!"
    exit
)

pause
goto menu_updates


:: DIAGNOSTICS =========================
:service_diagnostics
cls

:: Base Filtering Engine
sc query BFE | findstr /I "RUNNING" > nul
if !errorlevel!==0 (
    call :PrintGreen "Base Filtering Engine check passed"
) else (
    call :PrintRed "[X] Base Filtering Engine is not running. This service is required for MuZap to work"
)
echo:

:: Proxy check
set "proxyEnabled=0"
set "proxyServer="

for /f "tokens=2*" %%A in ('reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings" /v ProxyEnable 2^>nul ^| findstr /i "ProxyEnable"') do (
    if "%%B"=="0x1" set "proxyEnabled=1"
)

if !proxyEnabled!==1 (
    for /f "tokens=2*" %%A in ('reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings" /v ProxyServer 2^>nul ^| findstr /i "ProxyServer"') do (
        set "proxyServer=%%B"
    )
    call :PrintYellow "[?] System proxy is enabled: !proxyServer!"
    call :PrintYellow "Make sure it's valid or disable it if you don't use a proxy"
) else (
    call :PrintGreen "Proxy check passed"
)
echo:

:: TCP timestamps check
netsh interface tcp show global | findstr /i "timestamps" | findstr /i "enabled" > nul
if !errorlevel!==0 (
    call :PrintGreen "TCP timestamps check passed"
) else (
    call :PrintYellow "[?] TCP timestamps are disabled. Enabling timestamps..."
    netsh interface tcp set global timestamps=enabled > nul 2>&1
    if !errorlevel!==0 (
        call :PrintGreen "TCP timestamps successfully enabled"
    ) else (
        call :PrintRed "[X] Failed to enable TCP timestamps"
    )
)
echo:

:: AdguardSvc.exe
tasklist /FI "IMAGENAME eq AdguardSvc.exe" | find /I "AdguardSvc.exe" > nul
if !errorlevel!==0 (
    call :PrintRed "[X] Adguard process found. Adguard may cause problems with Discord"
) else (
    call :PrintGreen "Adguard check passed"
)
echo:

:: Killer
sc query | findstr /I "Killer" > nul
if !errorlevel!==0 (
    call :PrintRed "[X] Killer services found. Killer conflicts with MuZap"
) else (
    call :PrintGreen "Killer check passed"
)
echo:

:: Intel Connectivity Network Service
sc query | findstr /I "Intel" | findstr /I "Connectivity" | findstr /I "Network" > nul
if !errorlevel!==0 (
    call :PrintRed "[X] Intel Connectivity Network Service found. It conflicts with MuZap"
) else (
    call :PrintGreen "Intel Connectivity check passed"
)
echo:

:: Check Point
set "checkpointFound=0"
sc query | findstr /I "TracSrvWrapper" > nul
if !errorlevel!==0 set "checkpointFound=1"
sc query | findstr /I "EPWD" > nul
if !errorlevel!==0 set "checkpointFound=1"

if !checkpointFound!==1 (
    call :PrintRed "[X] Check Point services found. Check Point conflicts with MuZap"
    call :PrintRed "Try to uninstall Check Point"
) else (
    call :PrintGreen "Check Point check passed"
)
echo:

:: SmartByte
sc query | findstr /I "SmartByte" > nul
if !errorlevel!==0 (
    call :PrintRed "[X] SmartByte services found. SmartByte conflicts with MuZap"
    call :PrintRed "Try to uninstall or disable SmartByte through services.msc"
) else (
    call :PrintGreen "SmartByte check passed"
)
echo:

:: WinDivert64.sys file
set "BIN_PATH=%~dp0bin\"
if not exist "%BIN_PATH%\*.sys" (
    call :PrintRed "WinDivert64.sys file NOT found."
    echo:
)

:: VPN
set "VPN_SERVICES="
sc query | findstr /I "VPN" > nul
if !errorlevel!==0 (
    for /f "tokens=2 delims=:" %%A in ('sc query ^| findstr /I "VPN"') do (
        if not defined VPN_SERVICES (
            set "VPN_SERVICES=!VPN_SERVICES!%%A"
        ) else (
            set "VPN_SERVICES=!VPN_SERVICES!,%%A"
        )
    )
    call :PrintYellow "[?] VPN services found:!VPN_SERVICES!. Some VPNs can conflict with MuZap"
    call :PrintYellow "Make sure that all VPNs are disabled"
) else (
    call :PrintGreen "VPN check passed"
)
echo:

:: DNS
set "dohfound=0"
for /f "delims=" %%a in ('powershell -NoProfile -Command "Get-ChildItem -Recurse -Path 'HKLM:System\CurrentControlSet\Services\Dnscache\InterfaceSpecificParameters\' | Get-ItemProperty | Where-Object { $_.DohFlags -gt 0 } | Measure-Object | Select-Object -ExpandProperty Count"') do (
    if %%a gtr 0 (
        set "dohfound=1"
    )
)
if !dohfound!==0 (
    call :PrintYellow "[?] Make sure you have configured secure DNS in a browser with some non-default DNS service provider,"
    call :PrintYellow "If you use Windows 11 you can configure encrypted DNS in the Settings to hide this warning"
) else (
    call :PrintGreen "Secure DNS check passed"
)
echo:

:: Hosts file check
set "hostsFile=%SystemRoot%\System32\drivers\etc\hosts"
if exist "%hostsFile%" (

    :: YouTube / Google domains
    set "yt_found=0"
    for %%D in (youtube.com youtu.be googlevideo.com ytimg.com ggpht.com googleusercontent.com) do (
        >nul 2>&1 findstr /I "%%D" "%hostsFile%" && set "yt_found=1"
    )
    if !yt_found!==1 (
        call :PrintYellow "[?] hosts file contains YouTube/Google entries. This may break YouTube or Google Video"
    ) else (
        call :PrintGreen "Hosts YouTube/Google check passed"
    )

    :: Discord domains
    set "dc_found=0"
    for %%D in (discord.com discordapp.com discord.gg discord.media gateway.discord.gg) do (
        >nul 2>&1 findstr /I "%%D" "%hostsFile%" && set "dc_found=1"
    )
    if !dc_found!==1 (
        call :PrintYellow "[?] hosts file contains Discord entries. This may break Discord connectivity"
    ) else (
        call :PrintGreen "Hosts Discord check passed"
    )

    :: Telegram domains
    set "tg_found=0"
    for %%D in (telegram.org t.me web.telegram.org api.telegram.org) do (
        >nul 2>&1 findstr /I "%%D" "%hostsFile%" && set "tg_found=1"
    )
    if !tg_found!==1 (
        powershell -NoProfile -Command ^
            "$h = Get-Content '%hostsFile%'; $inBlock = $false; $outside = $false;" ^
            "foreach ($l in $h) {" ^
            "  if ($l -match '# --- MuZap BEGIN') { $inBlock = $true; continue }" ^
            "  if ($l -match '# --- MuZap END')   { $inBlock = $false; continue }" ^
            "  if (-not $inBlock -and $l -match 'telegram') { $outside = $true }" ^
            "}" ^
            "if ($outside) { exit 1 } else { exit 0 }" >nul 2>&1
        if !errorlevel!==1 (
            call :PrintYellow "[?] hosts file contains Telegram entries outside MuZap block. This may conflict with MuZap hosts management"
        ) else (
            call :PrintGreen "Hosts Telegram check passed (entries are inside MuZap block)"
        )
    ) else (
        call :PrintGreen "Hosts Telegram check passed"
    )
)
echo:

:: WinDivert conflict handling
tasklist /FI "IMAGENAME eq winws.exe" | find /I "winws.exe" > nul
set "winws_running=!errorlevel!"

sc query "WinDivert" | findstr /I "RUNNING STOP_PENDING" > nul
set "windivert_running=!errorlevel!"

if !winws_running! neq 0 if !windivert_running!==0 (
    call :PrintYellow "[?] winws.exe is not running but WinDivert service is active. Attempting to delete WinDivert..."

    net stop "WinDivert" >nul 2>&1
    sc delete "WinDivert" >nul 2>&1

    sc query "WinDivert" >nul 2>&1
    if !errorlevel!==0 (
        call :PrintGreen "WinDivert successfully removed"
    ) else (
        call :PrintRed "[X] Failed to delete WinDivert. Checking for conflicting services..."

        set "conflicting_services=GoodbyeDPI"
        set "found_conflict=0"

        for %%s in (!conflicting_services!) do (
            sc query "%%s" >nul 2>&1
            if !errorlevel!==0 (
                call :PrintYellow "[?] Found conflicting service: %%s. Stopping and removing..."
                net stop "%%s" >nul 2>&1
                sc delete "%%s" >nul 2>&1
                if !errorlevel!==0 (
                    call :PrintRed "[X] Failed to remove service: %%s"
                ) else (
                    call :PrintGreen "Successfully removed service: %%s"
                )
                set "found_conflict=1"
            )
        )

        if !found_conflict!==0 (
            call :PrintYellow "[?] Attempting to delete WinDivert again..."

            net stop "WinDivert" >nul 2>&1
            sc delete "WinDivert" >nul 2>&1

            sc query "WinDivert" >nul 2>&1
            if !errorlevel!==0 (
                call :PrintGreen "WinDivert successfully deleted after removing conflicting services"
            ) else (
                call :PrintRed "[X] WinDivert still cannot be deleted. Check manually if any other bypass is using WinDivert."
            )
        ) else (
            call :PrintRed "[X] No known conflicting services removed. Check manually if any other bypass is using WinDivert."
        )
    )

    echo:
)

:: Conflicting bypasses
set "conflicting_services=GoodbyeDPI discordfix_zapret winws1 winws2"
set "found_any_conflict=0"
set "found_conflicts="

for %%s in (!conflicting_services!) do (
    sc query "%%s" >nul 2>&1
    if !errorlevel!==0 (
        if "!found_conflicts!"=="" (
            set "found_conflicts=%%s"
        ) else (
            set "found_conflicts=!found_conflicts! %%s"
        )
        set "found_any_conflict=1"
    )
)

if !found_any_conflict!==1 (
    call :PrintRed "[X] Conflicting bypass services found: !found_conflicts!"

    set "CHOICE="
    set /p "CHOICE=Do you want to remove these conflicting services? (Y/N) (default: N) "
    if "!CHOICE!"=="" set "CHOICE=N"
    if "!CHOICE!"=="y" set "CHOICE=Y"

    if /i "!CHOICE!"=="Y" (
        for %%s in (!found_conflicts!) do (
            call :PrintYellow "Stopping and removing service: %%s"
            net stop "%%s" >nul 2>&1
            sc delete "%%s" >nul 2>&1
            if !errorlevel!==0 (
                call :PrintGreen "Successfully removed service: %%s"
            ) else (
                call :PrintRed "[X] Failed to remove service: %%s"
            )
        )

        net stop "WinDivert" >nul 2>&1
        sc delete "WinDivert" >nul 2>&1
        net stop "WinDivert14" >nul 2>&1
        sc delete "WinDivert14" >nul 2>&1
    )

    echo:
)

:: Discord cache clearing
set "CHOICE="
set /p "CHOICE=Do you want to clear the Discord cache? (Y/N) (default: Y)  "
if "!CHOICE!"=="" set "CHOICE=Y"
if "!CHOICE!"=="y" set "CHOICE=Y"

if /i "!CHOICE!"=="Y" (
    tasklist /FI "IMAGENAME eq Discord.exe" | findstr /I "Discord.exe" > nul
    if !errorlevel!==0 (
        echo Discord is running, closing...
        taskkill /IM Discord.exe /F > nul
        if !errorlevel! == 0 (
            call :PrintGreen "Discord was successfully closed"
        ) else (
            call :PrintRed "Unable to close Discord"
        )
    )

    set "discordCacheDir=%appdata%\discord"

    for %%d in ("Cache" "Code Cache" "GPUCache") do (
        set "dirPath=!discordCacheDir!\%%~d"
        if exist "!dirPath!" (
            rd /s /q "!dirPath!"
            if !errorlevel!==0 (
                call :PrintGreen "Successfully deleted !dirPath!"
            ) else (
                call :PrintRed "Failed to delete !dirPath!"
            )
        ) else (
            call :PrintRed "!dirPath! does not exist"
        )
    )
)
echo:

pause
goto menu_tools


:: GAME SWITCH =========================
:game_switch_status

set "GameFilterStatus=off"
set "GameFilter=12"
set "GameFilterTCP=12"
set "GameFilterUDP=12"

if /i "%CFG_GameFilterMode%"=="all" (
    set "GameFilterStatus=TCP+UDP"
    set "GameFilter=1024-65535"
    set "GameFilterTCP=1024-65535"
    set "GameFilterUDP=1024-65535"
) else if /i "%CFG_GameFilterMode%"=="tcp" (
    set "GameFilterStatus=TCP"
    set "GameFilter=1024-65535"
    set "GameFilterTCP=1024-65535"
    set "GameFilterUDP=12"
) else if /i "%CFG_GameFilterMode%"=="udp" (
    set "GameFilterStatus=UDP"
    set "GameFilter=1024-65535"
    set "GameFilterTCP=12"
    set "GameFilterUDP=1024-65535"
)

exit /b


:game_switch
cls

echo Select game filter mode:
echo   0. Disable
echo   1. TCP and UDP
echo   2. TCP only
echo   3. UDP only
echo.
set "GameFilterChoice=0"
set /p "GameFilterChoice=Select option (0-3, default: 0): "
if "%GameFilterChoice%"=="" set "GameFilterChoice=0"

if "%GameFilterChoice%"=="0" (
    call :config_set Features GameFilterMode off
) else if "%GameFilterChoice%"=="1" (
    call :config_set Features GameFilterMode all
) else if "%GameFilterChoice%"=="2" (
    call :config_set Features GameFilterMode tcp
) else if "%GameFilterChoice%"=="3" (
    call :config_set Features GameFilterMode udp
) else (
    echo Invalid choice, exiting...
    pause
    goto menu_settings
)

:: Reload config and game filter vars with new value
call :config_load
call :game_switch_status

:: Check if service is installed and offer auto-reinstall
sc query "MuZap" >nul 2>&1
if !errorlevel!==0 (
    echo.
    call :PrintYellow "Game Filter changed. Service must be reinstalled to apply new port settings."
    set "APPLY_CHOICE=Y"
    set /p "APPLY_CHOICE=Reinstall service now with new Game Filter? (Y/N, default: Y): "
    if "!APPLY_CHOICE!"=="" set "APPLY_CHOICE=Y"
    if /i "!APPLY_CHOICE!"=="y" set "APPLY_CHOICE=Y"

    if /i "!APPLY_CHOICE!"=="Y" (
        echo.
        call :service_reinstall_silent
    ) else (
        call :PrintYellow "Skipped. Reinstall the strategy manually from the Service menu to apply changes."
    )
) else (
    call :PrintYellow "Game Filter changed. Install a strategy from the Service menu to apply."
)

pause
goto menu_settings


:: IPSET SWITCH ========================
:ipset_switch_status

set "listFile=%~dp0lists\ipset-all.txt"
set "lineCount=0"

if not exist "%listFile%" (
    set "IPsetStatus=none"
    exit /b
)

for /f %%i in ('type "%listFile%" 2^>nul ^| find /c /v ""') do set "lineCount=%%i"

if !lineCount! EQU 0 (
    set "IPsetStatus=any"
) else (
    findstr /R "^203\.0\.113\.113/32$" "%listFile%" >nul
    if !errorlevel! EQU 0 (
        set "IPsetStatus=none"
    ) else (
        set "IPsetStatus=loaded"
    )
)
exit /b


:ipset_switch
cls

call :ipset_switch_status

set "listFile=%~dp0lists\ipset-all.txt"
set "backupFile=%listFile%.backup"

echo Current IPSet mode: [!IPsetStatus!]
echo.
echo Select new IPSet mode:
echo   1. none   - placeholder IP only (bypass for all IPs disabled)
echo   2. any    - all IPs pass through the filter
echo   3. loaded - use ipset-all.txt list
echo ------------------------------------------------
echo   0. Back
echo.
set "IPSET_CHOICE="
set /p "IPSET_CHOICE=Select (0-3): "

if "!IPSET_CHOICE!"=="0" goto menu_settings

if "!IPSET_CHOICE!"=="1" (
    :: Switch to none
    if "!IPsetStatus!"=="none" (
        call :PrintYellow "Already in 'none' mode."
        pause
        goto menu_settings
    )
    if "!IPsetStatus!"=="loaded" (
        if not exist "%backupFile%" (
            ren "%listFile%" "ipset-all.txt.backup"
        ) else (
            del /f /q "%backupFile%"
            ren "%listFile%" "ipset-all.txt.backup"
        )
    )
    >"%listFile%" echo 203.0.113.113/32
    call :PrintGreen "IPSet mode set to: none"

) else if "!IPSET_CHOICE!"=="2" (
    :: Switch to any
    if "!IPsetStatus!"=="any" (
        call :PrintYellow "Already in 'any' mode."
        pause
        goto menu_settings
    )
    if "!IPsetStatus!"=="loaded" (
        if not exist "%backupFile%" (
            ren "%listFile%" "ipset-all.txt.backup"
        ) else (
            del /f /q "%backupFile%"
            ren "%listFile%" "ipset-all.txt.backup"
        )
    ) else if "!IPsetStatus!"=="none" (
        del /f /q "%listFile%" >nul 2>&1
    )
    type nul >"%listFile%"
    call :PrintGreen "IPSet mode set to: any"

) else if "!IPSET_CHOICE!"=="3" (
    :: Switch to loaded
    if "!IPsetStatus!"=="loaded" (
        call :PrintYellow "Already in 'loaded' mode."
        pause
        goto menu_settings
    )
    if exist "%backupFile%" (
        if exist "%listFile%" del /f /q "%listFile%"
        ren "%backupFile%" "ipset-all.txt"
        call :PrintGreen "IPSet mode set to: loaded (restored from backup)"
    ) else (
        call :PrintRed "No backup found. Update the IPSet list first from the Updates menu."
        pause
        goto menu_settings
    )

) else (
    call :PrintYellow "Invalid choice."
    pause
    goto menu_settings
)

:: Offer to restart service to apply new ipset
echo.
sc query "MuZap" >nul 2>&1
if !errorlevel!==0 (
    call :PrintYellow "IPSet changed. Service restart required to apply."
    set "RS_CHOICE=Y"
    set /p "RS_CHOICE=Restart service now? (Y/N, default: Y): "
    if "!RS_CHOICE!"=="" set "RS_CHOICE=Y"
    if /i "!RS_CHOICE!"=="y" set "RS_CHOICE=Y"

    if /i "!RS_CHOICE!"=="Y" (
        net stop MuZap >nul 2>&1
        net start MuZap >nul 2>&1
        if !errorlevel!==0 (
            call :PrintGreen "Service MuZap restarted successfully."
        ) else (
            call :PrintRed "Failed to restart MuZap service."
        )
    ) else (
        call :PrintYellow "Skipped. Restart the service manually to apply changes."
    )
) else (
    call :PrintYellow "Service not installed. Start a strategy from the Service menu to apply."
)

pause
goto menu_settings


:: TELEMETRY SWITCH ====================
:telemetry_status
if /i "%CFG_TelemetryEnabled%"=="true" (
    set "TelemetryStatus=on"
) else (
    set "TelemetryStatus=off"
)
exit /b

:telemetry_switch
cls

call :telemetry_status

echo Current Telemetry mode: [!TelemetryStatus!]
echo.
echo Telemetry sends anonymous test results to help identify
echo which strategies work best across different ISPs and regions.
echo No IP address is ever stored or transmitted.
echo.
echo   1.  Enable
echo   2.  Disable
echo ------------------------------------------------
echo   0.  Back
echo.
set "TELE_CHOICE="
set /p "TELE_CHOICE=Select (0-2): "

if "!TELE_CHOICE!"=="0" goto menu_settings

if "!TELE_CHOICE!"=="1" (
    if /i "!TelemetryStatus!"=="on" (
        call :PrintYellow "Telemetry is already enabled."
        pause
        goto menu_settings
    )
    call :config_set Features TelemetryEnabled true
    call :PrintGreen "Telemetry enabled. Results will be sent after the next standard test run."
) else if "!TELE_CHOICE!"=="2" (
    if /i "!TelemetryStatus!"=="off" (
        call :PrintYellow "Telemetry is already disabled."
        pause
        goto menu_settings
    )
    call :config_set Features TelemetryEnabled false
    call :PrintGreen "Telemetry disabled."
) else (
    call :PrintYellow "Invalid choice."
)

pause
goto menu_settings


:: IPSET UPDATE ========================
:ipset_update
cls

set "listFile=%~dp0lists\ipset-all.txt"
set "tempFile=%~dp0lists\ipset-all.tmp"
set "url=https://raw.githubusercontent.com/MuXolotl/MuZap/main/.service/ipset-service.txt"

echo Updating ipset-all...

where curl.exe >nul 2>&1
if !errorlevel!==0 (
    curl -L -f -m 15 -o "%tempFile%" "%url%"
) else (
    powershell -NoProfile -Command ^
        "$url = '%url%';" ^
        "$out = '%tempFile%';" ^
        "$dir = Split-Path -Parent $out;" ^
        "if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null };" ^
        "$res = Invoke-WebRequest -Uri $url -TimeoutSec 10 -UseBasicParsing;" ^
        "if ($res.StatusCode -eq 200) { $res.Content | Out-File -FilePath $out -Encoding UTF8 } else { exit 1 }"
)

if not exist "%tempFile%" (
    call :PrintRed "Error: file was not downloaded."
    pause
    goto menu_updates
)

findstr /R "^[0-9A-Fa-f\.:][0-9A-Fa-f\.:]*/[0-9]" "%tempFile%" >nul 2>&1
if !errorlevel! neq 0 (
    call :PrintRed "Error: downloaded file contains no CIDR entries. Server may have returned an error page."
    del /f /q "%tempFile%"
    pause
    goto menu_updates
)

move /y "%tempFile%" "%listFile%" >nul 2>&1
call :PrintGreen "IPSet list updated successfully."

pause
goto menu_updates


:: HOSTS UPDATE ========================
:hosts_update
cls

set "hostsFile=%SystemRoot%\System32\drivers\etc\hosts"
set "hostsUrl=https://raw.githubusercontent.com/MuXolotl/MuZap/main/.service/hosts"
set "tempFile=%TEMP%\muzap_hosts.txt"
set "helperPs1=%~dp0utils\hosts_manage.ps1"

if not exist "%helperPs1%" (
    call :PrintRed "hosts_manage.ps1 not found in utils. Cannot update hosts."
    pause
    goto menu_updates
)

echo Downloading hosts entries...

where curl.exe >nul 2>&1
if !errorlevel!==0 (
    curl -L -f -m 15 -o "%tempFile%" "%hostsUrl%"
) else (
    powershell -NoProfile -Command ^
        "$url = '%hostsUrl%';" ^
        "$out = '%tempFile%';" ^
        "$res = Invoke-WebRequest -Uri $url -TimeoutSec 10 -UseBasicParsing;" ^
        "if ($res.StatusCode -eq 200) { $res.Content | Out-File -FilePath $out -Encoding UTF8 } else { exit 1 }"
)

if not exist "%tempFile%" (
    call :PrintRed "Failed to download hosts file from repository."
    pause
    goto menu_updates
)

findstr /R "^[0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*" "%tempFile%" >nul 2>&1
if !errorlevel! neq 0 (
    call :PrintRed "Error: downloaded hosts content seems invalid. Server may have returned an error page."
    del /f /q "%tempFile%"
    pause
    goto menu_updates
)

echo Updating MuZap section in system hosts (BackupMode=%CFG_HostsBackupMode%)...
powershell -NoProfile -ExecutionPolicy Bypass -File "%helperPs1%" -Action Update -HostsFile "%hostsFile%" -SourceFile "%tempFile%" -MarkerName "MuZap" -BackupMode "%CFG_HostsBackupMode%"
if !errorlevel! neq 0 (
    call :PrintRed "Hosts update failed (PowerShell helper returned error)."
    if exist "%tempFile%" del /f /q "%tempFile%"
    pause
    goto menu_updates
)

if exist "%tempFile%" del /f /q "%tempFile%"

echo.
call :PrintGreen "Hosts file update complete (MuZap section)."

pause
goto menu_updates


:hosts_remove
cls

set "hostsFile=%SystemRoot%\System32\drivers\etc\hosts"
set "helperPs1=%~dp0utils\hosts_manage.ps1"

if not exist "%helperPs1%" (
    call :PrintRed "hosts_manage.ps1 not found in utils. Cannot remove hosts section."
    pause
    goto menu_updates
)

echo Removing MuZap section from system hosts (BackupMode=%CFG_HostsBackupMode%)...
powershell -NoProfile -ExecutionPolicy Bypass -File "%helperPs1%" -Action Remove -HostsFile "%hostsFile%" -MarkerName "MuZap" -BackupMode "%CFG_HostsBackupMode%"
if !errorlevel! neq 0 (
    call :PrintRed "Hosts remove failed (PowerShell helper returned error)."
    pause
    goto menu_updates
)

echo.
call :PrintGreen "MuZap hosts section removed (if it existed)."

pause
goto menu_updates


:: RUN TESTS ===========================
:run_tests
cls

powershell -NoProfile -Command "if ($PSVersionTable -and $PSVersionTable.PSVersion -and $PSVersionTable.PSVersion.Major -ge 3) { exit 0 } else { exit 1 }" >nul 2>&1
if %errorLevel% neq 0 (
    echo PowerShell 3.0 or newer is required.
    echo Please upgrade PowerShell and rerun this script.
    echo.
    pause
    goto menu_tools
)

echo Starting configuration tests in PowerShell window...
echo.
start "" powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0utils\test_muzap.ps1"
pause
goto menu_tools


:: Utility functions ===================

:PrintGreen
echo %GREEN%%~1%RESET%
exit /b

:PrintRed
echo %RED%%~1%RESET%
exit /b

:PrintYellow
echo %YELLOW%%~1%RESET%
exit /b

:current_strategy_status
set "CurrentStrategy=not installed"
sc query "MuZap" >nul 2>&1
if !errorlevel!==0 (
    for /f "tokens=2*" %%A in ('reg query "HKLM\System\CurrentControlSet\Services\MuZap" /v MuZap-strategy 2^>nul') do set "CurrentStrategy=%%B"
)
exit /b

:check_command
where %1 >nul 2>&1
if %errorLevel% neq 0 (
    echo [ERROR] %1 not found in PATH
    echo Fix your PATH variable with instructions here https://github.com/MuXolotl/MuZap/issues
    pause
    exit /b 1
)
exit /b 0

:check_extracted
set "extracted=1"
if not exist "%~dp0bin\" set "extracted=0"

if "%extracted%"=="0" (
    echo MuZap must be extracted from archive first or bin folder not found for some reason
    pause
    exit
)
exit /b 0


:: CONFIG (INI) ========================
:config_bootstrap
if exist "%CONFIG_FILE%" exit /b

set "BOOT_GameFilterMode=off"

if exist "%~dp0utils\game_filter.enabled" (
    for /f "usebackq delims=" %%A in ("%~dp0utils\game_filter.enabled") do (
        if not defined BOOT_GameFilterMode_READ (
            set "BOOT_GameFilterMode=%%A"
            set "BOOT_GameFilterMode_READ=1"
        )
    )
)

if /i "%BOOT_GameFilterMode%"=="all" (
    rem ok
) else if /i "%BOOT_GameFilterMode%"=="tcp" (
    rem ok
) else if /i "%BOOT_GameFilterMode%"=="udp" (
    rem ok
) else (
    set "BOOT_GameFilterMode=off"
)

(
    echo ; MuZap configuration file
    echo ; Values:
    echo ;   GameFilterMode: off ^| all ^| tcp ^| udp
    echo ;
    echo ; Telemetry:
    echo ;   TelemetryEnabled: true ^| false
    echo ;
    echo ; Hosts backup:
    echo ;   BackupMode: off ^| once ^| single ^| timestamp
    echo.
    echo [App]
    echo Version=unknown
    echo.
    echo [Features]
    echo GameFilterMode=%BOOT_GameFilterMode%
    echo TelemetryEnabled=false
    echo.
    echo [Hosts]
    echo BackupMode=once
    echo.
)>"%CONFIG_FILE%"

exit /b


:config_load
setlocal EnableDelayedExpansion

set "CFG_Version=unknown"
set "CFG_GameFilterMode=off"
set "CFG_HostsBackupMode=once"
set "CFG_TelemetryEnabled=false"

if not exist "%CONFIG_FILE%" (
    endlocal & (
        set "CFG_Version=unknown"
        set "CFG_GameFilterMode=off"
        set "CFG_HostsBackupMode=once"
        set "CFG_TelemetryEnabled=false"
    )
    exit /b
)

set "section="
for /f "usebackq delims=" %%L in ("%CONFIG_FILE%") do (
    set "line=%%L"
    for /f "tokens=* delims= " %%A in ("!line!") do set "line=%%A"

    if "!line!"=="" (
        rem skip empty
    ) else if "!line:~0,1!"==";" (
        rem skip comment
    ) else if "!line:~0,1!"=="#" (
        rem skip comment
    ) else if "!line:~0,1!"=="[" (
        set "tmp=!line!"
        if "!tmp:~-1!"=="]" set "section=!tmp:~1,-1!"
    ) else (
        for /f "tokens=1* delims==" %%A in ("!line!") do (
            set "k=%%A"
            set "v=%%B"
            for /f "tokens=* delims= " %%p in ("!k!") do set "k=%%p"
            for /f "tokens=* delims= " %%q in ("!v!") do set "v=%%q"

            if /i "!section!"=="App" (
                if /i "!k!"=="Version" set "CFG_Version=!v!"
            )
            if /i "!section!"=="Features" (
                if /i "!k!"=="GameFilterMode"     set "CFG_GameFilterMode=!v!"
                if /i "!k!"=="TelemetryEnabled"   set "CFG_TelemetryEnabled=!v!"
            )
            if /i "!section!"=="Hosts" (
                if /i "!k!"=="BackupMode" set "CFG_HostsBackupMode=!v!"
            )
        )
    )
)

if /i "!CFG_GameFilterMode!"     NEQ "off"   if /i "!CFG_GameFilterMode!"     NEQ "all"   if /i "!CFG_GameFilterMode!"     NEQ "tcp"   if /i "!CFG_GameFilterMode!"     NEQ "udp"   set "CFG_GameFilterMode=off"
if /i "!CFG_HostsBackupMode!"    NEQ "off"   if /i "!CFG_HostsBackupMode!"    NEQ "once"  if /i "!CFG_HostsBackupMode!"    NEQ "single" if /i "!CFG_HostsBackupMode!"   NEQ "timestamp" set "CFG_HostsBackupMode=once"
if /i "!CFG_TelemetryEnabled!"   NEQ "true"  if /i "!CFG_TelemetryEnabled!"   NEQ "false" set "CFG_TelemetryEnabled=false"

endlocal & (
    set "CFG_Version=%CFG_Version%"
    set "CFG_GameFilterMode=%CFG_GameFilterMode%"
    set "CFG_HostsBackupMode=%CFG_HostsBackupMode%"
    set "CFG_TelemetryEnabled=%CFG_TelemetryEnabled%"
)
exit /b


:config_set
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0utils\config_set.ps1" ^
    -Path "%CONFIG_FILE%" ^
    -Section "%~1" ^
    -Key "%~2" ^
    -Value "%~3" >nul 2>&1
exit /b