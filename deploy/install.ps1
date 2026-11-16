# Instalador de nodo para Windows: deja la máquina lista con un solo comando.
# Idempotente: correrlo varias veces deja siempre el mismo estado.
#
# Sin clonar nada (PowerShell normal; Windows pedirá permiso para instalar Tailscale):
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/HugoRodriguezDEV/teco/HEAD/deploy/install.ps1)))
#
# Desde el repo:
#   powershell -NoProfile -ExecutionPolicy Bypass -File .\deploy\install.ps1
#
# Qué hace: instala Tailscale, conecta la máquina a TU cuenta de Tailscale (si no lo está,
# abre el inicio de sesión), publica Teco (y Zotz si está instalado) por HTTPS dentro de tu
# tailnet, descarga e instala Teco y lo registra para arrancar solo.
#
# Instalación desatendida en una red propia (opcional, nunca va en el repo):
#   %LOCALAPPDATA%\zotz-nodes\tailscale.secret   secreto de un OAuth client de Tailscale
#   %LOCALAPPDATA%\zotz-nodes\profile.env        TAGS=tag:teco,...  HOSTNAME=...  ALLOW_KILL=true
# Con ambos, la máquina entra sola a esa tailnet, etiquetada y sin abrir sesión.
[CmdletBinding()]
param(
    # Ruta a un teco.exe propio. Si no se da: build local del repo o, si no hay, el Release.
    [string]$Binary = "",
    [string]$ReleaseUrl = "https://github.com/HugoRodriguezDEV/teco/releases/download/teco-latest/teco-windows-x64.exe",
    [string]$InstallDir = "$env:LOCALAPPDATA\teco",
    [string]$TaskName = "Teco",
    # Solo para unión desatendida con secreto (o vienen del perfil local).
    [string]$Tags = "",
    [string]$Hostname = "",
    [int]$ServeHttpsPort = 443,
    [string]$ServeTarget = "http://127.0.0.1:7778",
    # Zotz: si está instalado, se publica también (https:8443 -> 127.0.0.1:6768).
    [int]$ZotzHttpsPort = 8443,
    [string]$ZotzTarget = "http://127.0.0.1:6768",
    # "true"/"false" fija allow_kill_from_panel en teco.toml; "" no lo toca.
    [string]$AllowKill = "",
    # Nodos extra para la vista de Dispositivos ("Nombre=https://..."); los de la tailnet
    # se descubren solos, así que normalmente no hace falta.
    [string[]]$Devices = @(),
    [string]$SecretFile = "$env:LOCALAPPDATA\zotz-nodes\tailscale.secret",
    [string]$ProfileFile = "$env:LOCALAPPDATA\zotz-nodes\profile.env",
    # Pide el secreto (en vez de abrir el inicio de sesión) si la máquina no está en la tailnet.
    [switch]$AskSecret
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"   # Invoke-WebRequest es lentísimo con la barra

function Say($m)  { Write-Host "🦉 $m" -ForegroundColor Cyan }
function Ok($m)   { Write-Host "   ✓ $m" }
function Warn($m) { Write-Host "   ⚠ $m" -ForegroundColor Yellow }
# `throw` y no `exit`: ejecutado con irm, `exit` cerraría la ventana de PowerShell.
function Die($m)  { Write-Host "❌ $m" -ForegroundColor Red; throw "Instalación detenida." }

# Perfil local de la red propia: solo datos (CLAVE=valor), nunca se ejecuta.
if (Test-Path $ProfileFile) {
    foreach ($line in Get-Content $ProfileFile) {
        if ($line -match '^\s*([A-Z_]+)\s*=\s*(.*?)\s*$') {
            switch ($Matches[1]) {
                "TAGS"       { if (-not $Tags) { $Tags = $Matches[2] } }
                "HOSTNAME"   { if (-not $Hostname) { $Hostname = $Matches[2] } }
                "ALLOW_KILL" { if (-not $AllowKill) { $AllowKill = $Matches[2] } }
            }
        }
    }
}

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
    # El ID distingue mayúsculas con --exact.
    winget install --id Tailscale.Tailscale --exact --source winget --silent --accept-package-agreements --accept-source-agreements
    if ($LASTEXITCODE -ne 0) {
        Die "winget no pudo instalar Tailscale (código $LASTEXITCODE). Instálalo a mano desde https://tailscale.com/download/windows y vuelve a correr esto."
    }
    $ts = Find-Tailscale
    if (-not $ts) { Die "Tailscale se instaló pero no encuentro tailscale.exe. Abre una terminal nueva y vuelve a correr esto." }
} else {
    Ok "instalado ($ts)"
}
# Que `tailscale` funcione en terminales nuevas.
$tsDir = Split-Path $ts
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (-not (($userPath -split ";") -contains $tsDir) -and -not (($env:Path -split ";") -contains $tsDir)) {
    [Environment]::SetEnvironmentVariable("Path", ($userPath.TrimEnd(";") + ";" + $tsDir), "User")
    Ok "agregado al PATH de tu usuario (terminales nuevas)"
}

