# Registra Teco para que arranque al iniciar sesión y se reinicie si se cae.
# Uso (PowerShell, sin admin):  .\install-task.ps1 -Exe "$env:LOCALAPPDATA\teco\teco.exe"
param([string]$Exe = "$env:LOCALAPPDATA\teco\teco.exe")

if (-not (Test-Path $Exe)) { throw "No encuentro $Exe" }
$action   = New-ScheduledTaskAction -Execute $Exe -Argument "run"
$trigger  = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
            -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) `
            -Priority 7
Register-ScheduledTask -TaskName "Teco" -Action $action -Trigger $trigger -Settings $settings `
  -Description "Teco: guardián de recursos (dev.hugorodriguez.teco)" -Force | Out-Null
Start-ScheduledTask -TaskName "Teco"
Write-Host "Teco registrado. Panel: http://127.0.0.1:7777"
