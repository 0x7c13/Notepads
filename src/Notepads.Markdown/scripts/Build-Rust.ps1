# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE file in the project root for license information.
# ---------------------------------------------------------------------------------------------

[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('i686-pc-windows-msvc','x86_64-pc-windows-msvc','aarch64-pc-windows-msvc')][string] $Target,
    [Parameter(Mandatory)][string] $TargetDirectory,
    [Parameter(Mandatory)][string] $StagedLibrary,
    [Parameter(Mandatory)][string] $NativeOutput,
    [Parameter(Mandatory)][string] $TrackedHeader,
    [string] $VsInstallRoot,
    [ValidateSet('true','false')][string] $Diagnostics = 'false'
)

$ErrorActionPreference = 'Stop'
# MSBuild launched from PS7 can retain its module paths in this PS5 child.
# Load this engine's Utility module, including PS5's Get-FileHash function.
Import-Module (Join-Path $PSHOME 'Modules/Microsoft.PowerShell.Utility/Microsoft.PowerShell.Utility.psd1') -Force
$moduleRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
. (Join-Path $PSScriptRoot 'RustToolchain.ps1')
$rustPolicy = Read-MarkdownRustPolicy $moduleRoot
$channel = $rustPolicy.Channel
$cargoPath = Find-MarkdownCargo
$hostProbePreviousToolchain = $env:RUSTUP_TOOLCHAIN
try
{
    $env:RUSTUP_TOOLCHAIN = $channel
    $cargoVersion = & $cargoPath -vV
    if ($LASTEXITCODE -ne 0) { throw "Cannot inspect the host for Markdown Rust $channel." }
    $devShellArchitecture = Get-MarkdownRustDevShellArchitecture $cargoVersion
}
finally { $env:RUSTUP_TOOLCHAIN = $hostProbePreviousToolchain }

# MSBuild's UWP tasks prepend Store CRT paths to LIB. Initialize the desktop
# tools for Cargo's host builds independently of its explicit library target.
if ($VsInstallRoot) { $vsRoot = [IO.Path]::GetFullPath($VsInstallRoot) }
else
{
    $component = if ($devShellArchitecture -eq 'arm64') { 'ARM64' } else { 'x86.x64' }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $vsRoot = & $vswhere -latest -version '[18.0,19.0)' -products '*' `
        -requires "Microsoft.VisualStudio.Component.VC.Tools.$component" -property installationPath
}
if (!$vsRoot) { throw 'Visual Studio 2026 with the UWP C++ tools is required.' }
Import-Module (Join-Path $vsRoot 'Common7/Tools/Microsoft.VisualStudio.DevShell.dll')
Enter-VsDevShell -VsInstallPath $vsRoot -SkipAutomaticLocation `
    -DevCmdArguments "-arch=$devShellArchitecture -host_arch=$devShellArchitecture" | Out-Null
$msbuildBin = Join-Path $vsRoot 'MSBuild/Current/Bin'
# Windows PowerShell does not inherit MSBuild.exe.config's dependency redirects.
# Resolve only beside the selected MSBuild, and remove the handler after the check.
$resolveMSBuildAssembly = [ResolveEventHandler] {
    param($assemblySender, $assemblyRequest)
    $name = [Reflection.AssemblyName]::new($assemblyRequest.Name).Name
    $path = Join-Path $msbuildBin ($name + '.dll')
    if (Test-Path -LiteralPath $path -PathType Leaf) { return [Reflection.Assembly]::LoadFrom($path) }
    return $null
}
[AppDomain]::CurrentDomain.add_AssemblyResolve($resolveMSBuildAssembly)
try
{
    Add-Type -Path (Join-Path $msbuildBin 'Microsoft.Build.Utilities.Core.dll')
    foreach ($trackedPath in @($StagedLibrary, $TrackedHeader))
    {
        $fullTrackedPath = [IO.Path]::GetFullPath($trackedPath)
        if ([Microsoft.Build.Utilities.FileTracker]::FileIsExcludedFromDependencies($fullTrackedPath))
        {
            throw "MSBuild filters this dependency from tracking: $fullTrackedPath. Set NotepadsMarkdownBuildRoot outside AppData/Temp."
        }
    }
}
finally { [AppDomain]::CurrentDomain.remove_AssemblyResolve($resolveMSBuildAssembly) }

function Assert-PlainPath([string] $Path)
{
    $cursor = [IO.Path]::GetFullPath($Path)
    while ($cursor)
    {
        if (Test-Path -LiteralPath $cursor)
        {
            $entry = Get-Item -LiteralPath $cursor -Force
            if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)
            {
                throw "Native link input cannot traverse a reparse point: $cursor"
            }
        }
        $parent = [IO.Path]::GetDirectoryName($cursor)
        if ($parent -eq $cursor) { break }
        $cursor = $parent
    }
}

$previousTarget = $env:CARGO_TARGET_DIR
$previousToolchain = $env:RUSTUP_TOOLCHAIN
Push-Location $moduleRoot
try
{
    $env:CARGO_TARGET_DIR = [IO.Path]::GetFullPath($TargetDirectory)
    # The checked-in channel wins over inherited rustup environment and directory overrides.
    $env:RUSTUP_TOOLCHAIN = $channel
    $featureArguments = if ($Diagnostics -eq 'true') { @('--features', 'diagnostics') } else { @() }
    & $cargoPath rustc --locked --release -p markdown-ffi --lib --target $Target @featureArguments -- --print native-static-libs
    if ($LASTEXITCODE -ne 0) { throw "Cargo failed with exit code $LASTEXITCODE." }
    $source = Join-Path $env:CARGO_TARGET_DIR "$Target/release/markdown_ffi.lib"
    $staged = [IO.Path]::GetFullPath($StagedLibrary)
    Assert-PlainPath $source
    Assert-PlainPath $staged
    $hash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
    $bytes = (Get-Item -LiteralPath $source).Length
    $now = [DateTime]::UtcNow
    if ((Test-Path -LiteralPath $NativeOutput) -and (Get-Item -LiteralPath $NativeOutput).LastWriteTimeUtc -ge $now)
    {
        throw 'Native output timestamp is in the future; incremental linking cannot be trusted.'
    }
    $changed = !(Test-Path -LiteralPath $staged) -or (Get-FileHash -LiteralPath $staged -Algorithm SHA256).Hash -cne $hash
    if ($changed)
    {
        [IO.Directory]::CreateDirectory((Split-Path -Parent $staged)) | Out-Null
        Copy-Item -LiteralPath $source -Destination $staged -Force
        [IO.File]::SetLastWriteTimeUtc($staged, $now)
    }
    Assert-PlainPath $staged
    if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -cne $hash -or
        (Get-FileHash -LiteralPath $staged -Algorithm SHA256).Hash -cne $hash -or
        (Get-Item -LiteralPath $staged).Length -ne $bytes)
    {
        throw 'Rust native link input changed during staging.'
    }
    [pscustomobject]@{ Source=$source; Staged=$staged; SHA256=$hash; Bytes=$bytes; Changed=$changed } |
        ConvertTo-Json | Set-Content -LiteralPath ($staged + '.json') -Encoding utf8
}
finally
{
    Pop-Location
    $env:CARGO_TARGET_DIR = $previousTarget
    $env:RUSTUP_TOOLCHAIN = $previousToolchain
}
