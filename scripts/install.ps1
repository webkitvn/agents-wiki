param(
    [string]$BinDir = "$env:USERPROFILE\.local\bin"
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir
$ManifestPath = Join-Path $RepoRoot "Cargo.toml"

if (-not (Test-Path $ManifestPath)) {
    Write-Error "scripts/install.ps1 must be run from an agents-wiki checkout."
}

New-Item -ItemType Directory -Force -Path $BinDir | Out-Null

cargo build --release --locked --manifest-path $ManifestPath

$Source = Join-Path $RepoRoot "target\release\agents-wiki.exe"
$Target = Join-Path $BinDir "agents-wiki.exe"

Copy-Item -Force $Source $Target

Write-Output "Installed agents-wiki to $Target"
Write-Output ""
Write-Output "If '$BinDir' is not on PATH, add it to your user PATH."
Write-Output ""
Write-Output "Then initialize a vault location:"
Write-Output '  agents-wiki init "$env:USERPROFILE\Documents\agents-wiki"'
