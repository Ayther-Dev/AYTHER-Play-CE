param([Parameter(Mandatory)][string]$Version)
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^\d+\.\d+\.\d+(-[a-zA-Z0-9.-]+)?$') { throw 'Invalid package version' }
$repo = Split-Path $PSScriptRoot -Parent
Push-Location $repo
try {
    $metadata = cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed' }
    $package = $metadata.packages | Where-Object name -eq 'ayther_play_ce'
    if ($package.version -ne $Version) { throw 'Version must match Cargo.toml' }
    cargo build -p ayther_play_ce --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    $name = "ayther-play-ce-$Version-windows-x86_64"
    $destination = Join-Path $repo "dist/$name"
    $archive = "$destination.zip"
    if ((Test-Path $destination) -or (Test-Path $archive)) { throw 'Package already exists; use a fresh output directory' }
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    Copy-Item (Join-Path $metadata.target_directory 'release/ayther_play_ce.exe') $destination
    Copy-Item (Join-Path $repo 'LICENSE'), (Join-Path $repo 'README.md') $destination
    Copy-Item (Join-Path $repo 'fonts') (Join-Path $destination 'font-notices') -Recurse
    Copy-Item (Join-Path $repo 'docs/RELEASE.md') $destination
    $commit = git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Cannot record source commit' }
    $dirty = git status --porcelain
    if ($LASTEXITCODE -ne 0 -or $dirty) { throw 'Packaging requires a clean committed checkout' }
    @{
        version = $Version; source_commit = $commit; platform = 'windows-x86_64'
        rustc = (rustc --version); runtime = 'external; see tools/runtime-lock.json'
    } | ConvertTo-Json | Set-Content (Join-Path $destination 'build-info.json')
    Compress-Archive -Path "$destination/*" -DestinationPath $archive
    (Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant() | Set-Content "$archive.sha256"
    Write-Output $archive
} finally { Pop-Location }