function Get-TsStatus {
    try { return (& $ts status --json 2>$null | Out-String | ConvertFrom-Json) } catch { return $null }
}
$status = Get-TsStatus
if ($status -and $status.BackendState -eq "Running") {
    # Ya hay sesión (de usuario o etiquetada): no se toca.
    Ok "ya está en la tailnet como $($status.Self.DNSName.TrimEnd('.')); no se cambia la sesión"
    if ($Tags -and -not $status.Self.Tags) {
        Warn "Tu perfil pide etiquetas ($Tags) pero esta máquina está a nombre de una cuenta personal."
        Warn "Para dejarla como nodo etiquetado, corre 'tailscale logout' aquí mismo y vuelve a ejecutar este instalador."
    }
} else {
    $secret = $env:TS_OAUTH_SECRET
    if (-not $secret) { $secret = [Environment]::GetEnvironmentVariable("TS_OAUTH_SECRET", "User") }
    if (-not $secret -and (Test-Path $SecretFile)) { $secret = (Get-Content $SecretFile -Raw).Trim() }
    if (-not $secret -and $AskSecret) {
        Write-Host "   Pega el secreto de Tailscale (tskey-client-...). No se mostrará en pantalla:"
        $sec = Read-Host "   Secreto" -AsSecureString
        $secret = [Runtime.InteropServices.Marshal]::PtrToStringAuto([Runtime.InteropServices.Marshal]::SecureStringToBSTR($sec))
        Remove-Variable sec
        if ($secret) {
            $secret = $secret.Trim()
            New-Item -ItemType Directory -Force -Path (Split-Path $SecretFile) | Out-Null
            Set-Content -Path $SecretFile -Value $secret -NoNewline
            Ok "secreto guardado en $SecretFile (no se vuelve a pedir)"
        }
    }
    $hostArg = @()
    if ($Hostname) { $hostArg = @("--hostname=$Hostname") }
    if ($secret) {
        # Red propia: unión desatendida y etiquetada.
        if (-not $Tags) { Die "Hay secreto de Tailscale pero faltan las etiquetas: ponlas en $ProfileFile (TAGS=tag:teco) o pasa -Tags." }
        if ($secret.StartsWith("tskey-client-")) { $secret = "${secret}?ephemeral=false&preauthorized=true" }
        & $ts up "--auth-key=$secret" "--advertise-tags=$Tags" @hostArg --unattended
        $code = $LASTEXITCODE
        Remove-Variable secret
        if ($code -ne 0) { Die "tailscale up falló (código $code). Si menciona 'tags', revisa que el OAuth client tenga esas etiquetas." }
        Ok "unida a tu tailnet con $Tags (desatendida)"
    } else {
        # Cualquier otra persona: inicia sesión con SU cuenta de Tailscale.
        Write-Host "   Inicia sesión en Tailscale con tu cuenta: abre el enlace que aparece abajo (o la ventana de Tailscale)." -ForegroundColor Yellow
        & $ts up --unattended @hostArg
        if ($LASTEXITCODE -ne 0) { Die "No se completó el inicio de sesión en Tailscale. Vuelve a correr el instalador." }
        Ok "conectada a tu cuenta de Tailscale"
    }
}
$self = (Get-TsStatus).Self.DNSName.TrimEnd('.')

# ------------------------------------------------------------------ tailscale serve
function Ensure-Serve([int]$port, [string]$target, [string]$what) {
    $prefix = if ($port -eq 443) { "https://$self/" } else { "https://${self}:$port/" }
    $proxy = $null
    try {
        $j = (& $ts serve status --json 2>$null | Out-String | ConvertFrom-Json)
        $web = $j.Web."${self}:$port"
        if ($web) { $proxy = $web.Handlers."/".Proxy }
    } catch { }
    if ($proxy -eq $target) {
        Ok "$what ya publicado en $prefix"
        return
    }
    & $ts serve --bg "--https=$port" $target | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Die "tailscale serve falló para $what. Si dice que Serve no está activado, abre el enlace que muestra, actívalo y vuelve a correr esto."
    }
    Ok "$what publicado en $prefix"
}
Say "Publicación en la tailnet"
Ensure-Serve $ServeHttpsPort $ServeTarget "Teco"
$zotzExe = Join-Path $env:LOCALAPPDATA "Programs\Zotz\Zotz.exe"
if (Test-Path $zotzExe) {
    Ensure-Serve $ZotzHttpsPort $ZotzTarget "Zotz"
} else {
    Ok "Zotz no está instalado; se omite (vuelve a correr esto después de instalarlo)"
}

