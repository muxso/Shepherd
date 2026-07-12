# Shepherd agent-runtime — Windows one-click installer.
#
# Usage (run in PowerShell):
#   irm https://raw.githubusercontent.com/muxso/Shepherd/main/scripts/install.ps1 | iex
#
# What it does:
#   1. resolves the latest GitHub release of muxso/Shepherd
#   2. downloads the x86_64-pc-windows-msvc zip asset
#   3. extracts to $env:LOCALAPPDATA\shepherd
#   4. adds that dir to the user PATH (if missing)
$ErrorActionPreference = 'Stop'

$Repo   = 'muxso/Shepherd'
$ApiUrl = "https://api.github.com/repos/$Repo/releases/latest"

Write-Host 'Fetching latest Shepherd agent-runtime release...' -ForegroundColor Cyan
$release = Invoke-RestMethod -Uri $ApiUrl -Headers @{ 'User-Agent' = 'shepherd-install' }
$version = $release.tag_name
Write-Host "Latest version: $version" -ForegroundColor White

$asset = $release.assets |
  Where-Object { $_.name -like 'agent-runtime-x86_64-pc-windows-msvc.zip' } |
  Select-Object -First 1
if (-not $asset) {
  Write-Error "No Windows asset found for $version"
  exit 1
}

$InstallDir = Join-Path $env:LOCALAPPDATA 'shepherd'
$Zip       = Join-Path $env:TEMP 'agent-runtime.zip'

Write-Host "Downloading $($asset.name) ..." -ForegroundColor Cyan
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $Zip

if (Test-Path $InstallDir) { Remove-Item $InstallDir -Recurse -Force }
New-Item -ItemType Directory -Path $InstallDir | Out-Null
Expand-Archive -Path $Zip -DestinationPath $InstallDir -Force
Remove-Item $Zip -Force

$Exe = Join-Path $InstallDir 'agent-runtime.exe'
if (-not (Test-Path $Exe)) {
  Write-Error 'agent-runtime.exe not found after extraction'
  exit 1
}

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -notlike "*$InstallDir*") {
  [Environment]::SetEnvironmentVariable('Path', "$userPath;$InstallDir", 'User')
  Write-Host "Added $InstallDir to user PATH. Restart your terminal to use it." -ForegroundColor Yellow
} else {
  Write-Host 'Already on PATH.' -ForegroundColor Green
}

Write-Host "`nInstalled agent-runtime $version to $Exe" -ForegroundColor Green
Write-Host 'Next: open a new terminal and run  agent-runtime --help' -ForegroundColor Cyan
