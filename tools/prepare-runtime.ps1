param([switch]$Integration)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$lock = Get-Content (Join-Path $PSScriptRoot 'runtime-lock.json') -Raw | ConvertFrom-Json
$deps = Join-Path $repo 'validation-deps'
$source = Join-Path $deps 'runtime-source'
New-Item -ItemType Directory -Force -Path $deps | Out-Null
if (-not (Test-Path -LiteralPath $source)) {
    git clone --no-checkout $lock.repository $source
    if ($LASTEXITCODE -ne 0) { throw 'Runtime source clone failed' }
    git -C $source checkout --detach $lock.source_commit
    if ($LASTEXITCODE -ne 0) { throw 'Pinned Runtime commit unavailable' }
}
$dirty = git -C $source status --porcelain
if ($LASTEXITCODE -ne 0 -or $dirty) { throw 'Runtime validation source must be clean' }
$head = git -C $source rev-parse HEAD
if ($LASTEXITCODE -ne 0) { throw 'Cannot read Runtime revision' }
if ($head -ne $lock.source_commit) {
    git -C $source fetch --tags origin $lock.source_commit
    if ($LASTEXITCODE -ne 0) { throw 'Pinned Runtime commit unavailable' }
    git -C $source checkout --detach $lock.source_commit
    if ($LASTEXITCODE -ne 0) { throw 'Cannot select pinned Runtime commit' }
}
$env:AYTHER_RUNTIME_SOURCE = $source
if (-not $Integration) { return }
if ($lock.status -ne 'available' -or -not $lock.asset_url -or -not $lock.asset_name -or
    $lock.sha256 -notmatch '^[a-fA-F0-9]{64}$' -or -not $lock.executable) {
    throw 'Release integration BLOCKED: record the versioned asset, SHA-256 and executable in tools/runtime-lock.json'
}
$tagCommit = git -C $source rev-parse ($lock.release_tag + '^{commit}')
if ($LASTEXITCODE -ne 0 -or $tagCommit -ne $lock.source_commit) { throw 'Runtime tag does not match the pinned source commit' }
if ($lock.platform -ne 'windows-x86_64') { throw 'Only Windows x64 integration is configured' }
$archive = Join-Path $deps 'runtime.zip'
if (-not (Test-Path $archive)) {
    gh release download $lock.release_tag --repo Ayther-Dev/AYTHER-Runtime --pattern $lock.asset_name --output $archive
    if ($LASTEXITCODE -ne 0) { throw 'Runtime download failed; authenticate GitHub CLI for repository access' }
}
if ((Get-FileHash $archive -Algorithm SHA256).Hash -ne $lock.sha256) { throw 'Runtime archive checksum mismatch' }
$distribution = Join-Path $deps ('runtime-' + $lock.sha256)
if (Test-Path $distribution) { throw 'Use a fresh validation-deps directory for release integration' }
Expand-Archive -LiteralPath $archive -DestinationPath $distribution
$exe = [IO.Path]::GetFullPath((Join-Path $distribution $lock.executable))
if (-not $exe.StartsWith($distribution + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Runtime executable escapes the distribution directory'
}
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw 'Runtime executable missing from archive' }
$env:AYTHER_RUNTIME_EXE = $exe
$env:AYTHER_TEST_CORE = Join-Path $deps 'play_probe_fixture.dll'
rustc --edition 2024 --crate-type cdylib (Join-Path $PSScriptRoot 'probe_fixture.rs') -o $env:AYTHER_TEST_CORE
if ($LASTEXITCODE -ne 0) { throw 'Probe fixture build failed' }
