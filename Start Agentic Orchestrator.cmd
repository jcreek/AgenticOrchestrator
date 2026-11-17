@echo off
setlocal
title Agentic Orchestrator
set "ROOT=%~dp0"
set "EXE=%ROOT%target\release\agentic-orchestrator.exe"
set "STAMP=%ROOT%target\release\agentic-orchestrator.built"

set "HEAD=unknown"
where /q git
if not errorlevel 1 for /f "usebackq delims=" %%h in (`git -C "%ROOT%." rev-parse HEAD 2^>^&1`) do set "HEAD=%%h"
set "BUILT="
if exist "%STAMP%" set /p BUILT=<"%STAMP%"

if exist "%EXE%" if "%BUILT%"=="%HEAD%" goto run

echo Building Agentic Orchestrator. The first build takes about five minutes.
echo.
for %%t in (git pnpm cargo claude) do (
  where /q %%t
  if errorlevel 1 (
    echo %%t was not found on PATH. See "Requirements" in README.md.
    goto failed
  )
)

set "CI=true"
call pnpm -C "%ROOT%app" install --frozen-lockfile
if errorlevel 1 goto failed
set "CI="
call pnpm -C "%ROOT%app" tauri build --no-bundle
if errorlevel 1 goto failed
>"%STAMP%" echo %HEAD%

:run
start "" "%EXE%"
exit /b 0

:failed
echo.
echo The build did not finish. Fix the problem above and run this file again.
pause
exit /b 1