# ------------------------------------------------------------------ binario
Say "Binario"
$tmp = $null
if (-not $Binary) {
    $local = ".\target\release\teco.exe"
    if ((Test-Path ".\Cargo.toml") -and (Test-Path $local)) {
        $Binary = $local
        Ok "usando el build local ($local)"
    } else {
        $tmp = Join-Path $env:TEMP "teco-download.exe"
        try {
            Invoke-WebRequest $ReleaseUrl -OutFile $tmp -UseBasicParsing
            $want = (Invoke-WebRequest "$ReleaseUrl.sha256" -UseBasicParsing).Content
            if ($want -is [byte[]]) { $want = [Text.Encoding]::ASCII.GetString($want) }
            $want = $want.Trim().ToUpper()
        } catch {
            Die "No pude descargar Teco de $ReleaseUrl ($($_.Exception.Message))."
        }
        $got = (Get-FileHash $tmp -Algorithm SHA256).Hash
        if ($got -ne $want) { Remove-Item $tmp -Force; Die "La huella SHA-256 del binario descargado no coincide; no se instala." }
        $Binary = $tmp
        Ok "descargado del Release y verificado (SHA-256)"
    }
}
if (-not (Test-Path $Binary)) { Die "No encuentro el binario $Binary." }
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$dest = Join-Path $InstallDir "teco.exe"
$changed = $false
if ((Test-Path $dest) -and ((Get-FileHash $Binary).Hash -eq (Get-FileHash $dest).Hash)) {
    Ok "sin cambios ($dest)"
} else {
    # Si está corriendo, hay que detenerlo para poder reemplazar el .exe.
    if (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) { Stop-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue }
    Get-Process -Name teco -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $dest } | Stop-Process -Force
    Start-Sleep -Milliseconds 500
    Copy-Item $Binary $dest -Force
    Unblock-File $dest -ErrorAction SilentlyContinue
    $changed = $true
    Ok "instalado en $dest"
}
if ($tmp) { Remove-Item $tmp -Force -ErrorAction SilentlyContinue }

# ------------------------------------------------------------------ configuración
Say "Configuración"
# `teco check` crea teco.toml con los valores por defecto solo si no existe.
$p = Start-Process -FilePath $dest -ArgumentList "check" -Wait -PassThru -WindowStyle Hidden
if ($p.ExitCode -ne 0) { Die "teco check falló: revisa $env:APPDATA\teco\teco.toml." }
$cfg = Join-Path $env:APPDATA "teco\teco.toml"
$text = Get-Content $cfg -Raw -Encoding UTF8
$orig = $text
# 1) Terminar desde el panel: solo si se pidió (parámetro o perfil); si no, se respeta.
$kill = $AllowKill.Trim().ToLower()
if ($kill -eq "true" -or $kill -eq "false") {
    if ($text -match "(?m)^\s*allow_kill_from_panel\s*=") {
        $text = [regex]::Replace($text, "(?m)^(\s*allow_kill_from_panel\s*=\s*)(true|false)", "`${1}$kill")
    } else {
        $text = "allow_kill_from_panel = $kill`r`n" + $text
    }
}
# 2) Otras máquinas en la vista de Dispositivos (se omite esta misma y las que ya estén).
foreach ($d in $Devices) {
    $name, $url = $d -split "=", 2
    if (-not $url) { continue }
    if ($url -match [regex]::Escape($self)) { continue }
    if ($text -match [regex]::Escape($url)) { continue }
    $text = $text.TrimEnd() + "`r`n`r`n[[device]]`r`nname = `"$name`"`r`nurl = `"$url`"`r`n"
}
if ($text -ne $orig) {
    [IO.File]::WriteAllText($cfg, $text, (New-Object Text.UTF8Encoding($false)))
    $p = Start-Process -FilePath $dest -ArgumentList "check" -Wait -PassThru -WindowStyle Hidden
    if ($p.ExitCode -ne 0) { [IO.File]::WriteAllText($cfg, $orig, (New-Object Text.UTF8Encoding($false))); Die "La configuración resultante no es válida; se dejó la anterior." }
    Ok "actualizada ($cfg)"
} else {
    Ok "ya al día ($cfg)"
}

# ------------------------------------------------------------------ arranque automático
Say "Arranque automático (tarea '$TaskName')"
$existing = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
$same = $existing -and $existing.Actions.Count -eq 1 -and $existing.Actions[0].Execute -eq $dest -and $existing.Actions[0].Arguments -eq "run"
if (-not $same) {
    $action   = New-ScheduledTaskAction -Execute $dest -Argument "run"
    $trigger  = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
                -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) `
                -MultipleInstances IgnoreNew -Priority 7
    Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Settings $settings `
        -Description "Teco (dev.hugorodriguez.teco)" -Force | Out-Null
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

$port = if ($ServeHttpsPort -eq 443) { "" } else { ":$ServeHttpsPort" }
Say "Listo: https://$self$port"
if (Test-Path $zotzExe) { Write-Host "   Zotz: https://${self}:$ZotzHttpsPort/web-index.html" }
