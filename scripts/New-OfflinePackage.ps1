# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

#requires -Version 5.1
[CmdletBinding()]
param(
    [ValidateSet('x86', 'x64', 'ARM64')][string[]] $Platform = @('x64', 'ARM64'),
    [string] $OutputDirectory = [Environment]::GetFolderPath('Desktop'),
    [string[]] $PackageRoot = @(),
    [switch] $AsDirectory,
    [switch] $NoInstall
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'DevBuildTools.ps1')
. (Join-Path $PSScriptRoot 'TestBuildIdentity.ps1')
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$sourceHashes = Get-TestSourceHashes Product
if (@($Platform | Select-Object -Unique).Count -ne $Platform.Count -or
    ($PackageRoot.Count -and $PackageRoot.Count -ne $Platform.Count)) {
    throw 'Supply unique platforms and, when reusing CI builds, one package root per platform in the same order.'
}
$commit = (& git -C $repo rev-parse --short HEAD)
if ($LASTEXITCODE) { throw 'Cannot identify the source commit.' }
$buildId = [Guid]::NewGuid().ToString('N')
$run = Join-Path $repo "artifacts/offline/$buildId"
$bundle = if ($AsDirectory) { [IO.Path]::GetFullPath($OutputDirectory) } else { Join-Path $run 'Notepads-V2-Offline' }
if (Test-Path -LiteralPath $bundle) { throw 'Offline output already exists. Choose a new directory to avoid mixing builds.' }
$null = New-Item -ItemType Directory -Path $bundle -Force
$null = New-Item -ItemType Directory -Path $run -Force

function Get-ProductionPayload([string] $Root, [string] $Architecture) {
    $uploads = @(Get-ChildItem -LiteralPath $Root -Recurse -File | Where-Object Extension -in '.msixupload', '.appxupload')
    if ($uploads.Count -ne 1) { throw 'Expected exactly one fresh Production upload package.' }
    $archive = [IO.Compression.ZipFile]::OpenRead($uploads[0].FullName)
    try {
        $bundles = @($archive.Entries | Where-Object Name -Match '\.(msixbundle|appxbundle)$')
        if ($bundles.Count -ne 1) { throw 'Expected exactly one bundle in the upload package.' }
        $bundlePath = Join-Path $run "production-$Architecture.msixbundle"
        [IO.Compression.ZipFileExtensions]::ExtractToFile($bundles[0], $bundlePath)
    } finally { $archive.Dispose() }
    $archive = [IO.Compression.ZipFile]::OpenRead($bundlePath)
    try {
        $entry = $archive.GetEntry('AppxMetadata/AppxBundleManifest.xml')
        if (!$entry) { throw 'Missing bundle manifest.' }
        $reader = [IO.StreamReader]::new($entry.Open())
        try { [xml]$manifest = $reader.ReadToEnd() } finally { $reader.Dispose() }
        $payloads = @($manifest.Bundle.Packages.Package)
        # A loose layout must contain all resources. Never silently discard a
        # language/scale resource pack while extracting an application payload.
        if ($payloads.Count -ne 1 -or $payloads[0].Type -ne 'application' -or $payloads[0].Architecture -ine $Architecture) {
            throw 'Offline output requires one architecture with all resources in its main package; disable auto resource splitting.'
        }
        $payload = Join-Path $run "production-$Architecture.msix"
        [IO.Compression.ZipFileExtensions]::ExtractToFile($archive.GetEntry($payloads[0].FileName), $payload)
        return $payload
    } finally { $archive.Dispose() }
}

