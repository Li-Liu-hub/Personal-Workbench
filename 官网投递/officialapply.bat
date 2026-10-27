@echo off
chcp 65001 >nul
cd /d "%~dp0"
set PYTHONPATH=%CD%\src
python -m officialapply.cli --config config.yaml %*
if errorlevel 1 pause
