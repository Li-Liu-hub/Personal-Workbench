@echo off
chcp 65001 >nul
cd /d "%~dp0"
set PYTHONPATH=%CD%\src
python -m otp_receiver --config config.yaml serve %*
if errorlevel 1 pause
