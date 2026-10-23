# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Copied to the root of the offline distribution by New-OfflinePackage.ps1.
#requires -Version 5.1
[CmdletBinding()]
param(
    [switch] $CheckOnly, [switch] $NoLaunch,
    [ValidateSet('Auto', 'x86', 'x64', 'ARM64')][string] $Platform = 'Auto'
)
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne 'Win32NT') { throw 'This package requires Windows.' }
$root = [IO.Path]::GetFullPath($PSScriptRoot).TrimEnd('\') + '\'
$info = Get-Content -LiteralPath (Join-Path $root 'bundle.json') -Raw | ConvertFrom-Json
if ($info.Format -ne 1 -or !$info.Files -or !$info.Platforms) { throw 'Invalid offline bundle metadata.' }
$listed = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)

# These hashes detect damaged/incomplete extraction; they are not a signature.
# Validate every bundled file before loading helpers or changing system settings.
foreach ($file in $info.Files) {
    if (!$file.Path -or [IO.Path]::IsPathRooted($file.Path) -or $file.Path.Contains(':') -or $file.Sha256 -notmatch '^[0-9A-Fa-f]{64}$') {
        throw 'Invalid offline bundle file entry.'
    }
    $path = [IO.Path]::GetFullPath([IO.Path]::Combine($root, $file.Path))
    if (!$path.StartsWith($root, [StringComparison]::OrdinalIgnoreCase) -or !$listed.Add($path) -or
        !(Test-Path -LiteralPath $path -PathType Leaf) -or (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ine $file.Sha256) {
        throw "Offline bundle is incomplete or damaged: $($file.Path). Extract the entire ZIP again."
    }
}
foreach ($file in Get-ChildItem -LiteralPath $root -Recurse -File) {
    if ($file.FullName -ine (Join-Path $root 'bundle.json') -and !$listed.Contains($file.FullName)) {
        throw "Offline bundle contains an unlisted file: $($file.Name). Extract to a new folder."
    }
}
. (Join-Path $root 'DevBuildTools.ps1')
$hostPlatform = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
if ($Platform -eq 'Auto') {
    $Platform = $hostPlatform
    if ($Platform -notin $info.Platforms -and 'x86' -in $info.Platforms) { $Platform = 'x86' }
    elseif ($Platform -eq 'Arm64' -and $Platform -notin $info.Platforms -and
        [Environment]::OSVersion.Version.Build -ge 22000 -and 'x64' -in $info.Platforms) { $Platform = 'x64' }
}
if ($Platform -notin $info.Platforms) { throw "This ZIP does not include $Platform. Included: $($info.Platforms -join ', ')." }
if (!$CheckOnly -and $Platform -ine $hostPlatform -and $Platform -ne 'x86' -and
    !($Platform -eq 'x64' -and $hostPlatform -eq 'Arm64' -and [Environment]::OSVersion.Version.Build -ge 22000)) {
    throw "The $Platform application cannot run on $hostPlatform Windows."
}
$layout = Join-Path $root "App/$platform"
$dependencies = Join-Path $root "Dependencies/$platform"
[xml]$manifest = Get-Content -LiteralPath (Join-Path $layout 'AppxManifest.xml') -Raw
Assert-DevManifest $manifest $platform '19282JackieLiu.Notepads-Beta'
if ($manifest.Package.Identity.Version -ne $info.Version -or
    $manifest.Package.Applications.Application.VisualElements.DisplayName -ne 'Notepads') { throw 'Offline version or display name does not match Production.' }
$minimum = [version]$manifest.Package.Dependencies.TargetDeviceFamily.MinVersion
if ([Environment]::OSVersion.Version -lt $minimum) { throw "This package requires Windows $minimum or later." }
foreach ($dependency in $manifest.Package.Dependencies.PackageDependency) {
    # Require the ZIP to be self-contained even when this machine already has it.
    $null = Find-DevFrameworkPackage $dependency $dependencies $platform
}
Write-Host "Verified offline Notepads $($info.Version) / $platform / source $($info.Commit)."
if (Test-DevRegistrationEnabled) { Write-Host 'Windows Developer Mode: enabled.' }
else { Write-Host 'Windows Developer Mode: disabled; installation will request administrator approval to enable it.' }
if ($CheckOnly) { Write-Output 'PASS: Offline files, architecture and dependencies verified; no installation or launch performed.'; return }

$package = Register-DevLayout $layout $dependencies $platform '19282JackieLiu.Notepads-Beta'
Write-Host "Registered Notepads. Keep this folder: $root"
if (!$NoLaunch) {
    $appId = $package.PackageFamilyName + '!App'
    Start-Process -FilePath (Join-Path $env:SystemRoot 'explorer.exe') -ArgumentList (ConvertTo-DevProcessArgument "shell:AppsFolder\$appId") -WindowStyle Hidden
    Write-Host 'Launch requested. You can also open Notepads from the Start menu.'
}
