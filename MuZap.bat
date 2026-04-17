@echo off
set "LOCAL_VERSION=1.0.1"
if exist "%~dp0.service\version.txt" set /p LOCAL_VERSION=<"%~dp0.service\version.txt"

:: External commands
if "%~1"=="status_zapret" (
    call :test_service MuZap soft
    call :tcp_enable
    exit /b
)

if "%~1"=="check_updates" (
    if defined NO_UPDATE_CHECK exit /b

    if exist "%~dp0utils\check_updates.enabled" (
        if not "%~2"=="soft" (
            start /b MuZap check_updates soft
        ) else (
            call :service_check_updates soft
        )
    )

    exit /b
)

if "%~1"=="load_game_filter" (
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


:: MENU ================================
setlocal EnableDelayedExpansion
:menu
cls
call :ipset_switch_status
call :game_switch_status
call :check_updates_switch_status
call :current_strategy_status

set "menu_choice=null"

echo.
echo   MUZAP SERVICE MANAGER v!LOCAL_VERSION!
echo   ----------------------------------------
echo.
echo   :: SERVICE
echo      1. Install Service     [!CurrentStrategy!]
echo      2. Restart Service
echo      3. Remove Services
echo      4. Check Status
echo.
echo   :: SETTINGS
echo      5. Game Filter         [!GameFilterStatus!]
echo      6. IPSet Filter        [!IPsetStatus!]
echo      7. Auto-Update Check   [!CheckUpdatesStatus!]
echo.
echo   :: UPDATES
echo      8. Update IPSet List
echo      9. Update Hosts File
echo      10. Check for Updates
echo.
echo   :: TOOLS
echo      11. Run Diagnostics
echo      12. Run Tests
echo.
echo   ----------------------------------------
echo      0. Exit
echo.

set /p menu_choice=   Select option (0-12): 

if "%menu_choice%"=="1" goto service_install
if "%menu_choice%"=="2" goto service_restart
if "%menu_choice%"=="3" goto service_remove
if "%menu_choice%"=="4" goto service_status
if "%menu_choice%"=="5" goto game_switch
if "%menu_choice%"=="6" goto ipset_switch
if "%menu_choice%"=="7" goto check_updates_switch
if "%menu_choice%"=="8" goto ipset_update
if "%menu_choice%"=="9" goto hosts_update
if "%menu_choice%"=="10" goto service_check_updates
if "%menu_choice%"=="11" goto service_diagnostics
if "%menu_choice%"=="12" goto run_tests
if "%menu_choice%"=="0" exit /b
goto menu


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
chcp 437 > nul

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
goto menu

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
    call :PrintYellow "!ServiceName! is STOP_PENDING, that may be caused by a conflict with another bypass. Run Diagnostics to try to fix conflicts"
) else if not "%~2"=="soft" (
    echo "%ServiceName%" service is NOT running.
)

exit /b


:: REMOVE ==============================
:service_remove
cls
chcp 65001 > nul

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
goto menu


:: RESTART =============================
:service_restart
cls
chcp 437 > nul

