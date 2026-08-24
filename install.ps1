[CmdletBinding()]
param(
    [string]$Version = "latest",
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\sonarctl"),
    [switch]$NoPath
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if (-not $env:LOCALAPPDATA) {
    throw "LOCALAPPDATA is not set."
}

$releaseBase = if ($Version -eq "latest") {
    "https://github.com/adrifer/sonarctl/releases/latest/download"
} elseif ($Version -match "^v\d+\.\d+\.\d+$") {
    "https://github.com/adrifer/sonarctl/releases/download/$Version"
} else {
    throw "Version must be 'latest' or a tag such as 'v0.1.0'."
}

$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) "sonarctl-$([guid]::NewGuid())"
$downloadedExe = Join-Path $tempDir "sonarctl.exe"
$downloadedChecksum = Join-Path $tempDir "sonarctl.exe.sha256"

try {
    New-Item -ItemType Directory -Path $tempDir | Out-Null

    $webRequest = @{
        UseBasicParsing = $true
        ErrorAction = "Stop"
    }
    Invoke-WebRequest @webRequest -Uri "$releaseBase/sonarctl.exe" -OutFile $downloadedExe
    Invoke-WebRequest @webRequest -Uri "$releaseBase/sonarctl.exe.sha256" -OutFile $downloadedChecksum

    $expectedHash = ((Get-Content -Raw $downloadedChecksum).Trim() -split "\s+")[0]
    if ($expectedHash -notmatch "^[a-fA-F0-9]{64}$") {
        throw "The release checksum file is invalid."
    }

    $actualHash = (Get-FileHash -Algorithm SHA256 $downloadedExe).Hash
    if ($actualHash -ne $expectedHash) {
        throw "Checksum verification failed. Expected $expectedHash, received $actualHash."
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $installedExe = Join-Path $InstallDir "sonarctl.exe"
    Copy-Item -Force $downloadedExe $installedExe

    if (-not $NoPath) {
        $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
        $pathEntries = @($userPath -split ";" | Where-Object { $_ })
        if (-not ($pathEntries | Where-Object { $_.TrimEnd("\") -ieq $InstallDir.TrimEnd("\") })) {
            $newUserPath = (@($pathEntries) + $InstallDir) -join ";"
            [Environment]::SetEnvironmentVariable("Path", $newUserPath, "User")
        }

        if (-not (($env:Path -split ";") | Where-Object { $_.TrimEnd("\") -ieq $InstallDir.TrimEnd("\") })) {
            $env:Path = "$InstallDir;$env:Path"
        }
    }

    Write-Host "Installed sonarctl to $installedExe"
    if (-not $NoPath) {
        Write-Host "The install directory is on your user PATH. Open a new terminal, then run: sonarctl doctor"
    }
} finally {
    if (Test-Path $tempDir) {
        Remove-Item -Recurse -Force $tempDir
    }
}
