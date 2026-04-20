# KAIRO-WIN Deployment Script (Standard Edition)
# Target: C:\Program Files\THP-lab\KAIRO-FW
# Requirement: Run as Administrator

$ErrorActionPreference = "Stop"

$VendorName = "THP-lab"
$AppName = "KAIRO-FW"
$InstallRoot = Join-Path $env:ProgramFiles $VendorName
$InstallDir = Join-Path $InstallRoot $AppName
$ServiceName = "kairo-win-service"

# 1. Create Directories (The "Holy Ground")
if (-not (Test-Path $InstallDir)) {
    Write-Host "[*] Creating installation directory: $InstallDir"
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
}

# 2. Preparation of Binaries and Docs
$SourceExe = Join-Path $PSScriptRoot "target\release\kairo-win-service.exe"
$SourceCli = Join-Path $PSScriptRoot "target\release\kairo-win-f.exe"
$SourceDoc = Join-Path $PSScriptRoot "KAIRO_WIN_REFERENCE.md"

if (-not (Test-Path $SourceExe)) {
    Write-Error "Executable not found: $SourceExe. Please run 'cargo build --release' first."
    exit 1
}

# 3. Stop existing service if any
$Service = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($Service) {
    Write-Host "[*] Stopping existing service..."
    Stop-Service -Name $ServiceName -Force -ErrorAction SilentlyContinue
    # Small delay to ensure process is released
    Start-Sleep -Seconds 2
}

# 4. Copying Files (Deployment)
Write-Host "[*] Deploying KAIRO-WIN binaries..."
Copy-Item -Path $SourceExe -Destination (Join-Path $InstallDir "kairo-win-service.exe") -Force
if (Test-Path $SourceCli) {
    Copy-Item -Path $SourceCli -Destination (Join-Path $InstallDir "kairo-win-f.exe") -Force
}
Copy-Item -Path $SourceDoc -Destination (Join-Path $InstallDir "REFERENCE.md") -Force

# 5. Service Registration and Auto-Start Configuration
Write-Host "[*] Registering Windows Service (Automatic Start)..."
if ($Service) {
    # Update existing service config
    sc.exe config $ServiceName binPath= "$(Join-Path $InstallDir 'kairo-win-service.exe')" start= auto
} else {
    # Create new service
    sc.exe create $ServiceName binPath= "$(Join-Path $InstallDir 'kairo-win-service.exe')" start= auto
}

# Set Description
sc.exe description $ServiceName "KAIRO-WIN - Absolute AI-Proxy Shield (THP-lab)"

# 6. Activation
Write-Host "[*] Starting KAIRO-WIN service..."
Start-Service -Name $ServiceName

Write-Host "`n=== KAIRO-WIN Deployment Successful ==="
Write-Host "Location: $InstallDir"
Write-Host "Service:  $ServiceName (Status: Running, Startup: Automatic)"
Write-Host "Reference: REFERENCE.md has been copied to the install directory."
Write-Host "========================================"
