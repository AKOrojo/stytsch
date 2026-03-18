# stytsch installer for Windows
# Usage: irm https://raw.githubusercontent.com/AKOrojo/stytsch/main/installer/install.ps1 | iex

$ErrorActionPreference = "Stop"
$repo = "AKOrojo/stytsch"
$installDir = "$env:LOCALAPPDATA\stytsch\bin"

Write-Host "stytsch installer" -ForegroundColor Cyan
Write-Host ""

# Check prerequisites
$hasClink = Get-Command clink -ErrorAction SilentlyContinue
if (-not $hasClink) {
    Write-Host "[!] Clink is required but not installed." -ForegroundColor Yellow
    Write-Host "    Install it: scoop install clink" -ForegroundColor Yellow
    Write-Host "    Or download from: https://github.com/chrisant996/clink/releases" -ForegroundColor Yellow
    Write-Host ""
}

$hasFzf = Get-Command fzf -ErrorAction SilentlyContinue
if (-not $hasFzf) {
    Write-Host "[!] fzf is required but not installed." -ForegroundColor Yellow
    Write-Host "    Install it: scoop install fzf" -ForegroundColor Yellow
    Write-Host ""
}

# Get latest release
Write-Host "[..] Finding latest release..."
$release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest"
$asset = $release.assets | Where-Object { $_.name -like "stytsch.exe" } | Select-Object -First 1

if (-not $asset) {
    $asset = $release.assets | Where-Object { $_.name -like "*x86_64-windows.zip" } | Select-Object -First 1
}

if (-not $asset) {
    Write-Host "[ERROR] No release asset found." -ForegroundColor Red
    exit 1
}

# Download
Write-Host "[..] Downloading $($asset.name)..."
New-Item -ItemType Directory -Path $installDir -Force | Out-Null

if ($asset.name -like "*.zip") {
    $zipPath = "$env:TEMP\stytsch.zip"
    Invoke-WebRequest $asset.browser_download_url -OutFile $zipPath
    Expand-Archive $zipPath -DestinationPath $installDir -Force
    Remove-Item $zipPath
} else {
    Invoke-WebRequest $asset.browser_download_url -OutFile "$installDir\stytsch.exe"
}

Write-Host "[OK] Installed to $installDir" -ForegroundColor Green

# Add to PATH if needed
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$installDir", "User")
    $env:Path += ";$installDir"
    Write-Host "[OK] Added to PATH" -ForegroundColor Green
}

# Run install to set up Clink plugin
Write-Host ""
& "$installDir\stytsch.exe" install

Write-Host ""
Write-Host "Done! Open a new cmd.exe window to start using stytsch." -ForegroundColor Green
Write-Host "  Up Arrow / Ctrl+R  -> fuzzy search history" -ForegroundColor Cyan
Write-Host "  Enter              -> run selected command" -ForegroundColor Cyan
Write-Host "  Tab                -> paste for editing" -ForegroundColor Cyan
Write-Host "  Ctrl+Q             -> toggle tracking" -ForegroundColor Cyan
