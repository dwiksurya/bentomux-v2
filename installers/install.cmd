@echo off
rem Bentomux installer for Windows, for environments where running PowerShell
rem straight from the internet is blocked:
rem
rem   curl.exe -fsSLo install.cmd https://raw.githubusercontent.com/takora-dev/bentomux-v2/master/installers/install.cmd && install.cmd && del install.cmd
rem
rem It downloads install.ps1 next to itself in %TEMP% and runs it with the
rem arguments you passed, so -DryRun still works.
setlocal

set "INSTALLER_URL=https://raw.githubusercontent.com/takora-dev/bentomux-v2/master/installers/install.ps1"
set "CURL_PROTOCOL=--proto =https --tlsv1.2"
if defined BENTOMUX_INSTALLER_URL (
  set "INSTALLER_URL=%BENTOMUX_INSTALLER_URL%"
  set "CURL_PROTOCOL=--proto =http,https,file --tlsv1.2"
)

where curl.exe >nul 2>nul
if errorlevel 1 (
  echo   x curl.exe is required ^(it ships with Windows 10 1803 and later^)
  exit /b 1
)

set "TEMP_PS1=%TEMP%\bentomux-install-%RANDOM%-%RANDOM%.ps1"
curl.exe -fsSL %CURL_PROTOCOL% -o "%TEMP_PS1%" "%INSTALLER_URL%"
if errorlevel 1 (
  echo   x could not download %INSTALLER_URL%
  if exist "%TEMP_PS1%" del /q "%TEMP_PS1%" >nul 2>nul
  exit /b 1
)

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%TEMP_PS1%" %*
set "EXIT_CODE=%ERRORLEVEL%"

del /q "%TEMP_PS1%" >nul 2>nul
exit /b %EXIT_CODE%