sc query "MuZap" >nul 2>&1
if !errorlevel! neq 0 (
    call :PrintRed "Service MuZap is not installed. Use Install Service first."
    pause
    goto menu
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
goto menu


:: INSTALL =============================
:service_install
cls
chcp 437 > nul

:: Main
cd /d "%~dp0"
set "BIN_PATH=%~dp0bin\"
set "LISTS_PATH=%~dp0lists\"
set "INI_FILE=%~dp0strategies.ini"

if not exist "%INI_FILE%" (
    call :PrintRed "strategies.ini NOT found in root directory."
    pause
    goto menu
)

:: Load game filter variables first so they can be substituted
call :game_switch_status

echo Pick one of the strategies from strategies.ini:
set "count=0"
set "CUR_SEC="

for /f "usebackq tokens=1,* delims==" %%A in ("%INI_FILE%") do (
    set "KEY=%%A"
    for /f "tokens=* delims= " %%a in ("!KEY!") do set "KEY=%%a"
    
    if "!KEY:~0,1!"=="[" (
        set /a count+=1
        set "CUR_SEC=!KEY:~1,-1!"
        set "section!count!=!CUR_SEC!"
        set "desc!count!=No description"
    ) else if /i "!KEY!"=="Description" (
        if defined CUR_SEC set "desc!count!=%%B"
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
    goto menu
)

set "selectedSection=!section%choice%!"
if not defined selectedSection (
    echo Invalid choice, exiting...
    pause
    goto menu
)

:: Find and read the Params for selectedSection
set "STRATEGY_PARAMS="
set "READING=0"
for /f "usebackq tokens=1,* delims==" %%A in ("%INI_FILE%") do (
    set "KEY=%%A"
    for /f "tokens=* delims= " %%a in ("!KEY!") do set "KEY=%%a"
    
    if "!KEY:~0,1!"=="[" (
        set "CUR_READ_SEC=!KEY:~1,-1!"
        if /i "!CUR_READ_SEC!"=="!selectedSection!" (
            set "READING=1"
        ) else (
            set "READING=0"
        )
    ) else if "!READING!"=="1" (
        if /i "!KEY!"=="Params" (
            set "STRATEGY_PARAMS=%%B"
            set "READING=0"
        )
    )
)

if not defined STRATEGY_PARAMS (
    call :PrintRed "Failed to parse Params for [!selectedSection!]"
    pause
    goto menu
)

:: Substitute variables in STRATEGY_PARAMS
:: We MUST use %%VAR%% to search for the literal text %VAR% and prevent CMD from prematurely evaluating it
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
:: Replace EXCL_MARK with ^! and escape quotes for registry
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
goto menu


:: CHECK UPDATES =======================
:service_check_updates
chcp 437 > nul
cls

:: Set current version and URLs
set "GITHUB_VERSION_URL=https://raw.githubusercontent.com/MuXolotl/MuZap/main/.service/version.txt"
set "GITHUB_RELEASE_URL=https://github.com/MuXolotl/MuZap/releases/tag/"
set "GITHUB_DOWNLOAD_URL=https://github.com/MuXolotl/MuZap/releases/latest"

:: Get the latest version from GitHub
for /f "delims=" %%A in ('powershell -NoProfile -Command "(Invoke-WebRequest -Uri \"%GITHUB_VERSION_URL%\" -Headers @{\"Cache-Control\"=\"no-cache\"} -UseBasicParsing -TimeoutSec 5).Content.Trim()" 2^>nul') do set "GITHUB_VERSION=%%A"

:: Error handling
if not defined GITHUB_VERSION (
    echo Warning: failed to fetch the latest version. This warning does not affect the operation of MuZap
    timeout /T 9
    if "%1"=="soft" exit 
    goto menu
)

:: Version comparison
if "%LOCAL_VERSION%"=="%GITHUB_VERSION%" (
    echo Latest version installed: %LOCAL_VERSION%
    
    if "%1"=="soft" exit 
    pause
    goto menu
) 

echo New version available: %GITHUB_VERSION%
echo Release page: %GITHUB_RELEASE_URL%%GITHUB_VERSION%

echo Opening the download page...
start "" "%GITHUB_DOWNLOAD_URL%"


if "%1"=="soft" exit 
pause
goto menu



:: DIAGNOSTICS =========================
:service_diagnostics
chcp 437 > nul
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
if !errorlevel!==0 (
    set "checkpointFound=1"
)

sc query | findstr /I "EPWD" > nul
if !errorlevel!==0 (
    set "checkpointFound=1"
)

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
    set "yt_found=0"
    >nul 2>&1 findstr /I "youtube.com" "%hostsFile%" && set "yt_found=1"
    >nul 2>&1 findstr /I "youtu.be" "%hostsFile%" && set "yt_found=1"
    if !yt_found!==1 (
        call :PrintYellow "[?] Your hosts file contains entries for youtube.com or youtu.be. This may cause problems with YouTube access"
    )
)

:: WinDivert conflict
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
                    call :PrintGreen "Successfully removed service: %%s"
                ) else (
                    call :PrintRed "[X] Failed to remove service: %%s"
                )
                set "found_conflict=1"
            )
        )
        
        if !found_conflict!==0 (
            call :PrintRed "[X] No conflicting services found. Check manually if any other bypass is using WinDivert."
        ) else (
            call :PrintYellow "[?] Attempting to delete WinDivert again..."

            net stop "WinDivert" >nul 2>&1
            sc delete "WinDivert" >nul 2>&1
            sc query "WinDivert" >nul 2>&1
            if !errorlevel! neq 0 (
                call :PrintGreen "WinDivert successfully deleted after removing conflicting services"
            ) else (
                call :PrintRed "[X] WinDivert still cannot be deleted. Check manually if any other bypass is using WinDivert."
            )
        )
    ) else (
        call :PrintGreen "WinDivert successfully removed"
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
goto menu


:: GAME SWITCH ========================
:game_switch_status
chcp 437 > nul

set "gameFlagFile=%~dp0utils\game_filter.enabled"

if not exist "%gameFlagFile%" (
    set "GameFilterStatus=disabled"
    set "GameFilter=12"
    set "GameFilterTCP=12"
    set "GameFilterUDP=12"
    exit /b
)

set "GameFilterMode="
for /f "usebackq delims=" %%A in ("%gameFlagFile%") do (
    if not defined GameFilterMode set "GameFilterMode=%%A"
)

if /i "%GameFilterMode%"=="all" (
    set "GameFilterStatus=enabled (TCP and UDP)"
    set "GameFilter=1024-65535"
    set "GameFilterTCP=1024-65535"
    set "GameFilterUDP=1024-65535"
) else if /i "%GameFilterMode%"=="tcp" (
    set "GameFilterStatus=enabled (TCP)"
    set "GameFilter=1024-65535"
    set "GameFilterTCP=1024-65535"
    set "GameFilterUDP=12"
) else (
    set "GameFilterStatus=enabled (UDP)"
    set "GameFilter=1024-65535"
    set "GameFilterTCP=12"
    set "GameFilterUDP=1024-65535"
)
exit /b


:game_switch
chcp 437 > nul
cls

set "gameFlagFile=%~dp0utils\game_filter.enabled"

echo Select game filter mode:
echo   0. Disable
echo   1. TCP and UDP
echo   2. TCP only
echo   3. UDP only
echo.
set "GameFilterChoice=0"
set /p "GameFilterChoice=Select option (0-3, default: 0): "
if %GameFilterChoice%=="" set "GameFilterChoice=0"

if "%GameFilterChoice%"=="0" (
    if exist "%gameFlagFile%" (
        del /f /q "%gameFlagFile%"
    ) else (
        goto menu
    )
) else if "%GameFilterChoice%"=="1" (
    echo all>"%gameFlagFile%"
) else if "%GameFilterChoice%"=="2" (
    echo tcp>"%gameFlagFile%"
) else if "%GameFilterChoice%"=="3" (
    echo udp>"%gameFlagFile%"
) else (
    echo Invalid choice, exiting...
    pause
    goto menu
)

call :PrintYellow "Restart MuZap to apply the changes"
pause
goto menu


:: CHECK UPDATES SWITCH =================
:check_updates_switch_status
chcp 437 > nul

set "checkUpdatesFlag=%~dp0utils\check_updates.enabled"

if exist "%checkUpdatesFlag%" (
    set "CheckUpdatesStatus=enabled"
) else (
    set "CheckUpdatesStatus=disabled"
)
exit /b


:check_updates_switch
chcp 437 > nul
cls

if not exist "%checkUpdatesFlag%" (
    echo Enabling check updates...
    echo ENABLED > "%checkUpdatesFlag%"
) else (
    echo Disabling check updates...
    del /f /q "%checkUpdatesFlag%"
)

pause
goto menu


:: IPSET SWITCH =======================
:ipset_switch_status
chcp 437 > nul

set "listFile=%~dp0lists\ipset-all.txt"
for /f %%i in ('type "%listFile%" 2^>nul ^| find /c /v ""') do set "lineCount=%%i"

if !lineCount!==0 (
    set "IPsetStatus=any"
) else (
    findstr /R "^203\.0\.113\.113/32$" "%listFile%" >nul
    if !errorlevel!==0 (
        set "IPsetStatus=none"
    ) else (
        set "IPsetStatus=loaded"
    )
)
exit /b


:ipset_switch
chcp 437 > nul
cls

set "listFile=%~dp0lists\ipset-all.txt"
set "backupFile=%listFile%.backup"

if "%IPsetStatus%"=="loaded" (
    echo Switching to none mode...
    
    if not exist "%backupFile%" (
        ren "%listFile%" "ipset-all.txt.backup"
    ) else (
        del /f /q "%backupFile%"
        ren "%listFile%" "ipset-all.txt.backup"
    )
    
    >"%listFile%" (
        echo 203.0.113.113/32
    )
    
) else if "%IPsetStatus%"=="none" (
    echo Switching to any mode...
    
    >"%listFile%" (
        rem Creating empty file
    )
    
) else if "%IPsetStatus%"=="any" (
    echo Switching to loaded mode...
    
    if exist "%backupFile%" (
        del /f /q "%listFile%"
        ren "%backupFile%" "ipset-all.txt"
    ) else (
        echo Error: no backup to restore. Update list from service menu first
        pause
        goto menu
    )
    
)

pause
goto menu


:: IPSET UPDATE =======================
:ipset_update
chcp 437 > nul
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
    goto menu
)

