# Instalador de nodo para Windows (Teco por defecto; reutilizable por Zotz).
# Idempotente: correrlo varias veces deja siempre el mismo estado.
#
#   .\deploy\install.ps1                                   # Teco en la MSI
#   .\deploy\install.ps1 -App zotz -Binary .\zotz.exe -ServeHttpsPort 8443 `
#       -ServeTarget http://127.0.0.1:8788 -ConfigureApp:$false
#
# El secreto de Tailscale (solo para una máquina que aún no está en la tailnet) se lee
# de la variable TS_OAUTH_SECRET o de %LOCALAPPDATA%\zotz-nodes\tailscale.secret.
# Nunca se imprime.
[CmdletBinding()]
param(
    [string]$App = "teco",
    [string]$Binary = ".\target\release\$App.exe",
    [string]$Arguments = "run",
    [string]$InstallDir = "$env:LOCALAPPDATA\$App",
    [string]$TaskName = $(if ($App -eq "teco") { "Teco" } else { (Get-Culture).TextInfo.ToTitleCase($App) }),
    # La MSI corre Teco y Zotz: lleva las dos etiquetas desde el primer registro.
    [string]$Tags = "tag:teco,tag:zotz",
    [string]$Hostname = "msi-zotz",
    [int]$ServeHttpsPort = 443,
    [string]$ServeTarget = "http://127.0.0.1:7778",
    [string]$SecretFile = "$env:LOCALAPPDATA\zotz-nodes\tailscale.secret",
    [bool]$ConfigureApp = $true
)

$ErrorActionPreference = "Stop"
$Tailnet = "tail59ed16.ts.net"

function Say($m)  { Write-Host "🦉 $m" -ForegroundColor Cyan }
function Ok($m)   { Write-Host "   ✓ $m" }
function Die($m)  { Write-Host "❌ $m" -ForegroundColor Red; exit 1 }

# ------------------------------------------------------------------ Tailscale
Say "Tailscale"
function Find-Tailscale {
    $c = Get-Command tailscale.exe -ErrorAction SilentlyContinue
    if ($c) { return $c.Source }
    $p = Join-Path $env:ProgramFiles "Tailscale\tailscale.exe"
    if (Test-Path $p) { return $p }
    return $null
}
$ts = Find-Tailscale
if (-not $ts) {
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        Die "Falta Tailscale y no hay winget. Descárgalo de https://tailscale.com/download/windows, instálalo y vuelve a correr esto."
    }
    winget install --id tailscale.tailscale -e --silent --accept-package-agreements --accept-source-agreements
    $ts = Find-Tailscale
    if (-not $ts) { Die "Tailscale se instaló pero no encuentro tailscale.exe. Abre una terminal nueva y vuelve a correr esto." }
} else {
    Ok "instalado ($ts)"
}

function Get-TsStatus {
    try { return (& $ts status --json 2>$null | Out-String | ConvertFrom-Json) } catch { return $null }
}
$status = Get-TsStatus
if ($status -and $status.BackendState -eq "Running") {
    # Ya hay sesión (de usuario o etiquetada): no se toca.
    Ok "ya está en la tailnet como $($status.Self.DNSName.TrimEnd('.')); no se cambia la sesión"
} else {
    $secret = $env:TS_OAUTH_SECRET
    if (-not $secret) { $secret = [Environment]::GetEnvironmentVariable("TS_OAUTH_SECRET", "User") }
    if (-not $secret -and (Test-Path $SecretFile)) { $secret = (Get-Content $SecretFile -Raw).Trim() }
    if (-not $secret) {
        Die "La máquina no está en la tailnet y no hay secreto. Define TS_OAUTH_SECRET (usuario) o guárdalo en $SecretFile."
    }
    # Un secreto de OAuth client necesita estos parámetros; una auth key normal no.
    if ($secret.StartsWith("tskey-client-")) { $secret = "${secret}?ephemeral=false&preauthorized=true" }
    & $ts up "--auth-key=$secret" "--advertise-tags=$Tags" "--hostname=$Hostname" --unattended
    $code = $LASTEXITCODE
    Remove-Variable secret
    if ($code -ne 0) { Die "tailscale up falló (código $code)." }
    Ok "unida como $Hostname.$Tailnet con $Tags (desatendida)"
}

# ------------------------------------------------------------------ tailscale serve
Say "Publicación en la tailnet (https:$ServeHttpsPort -> $ServeTarget)"
$serve = (& $ts serve status 2>$null | Out-String)
if ($serve -match [regex]::Escape($ServeTarget.Replace("http://", ""))) {
    Ok "ya publicado"
} else {
    & $ts serve --bg "--https=$ServeHttpsPort" $ServeTarget
    if ($LASTEXITCODE -ne 0) { Die "tailscale serve falló. Si dice 'access denied', corre este script una vez como administrador." }
}

# ------------------------------------------------------------------ binario
Say "Binario"
if (-not (Test-Path $Binary) -and $App -eq "teco" -and (Test-Path ".\Cargo.toml")) {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Die "No encuentro $Binary ni cargo. Instala Rust (https://rustup.rs) o pasa -Binary C:\ruta\$App.exe."
    }
    cargo build --release
    if ($LASTEXITCODE -ne 0) { Die "cargo build falló." }
}
if (-not (Test-Path $Binary)) { Die "No encuentro el binario $Binary (pásalo con -Binary)." }
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$dest = Join-Path $InstallDir "$App.exe"
$changed = $false
if ((Test-Path $dest) -and ((Get-FileHash $Binary).Hash -eq (Get-FileHash $dest).Hash)) {
    Ok "sin cambios ($dest)"
} else {
    # Si está corriendo, hay que detenerlo para poder reemplazar el .exe.
    if (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) { Stop-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue }
    Get-Process -Name $App -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $dest } | Stop-Process -Force
    Start-Sleep -Milliseconds 500
    Copy-Item $Binary $dest -Force
    Unblock-File $dest -ErrorAction SilentlyContinue
    $changed = $true
    Ok "instalado en $dest"
}

# ------------------------------------------------------------------ configuración
if ($ConfigureApp -and $App -eq "teco") {
    Say "Configuración"
    # `teco check` crea teco.toml con los valores por defecto solo si no existe.
    $p = Start-Process -FilePath $dest -ArgumentList "check" -Wait -PassThru -WindowStyle Hidden
    if ($p.ExitCode -ne 0) { Die "teco check falló: revisa %APPDATA%\teco\teco.toml." }
    Ok "$env:APPDATA\teco\teco.toml"
}

# ------------------------------------------------------------------ arranque automático
Say "Arranque automático (tarea '$TaskName')"
$existing = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
$same = $existing -and $existing.Actions.Count -eq 1 -and $existing.Actions[0].Execute -eq $dest -and $existing.Actions[0].Arguments -eq $Arguments
if (-not $same) {
    $action   = New-ScheduledTaskAction -Execute $dest -Argument $Arguments
    $trigger  = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
                -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) `
                -MultipleInstances IgnoreNew -Priority 7
    Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Settings $settings `
        -Description "$App (dev.hugorodriguez.$App)" -Force | Out-Null
    Ok "registrada"
} else {
    Ok "ya registrada"
}
$task = Get-ScheduledTask -TaskName $TaskName
if ($task.State -ne "Running" -or $changed) {
    Start-ScheduledTask -TaskName $TaskName
    Ok "iniciada"
} else {
    Ok "ya corriendo"
}

$self = (Get-TsStatus).Self.DNSName
$port = if ($ServeHttpsPort -eq 443) { "" } else { ":$ServeHttpsPort" }
Say "Listo: https://$($self.TrimEnd('.'))$port"
