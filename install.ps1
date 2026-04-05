param(
    [string]$Version = $env:REPOSWEEP_VERSION,
    [string]$InstallDir = $env:REPOSWEEP_INSTALL_DIR,
    [string]$Repo = $(if ($env:REPOSWEEP_REPO) { $env:REPOSWEEP_REPO } else { "youssefsz/reposweep" }),
    [switch]$FromSource
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "latest"
}

$BinaryName = "reposweep.exe"

function Test-PathContainsDir {
    param([string]$Candidate)

    $target = [System.IO.Path]::GetFullPath($Candidate).TrimEnd('\')
    foreach ($entry in ($env:PATH -split ';')) {
        if ([string]::IsNullOrWhiteSpace($entry)) {
            continue
        }

        $normalized = [System.IO.Path]::GetFullPath($entry).TrimEnd('\')
        if ($normalized.Equals($target, [System.StringComparison]::OrdinalIgnoreCase)) {
            return $true
        }
    }

    return $false
}

function Resolve-InstallDir {
    $candidates = @(
        (Join-Path $HOME ".cargo\bin"),
        (Join-Path $HOME ".local\bin")
    )

    foreach ($candidate in $candidates) {
        if (Test-PathContainsDir -Candidate $candidate) {
            return $candidate
        }
    }

    return (Join-Path $HOME ".local\bin")
}

function Resolve-Version {
    param([string]$RequestedVersion, [string]$Repository)

    if ($RequestedVersion -ne "latest") {
        return $RequestedVersion
    }

    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repository/releases/latest"
    if (-not $release.tag_name) {
        throw "Failed to resolve the latest release tag for $Repository"
    }
    return $release.tag_name
}

function Install-FromSource {
    param([string]$Repository, [string]$Destination)

    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw "cargo is required for --FromSource installs"
    }

    $CargoRoot = if ($env:REPOSWEEP_CARGO_ROOT) { $env:REPOSWEEP_CARGO_ROOT } else { Join-Path $HOME ".cargo" }
    cargo install --locked --git "https://github.com/$Repository.git" --bin reposweep --root $CargoRoot reposweep

    $CargoBinary = Join-Path $CargoRoot "bin\reposweep.exe"
    New-Item -ItemType Directory -Force -Path $Destination | Out-Null
    Copy-Item $CargoBinary (Join-Path $Destination $BinaryName) -Force
}

function Install-FromRelease {
    param([string]$RequestedVersion, [string]$Repository, [string]$Destination)

    $ResolvedVersion = Resolve-Version -RequestedVersion $RequestedVersion -Repository $Repository
    $Target = "x86_64-pc-windows-msvc"
    $ArchiveName = "reposweep-$ResolvedVersion-$Target.zip"
    $Url = "https://github.com/$Repository/releases/download/$ResolvedVersion/$ArchiveName"
    $TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("reposweep-install-" + [System.Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $TempDir | Out-Null

    try {
        $ArchivePath = Join-Path $TempDir $ArchiveName
        Write-Host "Downloading $ArchiveName"
        Invoke-WebRequest -Uri $Url -OutFile $ArchivePath

        Expand-Archive -Path $ArchivePath -DestinationPath $TempDir -Force
        $BinaryPath = Get-ChildItem -Path $TempDir -Filter $BinaryName -Recurse | Select-Object -First 1
        if (-not $BinaryPath) {
            throw "Archive did not contain $BinaryName"
        }

        New-Item -ItemType Directory -Force -Path $Destination | Out-Null
        Copy-Item $BinaryPath.FullName (Join-Path $Destination $BinaryName) -Force
    }
    finally {
        if (Test-Path $TempDir) {
            Remove-Item -Recurse -Force $TempDir
        }
    }
}

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    $InstallDir = Resolve-InstallDir
}

if ($FromSource) {
    Install-FromSource -Repository $Repo -Destination $InstallDir
}
else {
    Install-FromRelease -RequestedVersion $Version -Repository $Repo -Destination $InstallDir
}

Write-Host ""
Write-Host "Installed $BinaryName to $(Join-Path $InstallDir $BinaryName)"
if (-not (($env:PATH -split ';') -contains $InstallDir)) {
    Write-Host "Add this directory to your PATH if needed:"
    Write-Host "  $InstallDir"
}
