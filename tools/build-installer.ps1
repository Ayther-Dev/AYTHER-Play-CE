<#
.SYNOPSIS
Build the Windows installer of AYTHER Play CE with the locked Runtime.

.DESCRIPTION
1. Builds Play CE (release) from a clean, committed checkout.
2. Downloads the Runtime archive named by tools/runtime-lock.json and checks
   its SHA-256 against the lock.
3. Stages one folder: Play CE, the Runtime next to it (executable, DLLs,
   fonts, shaders), the Visual C++ runtime DLLs (app-local), licenses and
   build-info.json.
4. Refuses Visual C++ DLLs older than the toolset that linked any staged
   executable: an older msvcp140.dll can crash a newer build at startup.
5. Compiles installer/ayther-play-ce.iss with Inno Setup 6 and writes the
   setup's SHA-256 next to it.

Output: dist/ayther-play-ce-<version>-windows-x86_64-setup.exe (+ .sha256).

The release workflow runs -Step Stage and then -Step Compile; locally,
-Step All (the default) does both in one go. The setup is not code-signed.

.PARAMETER Step
All (default): stage and compile. Stage: steps 1-4 into
dist/<name>-installer/stage. Compile: step 5 from an existing stage.

.PARAMETER Version
Defaults to the version in Cargo.toml; when given it must match it.

.PARAMETER Iscc
Path to ISCC.exe. Defaults to the usual Inno Setup 6 locations.
#>
[CmdletBinding()]
param(
    [ValidateSet('All', 'Stage', 'Compile')]
    [string]$Step = 'All',
    [string]$Version,
    [string]$Iscc
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows) { throw 'The installer is built on Windows.' }