$builds = @()
foreach ($architecture in $Platform | Select-Object -Unique) {
    if ($PackageRoot) {
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $packages = $PackageRoot[[array]::IndexOf($Platform, $architecture)]
        $receipt = Join-Path (Split-Path ([IO.Path]::GetFullPath($packages)) -Parent) 'sources.json'
        if (!(Test-Path -LiteralPath $receipt -PathType Leaf)) { throw 'Reused CI packages require their source fingerprint receipt.' }
        $recorded = Get-Content -LiteralPath $receipt -Raw | ConvertFrom-Json
        $properties = @($recorded.PSObject.Properties)
        if ($properties.Count -ne $sourceHashes.Count -or @($properties | Where-Object { $sourceHashes[$_.Name] -ne $_.Value }).Count) {
            throw 'Reused CI packages do not match the current product sources.'
        }
        $payload = Get-ProductionPayload $packages $architecture
        $layout = Join-Path $run "app-$architecture"
        Expand-DevArchive $payload $layout
        $build = [pscustomobject]@{ Platform = $architecture; Configuration = 'Production'; Layout = $layout; Package = $payload; PackageRoot = $packages }
    }
    else {
        $build = & (Join-Path $PSScriptRoot 'Run-Dev.ps1') -Platform $architecture -Configuration Production -BuildOnly -PassThru -NoInstall:$NoInstall
    }
    if (!$build.Layout -or $build.Platform -ne $architecture) { throw 'The development build did not return a matching layout.' }
    & (Join-Path $PSScriptRoot 'Test-AppPackage.ps1') -Path $build.Package -ExpectedArchitectures @($architecture) | Write-Host
    $app = Join-Path $bundle "App/$architecture"
    $frameworks = Join-Path $bundle "Dependencies/$architecture"
    $null = New-Item -ItemType Directory -Path $frameworks -Force
    Copy-Item -LiteralPath $build.Layout -Destination $app -Recurse
    [xml]$manifest = Get-Content -LiteralPath (Join-Path $app 'AppxManifest.xml') -Raw
    Assert-DevManifest $manifest $architecture '19282JackieLiu.Notepads-Beta'
    if ($builds.Count -and $manifest.Package.Identity.Version -ne $version) { throw 'Offline architectures must have the same version.' }
    $version = $manifest.Package.Identity.Version
    $number = 0
    foreach ($dependency in $manifest.Package.Dependencies.PackageDependency) {
        $path = Find-DevFrameworkPackage $dependency $build.PackageRoot $architecture
        Copy-Item -LiteralPath $path -Destination (Join-Path $frameworks ("$number-" + [IO.Path]::GetFileName($path)))
        $number++
    }
    $builds += $build
}
$after = Get-TestSourceHashes Product
if ($sourceHashes.Count -ne $after.Count -or @($sourceHashes.Keys | Where-Object { $sourceHashes[$_] -ne $after[$_] }).Count) {
    throw 'Product sources changed during packaging; do not distribute mixed builds.'
}
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'DevBuildTools.ps1') -Destination $bundle
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Install-Offline.ps1') -Destination (Join-Path $bundle 'Install.ps1')
Copy-Item -LiteralPath (Join-Path $repo 'LICENSE.txt') -Destination $bundle
$launcher = @'
@echo off
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0Install.ps1"
set "result=%errorlevel%"
echo.
if not "%result%"=="0" echo Installation failed. See the error above.
pause
exit /b %result%
'@
[IO.File]::WriteAllText((Join-Path $bundle 'Start.cmd'), $launcher.Replace("`r`n", "`n").Replace("`n", "`r`n"), [Text.Encoding]::ASCII)
$instructions = @"
Notepads V2 offline preview - work in progress
Version: $($manifest.Package.Identity.Version) | Source: $commit
Architectures: $($builds.Platform -join ', ')

1. Extract the ENTIRE ZIP to a permanent local folder. Do not run from inside the ZIP.
2. Close any running Notepads instance of this preview, then double-click Start.cmd.
3. If Windows Developer Mode is disabled, Windows asks for administrator approval.
   An organization policy may block this setting; the installer then stops.
4. Keep the extracted folder while this development version is registered.
   Later launches are also available through Notepads in the Start menu.

No internet, Visual Studio, Build Tools, .NET installation or signing certificate
is needed on the test machine. All framework dependencies are included.
Windows 10 build 17763+ or Windows 11; a compatible architecture is selected automatically.
The script makes no download requests. Windows certificate/security policy can
still prevent deployment on a particular offline or managed machine.

This is the Production Native AOT build, displayed as Notepads (no -Dev label).
Package identity: 19282JackieLiu.Notepads-Beta. It can replace an existing package
with that identity and uses its data. Other Notepads identities are separate.
Back up valuable documents before trying an unreleased version. If Windows refuses
an identity/version conflict, stop; this script does not remove the existing app.
It does not uninstall apps, reset data or forcibly close running applications.

Validation only (no changes):
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Install.ps1 -CheckOnly
Install without launching: add -NoLaunch instead.

Normal removal: Windows Settings > Apps > Installed apps > Notepads > Uninstall.
Standard uninstallation can remove this development app's data; export/save work first.
Send the source ID, Windows version, architecture and exact error when reporting bugs.
"@
[IO.File]::WriteAllText((Join-Path $bundle 'README.txt'), $instructions, [Text.UTF8Encoding]::new($true))
$files = @(Get-ChildItem -LiteralPath $bundle -Recurse -File | Sort-Object FullName | ForEach-Object {
    [ordered]@{ Path = $_.FullName.Substring($bundle.Length + 1).Replace('\', '/'); Sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }
})
$info = [ordered]@{ Format = 1; Version = $manifest.Package.Identity.Version; Commit = $commit; Platforms = @($builds.Platform); Files = $files }
$info | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $bundle 'bundle.json') -Encoding utf8
$sourceHashes | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $run 'sources.json') -Encoding utf8
$builds | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $run 'builds.json') -Encoding utf8

# CI validates all three architectures on its x64 runner without deploying them.
foreach ($architecture in $Platform | Select-Object -Unique) {
    & (Join-Path $bundle 'Install.ps1') -CheckOnly -Platform $architecture | Write-Host
}
if ($AsDirectory) { Write-Output $bundle; return }

$null = New-Item -ItemType Directory -Path $OutputDirectory -Force
$name = 'Notepads-V2-Offline-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + $commit + '.zip'
$zip = Join-Path ([IO.Path]::GetFullPath($OutputDirectory)) $name
Add-Type -AssemblyName System.IO.Compression.FileSystem
[IO.Compression.ZipFile]::CreateFromDirectory($bundle, $zip, [IO.Compression.CompressionLevel]::Optimal, $false)
$hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash
[IO.File]::WriteAllText(($zip + '.sha256'), "$hash  $name`r`n", [Text.Encoding]::ASCII)
Write-Output "Offline package: $zip"
Write-Output "SHA-256: $hash"
