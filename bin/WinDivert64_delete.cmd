@echo off
chcp 65001 >nul

if "%1%" == "del" (
	echo УДАЛЕНИЕ ДРАЙВЕРА WINDIVERT
	sc stop windivert
	sc delete windivert
	goto :end
)

sc qc windivert
if errorlevel 1 goto :end

echo.
choice /C YN /M "Хотите остановить и удалить WinDivert?"
if ERRORLEVEL 2 goto :eof

"%~dp0elevator" "%~f0" del
goto :eof

:end
pause