$repo = Split-Path $PSScriptRoot -Parent
Push-Location $repo
try {
    # -- Inputs -------------------------------------------------------------
    $metadata = cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed.' }
    $package = $metadata.packages | Where-Object name -eq 'ayther_play_ce'
    if (-not $Version) { $Version = $package.version }
    if ($package.version -ne $Version) { throw "Version $Version does not match Cargo.toml ($($package.version))." }

    $doStage = $Step -ne 'Compile'
    $doCompile = $Step -ne 'Stage'

    if ($doCompile -and -not $Iscc) {
        $Iscc = @(
            "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
            "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
            "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
        ) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    }
    if ($doCompile -and (-not $Iscc -or -not (Test-Path -LiteralPath $Iscc))) {
        throw 'Inno Setup 6 (ISCC.exe) not found. Install it (winget install JRSoftware.InnoSetup) or pass -Iscc.'
    }

    $dirty = git status --porcelain --untracked-files=no
    if ($LASTEXITCODE -ne 0) { throw 'git status failed.' }
    if ($dirty) { throw 'The installer is built from a clean committed checkout; commit or stash first.' }
    $commit = (git rev-parse HEAD).Trim()

    $name = "ayther-play-ce-$Version-windows-x86_64"
    $dist = Join-Path $repo 'dist'
    $work = Join-Path $dist "$name-installer"
    $stage = Join-Path $work 'stage'
    $setup = Join-Path $dist "$name-setup.exe"
    if ($doCompile -and (Test-Path -LiteralPath $setup)) { throw "$setup already exists; remove it or bump the version." }

    $lock = Get-Content -LiteralPath (Join-Path $repo 'tools/runtime-lock.json') -Raw | ConvertFrom-Json
    if ($lock.status -ne 'available') { throw "Runtime lock status is '$($lock.status)'." }

    if ($doStage) {
        if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force }
        New-Item -ItemType Directory -Force -Path $stage | Out-Null

        # -- Play CE -------------------------------------------------------------
        cargo build -p ayther_play_ce --release --locked
        if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
        $playExe = Join-Path $metadata.target_directory 'release/ayther_play_ce.exe'
        Copy-Item -LiteralPath $playExe -Destination $stage

        # -- Runtime (locked, verified) -----------------------------------------
        $runtimeZip = Join-Path $work $lock.asset_name
        Write-Host "Downloading $($lock.asset_url)..."
        Invoke-WebRequest -Uri $lock.asset_url -OutFile $runtimeZip
        $actual = (Get-FileHash -LiteralPath $runtimeZip -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $lock.sha256) { throw "Runtime SHA-256 mismatch: lock $($lock.sha256), got $actual." }
        $runtimeDir = Join-Path $work 'runtime'
        Expand-Archive -LiteralPath $runtimeZip -DestinationPath $runtimeDir
        $runtimeExe = Join-Path $runtimeDir $lock.executable
        if (-not (Test-Path -LiteralPath $runtimeExe)) { throw "The Runtime archive has no $($lock.executable)." }
        # Everything next to the Runtime executable (DLLs, fonts/, shaders/) goes
        # next to Play CE: Play looks for ayther_runtime.exe in its own folder.
        Get-ChildItem -LiteralPath (Split-Path $runtimeExe) | Copy-Item -Destination $stage -Recurse
        $licenses = Join-Path $stage 'licenses'
        New-Item -ItemType Directory -Force -Path $licenses | Out-Null
        $runtimeLicenses = Join-Path $runtimeDir 'share/licenses'
        if (Test-Path -LiteralPath $runtimeLicenses) {
            Get-ChildItem -LiteralPath $runtimeLicenses | Copy-Item -Destination $licenses -Recurse
        }
        $runtimeMetadata = Join-Path $runtimeDir 'share/ayther-runtime/ayther-runtime-package.json'
        if (Test-Path -LiteralPath $runtimeMetadata) {
            $runtimePackage = Get-Content -LiteralPath $runtimeMetadata -Raw | ConvertFrom-Json
            if (-not $runtimePackage.PSObject.Properties['tessera_provider'] -or -not $runtimePackage.tessera_provider) {
                throw 'The locked Runtime has no Tessera provider; Play CE defaults to Tessera.'
            }
        }

        # -- Play CE licenses ------------------------------------------------------
        $playLicenses = Join-Path $licenses 'ayther-play-ce'
        New-Item -ItemType Directory -Force -Path $playLicenses | Out-Null
        Copy-Item -LiteralPath (Join-Path $repo 'LICENSE') -Destination $playLicenses
        Copy-Item -LiteralPath (Join-Path $repo 'fonts') -Destination (Join-Path $playLicenses 'font-notices') -Recurse

        # -- Visual C++ runtime (app-local) --------------------------------------
        $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
        $vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if (-not $vs) { throw 'Visual Studio with the C++ x64 tools is required for the Visual C++ runtime DLLs.' }
        $crt = Get-ChildItem -LiteralPath (Join-Path $vs 'VC/Redist/MSVC') -Directory |
            Where-Object { $_.Name -match '^\d+\.\d+\.\d+$' } |
            Sort-Object { [version]$_.Name } -Descending |
            ForEach-Object { Get-ChildItem -LiteralPath (Join-Path $_.FullName 'x64') -Directory -Filter 'Microsoft.VC*.CRT' -ErrorAction SilentlyContinue } |
            Select-Object -First 1
        if (-not $crt) { throw "No Microsoft.VC*.CRT folder under $vs\VC\Redist\MSVC." }
        $crtDlls = Get-ChildItem -LiteralPath $crt.FullName -Filter '*.dll' |
            Where-Object { $_.Name -match '^(msvcp140|vcruntime140|concrt140)' }
        $crtDlls | Copy-Item -Destination $stage
        $crtVersion = [version](Get-Item -LiteralPath (Join-Path $crt.FullName 'msvcp140.dll')).VersionInfo.FileVersion

        # The linker version in each PE header is the toolset that built it.
        function Get-LinkerVersion([string]$Path) {
            $bytes = [IO.File]::ReadAllBytes($Path)
            $pe = [BitConverter]::ToInt32($bytes, 0x3C)
            $optional = $pe + 4 + 20
            return [version]::new($bytes[$optional + 2], $bytes[$optional + 3])
        }
        foreach ($exe in Get-ChildItem -LiteralPath $stage -Filter '*.exe') {
            $linker = Get-LinkerVersion $exe.FullName
            if ($linker.Major -eq 14 -and [version]::new($crtVersion.Major, $crtVersion.Minor) -lt $linker) {
                throw "$($exe.Name) was linked by MSVC $linker but the Visual C++ DLLs are $crtVersion. Update Visual Studio."
            }
            Write-Host "  $($exe.Name): MSVC linker $linker; Visual C++ DLLs $crtVersion"
        }

        # -- Build info -------------------------------------------------------------
        [ordered]@{
            version = $Version
            source_commit = $commit
            platform = 'windows-x86_64'
            rustc = (rustc --version)
            runtime = [ordered]@{ release_tag = $lock.release_tag; source_commit = $lock.source_commit; sha256 = $lock.sha256 }
            vc_runtime = $crtVersion.ToString()
        } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $stage 'build-info.json') -Encoding utf8
        if (-not $doCompile) { Write-Output $stage; return }
    }

    # -- Setup ------------------------------------------------------------------
    $buildInfo = Join-Path $stage 'build-info.json'
    if (-not (Test-Path -LiteralPath $buildInfo)) { throw "No staged build at $stage; run -Step Stage first." }
    $staged = Get-Content -LiteralPath $buildInfo -Raw | ConvertFrom-Json
    if ($staged.version -ne $Version -or $staged.source_commit -ne $commit) {
        throw "The stage is $($staged.version) at $($staged.source_commit), not $Version at $commit; stage again."
    }
    & $Iscc /Qp "/DAppVersion=$Version" "/DRuntimeTag=$($lock.release_tag)" "/DStageDir=$stage" `
        "/DOutputDir=$dist" "/DRepoDir=$repo" (Join-Path $repo 'installer/ayther-play-ce.iss')
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed ($LASTEXITCODE)." }
    if (-not (Test-Path -LiteralPath $setup)) { throw "ISCC did not write $setup." }
    $hash = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $(Split-Path $setup -Leaf)" | Set-Content -LiteralPath "$setup.sha256" -Encoding ascii
    Write-Output $setup
} finally {
    Pop-Location
}
