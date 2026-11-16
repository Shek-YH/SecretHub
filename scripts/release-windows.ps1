<#
.SYNOPSIS
  Build, sign and publish a SecretHub Windows release (NSIS installer + updater manifest).

.DESCRIPTION
  1. reads the version from apps/desktop/src-tauri/tauri.conf.json
  2. npm --prefix apps/mcp run build:sea            (standalone MCP sidecar exe)
  3. npm --prefix apps/desktop run tauri build      (signed with the updater private key)
  4. writes latest.json next to the installer       (version, notes, pub_date, windows-x86_64 signature + url)
  5. uploads installer + latest.json with scp       (skipped with -DryRun: the commands are only printed)

  The updater private key never lives in the repo: pass -KeyPath or keep it at
  C:\Users\Administrator\.tauri\secrethub-updater.key (generated without a password).
  The endpoint the app polls is plugins.updater.endpoints in tauri.conf.json
  (https://updates.9aizhuan.com/secrethub/latest.json). -BaseUrl must be the directory that serves
  latest.json and the installer; the defaults publish to that directory on the tx-bt server.

.EXAMPLE
  powershell -File scripts\release-windows.ps1 -Notes "..." -DryRun
#>
param(
  [string]$SshHost = 'tx-bt',
  [string]$RemoteDir = '/www/wwwroot/updates.9aizhuan.com/secrethub',
  [string]$BaseUrl = 'https://updates.9aizhuan.com/secrethub',
  [string]$Notes = '',
  [string]$KeyPath = (Join-Path $env:USERPROFILE '.tauri\secrethub-updater.key'),
  [switch]$DryRun,
  [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$conf = Get-Content (Join-Path $repo 'apps\desktop\src-tauri\tauri.conf.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$version = $conf.version
if (-not $version) { throw 'version not found in tauri.conf.json' }
$BaseUrl = $BaseUrl.TrimEnd('/')
$RemoteDir = $RemoteDir.TrimEnd('/')
if (-not (Test-Path $KeyPath)) { throw "updater private key not found: $KeyPath" }
Write-Host "SecretHub $version  (DryRun=$($DryRun.IsPresent))"

function Invoke-Step([string]$What, [scriptblock]$Block) {
  Write-Host "==> $What"
  & $Block
  if ($LASTEXITCODE -ne 0) { throw "$What failed (exit $LASTEXITCODE)" }
}

if (-not $SkipBuild) {
  Invoke-Step 'build MCP sidecar' { npm --prefix (Join-Path $repo 'apps\mcp') run build:sea }
  # The key has no password, but the CLI prompts (and hangs) unless TAURI_SIGNING_PRIVATE_KEY_PASSWORD exists
  # as an EMPTY variable. PowerShell deletes empty env vars, so start the build through ProcessStartInfo.
  $psi = New-Object System.Diagnostics.ProcessStartInfo 'cmd.exe'
  $psi.WorkingDirectory = $repo
  $psi.Arguments = '/c npm --prefix apps\desktop run tauri build'
  $psi.UseShellExecute = $false
  $psi.EnvironmentVariables['TAURI_SIGNING_PRIVATE_KEY'] = (Get-Content $KeyPath -Raw).Trim()
  $psi.EnvironmentVariables['TAURI_SIGNING_PRIVATE_KEY_PASSWORD'] = ''
  Write-Host '==> tauri build'
  $build = [System.Diagnostics.Process]::Start($psi)
  $build.WaitForExit()
  if ($build.ExitCode -ne 0) { throw "tauri build failed (exit $($build.ExitCode))" }
}

$bundleDir = Join-Path $repo 'target\release\bundle\nsis'
$installerName = "SecretHub_${version}_x64-setup.exe"
$installer = Join-Path $bundleDir $installerName
$sigFile = "$installer.sig"
if (-not (Test-Path $installer)) { throw "installer not found: $installer" }
if (-not (Test-Path $sigFile)) { throw "signature not found: $sigFile (was the build signed?)" }

$manifest = [ordered]@{
  version  = $version
  notes    = if ($Notes) { $Notes } else { "SecretHub $version" }
  pub_date = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
  platforms = [ordered]@{
    'windows-x86_64' = [ordered]@{
      signature = (Get-Content $sigFile -Raw -Encoding UTF8).Trim()
      url       = "$BaseUrl/$([uri]::EscapeDataString($installerName))"
    }
  }
}
$latest = Join-Path $bundleDir 'latest.json'
[System.IO.File]::WriteAllText($latest, ($manifest | ConvertTo-Json -Depth 5), (New-Object System.Text.UTF8Encoding($false)))
Write-Host "wrote $latest"

# Upload the installer first, latest.json last, so clients never see a manifest that points at a missing file.
$uploads = @(
  @{ Local = $installer; Remote = "${SshHost}:$RemoteDir/$installerName" },
  @{ Local = $latest; Remote = "${SshHost}:$RemoteDir/latest.json" }
)
foreach ($item in $uploads) {
  if ($DryRun) {
    Write-Host "[dry-run] scp `"$($item.Local)`" $($item.Remote)"
  } else {
    Invoke-Step "scp $($item.Local)" { scp $item.Local $item.Remote }
  }
}
# Stable download link for websites and docs: always points at the newest installer.
$linkCmd = "ln -sfn '$installerName' '$RemoteDir/SecretHub-setup-latest.exe'"
if ($DryRun) { Write-Host "[dry-run] ssh $SshHost $linkCmd" } else { Invoke-Step 'link latest installer' { ssh $SshHost $linkCmd } }
Write-Host "download: $BaseUrl/SecretHub-setup-latest.exe"
Write-Host ('done' + $(if ($DryRun) { ' (dry run, nothing uploaded)' } else { '' }))