:: Check that we downloaded an IP list, not an HTML error page
findstr /R "^[0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*" "%tempFile%" >nul 2>&1
if !errorlevel! neq 0 (
    call :PrintRed "Error: downloaded file contains no IP addresses. Server may have returned an error page."
    del /f /q "%tempFile%"
    pause
    goto menu
)

:: All good - replace the main file
move /y "%tempFile%" "%listFile%" >nul 2>&1
call :PrintGreen "IPSet list updated successfully."

pause
goto menu


:: HOSTS UPDATE =======================
:hosts_update
chcp 437 > nul
cls

set "hostsFile=%SystemRoot%\System32\drivers\etc\hosts"
set "hostsUrl=https://raw.githubusercontent.com/MuXolotl/MuZap/main/.service/hosts"
set "tempFile=%TEMP%\muzap_hosts.txt"

echo Checking hosts file...

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
    goto menu
)

:: Check that we downloaded a valid hosts file, not an HTML error page
findstr /R "^[0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*" "%tempFile%" >nul 2>&1
if !errorlevel! neq 0 (
    call :PrintRed "Error: downloaded file seems invalid. Server may have returned an error page."
    del /f /q "%tempFile%"
    pause
    goto menu
)

:: Merge via PowerShell
powershell -NoProfile -Command ^
    "$tempFile = '%tempFile%';" ^
    "$hostsFile = '%hostsFile%';" ^
    "$existing = [System.IO.File]::ReadAllLines($hostsFile);" ^
    "$new = [System.IO.File]::ReadAllLines($tempFile);" ^
    "$toAdd = @();" ^
    "$added = 0;" ^
    "$skipped = 0;" ^
    "foreach ($line in $new) {" ^
    "    $trimmed = $line.Trim();" ^
    "    if ($trimmed -eq '' -or $trimmed.StartsWith('#')) { continue };" ^
    "    if ($existing -contains $trimmed) { $skipped++; continue };" ^
    "    $toAdd += $trimmed;" ^
    "    $added++;" ^
    "};" ^
    "if ($toAdd.Count -gt 0) {" ^
    "    [System.IO.File]::AppendAllLines($hostsFile, [string[]]$toAdd);" ^
    "};" ^
    "Write-Host \"Added: $added lines, Skipped: $skipped lines (already present).\""

if exist "%tempFile%" del /f /q "%tempFile%"

echo.
call :PrintGreen "Hosts file update complete."

pause
goto menu


:: RUN TESTS =============================
:run_tests
chcp 437 >nul
cls

:: Require PowerShell 3.0+
powershell -NoProfile -Command "if ($PSVersionTable -and $PSVersionTable.PSVersion -and $PSVersionTable.PSVersion.Major -ge 3) { exit 0 } else { exit 1 }" >nul 2>&1
if %errorLevel% neq 0 (
    echo PowerShell 3.0 or newer is required.
    echo Please upgrade PowerShell and rerun this script.
    echo.
    pause
    goto menu
)

echo Starting configuration tests in PowerShell window...
echo.
start "" powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0utils\test_muzap.ps1"
pause
goto menu


:: Utility functions

:PrintGreen
powershell -NoProfile -Command "Write-Host \"%~1\" -ForegroundColor Green"
exit /b

:PrintRed
powershell -NoProfile -Command "Write-Host \"%~1\" -ForegroundColor Red"
exit /b

:PrintYellow
powershell -NoProfile -Command "Write-Host \"%~1\" -ForegroundColor Yellow"
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