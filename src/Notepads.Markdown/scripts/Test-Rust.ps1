# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE file in the project root for license information.
# ---------------------------------------------------------------------------------------------

[CmdletBinding()]
param(
    [ValidateSet('All', 'Core', 'Interop', 'Svg', 'Math', 'Diagrams')]
    [string] $Layer = 'All',
    [ValidateSet('i686-pc-windows-msvc', 'x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc')]
    [string] $Target = 'x86_64-pc-windows-msvc',
    [string] $TargetDirectory
)

$ErrorActionPreference = 'Stop'
$moduleRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'RustToolchain.ps1')
$rustPolicy = Read-MarkdownRustPolicy $moduleRoot
$channel = $rustPolicy.Channel
$cargoPath = Find-MarkdownCargo
$previousTarget = $env:CARGO_TARGET_DIR
$previousToolchain = $env:RUSTUP_TOOLCHAIN
Push-Location $moduleRoot
try
{
    # Keep compiler selection consistent with the verified checked-in policy.
    $env:RUSTUP_TOOLCHAIN = $channel
    if ($Layer -in @('Core', 'All'))
    {
        & (Join-Path $PSScriptRoot 'Test-FixedAssets.ps1') -Check All
    }
    if (!$TargetDirectory) { $TargetDirectory = $previousTarget }
    if (!$TargetDirectory)
    {
        $sha = [Security.Cryptography.SHA256]::Create()
        try
        {
            $fingerprint = ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($moduleRoot)))).Replace('-', '').Substring(0, 16)
        }
        finally { $sha.Dispose() }
        $TargetDirectory = Join-Path $env:USERPROFILE ".notepads-build/MarkdownTests/$fingerprint"
    }
    $env:CARGO_TARGET_DIR = [IO.Path]::GetFullPath($TargetDirectory)
    $common = @('test', '--locked', '--release', '--target', $Target)
    switch ($Layer)
    {
        'Core' { $arguments = @('-p', 'markdown-core') }
        'Interop' { $arguments = @('-p', 'markdown-ffi', '--lib', '--features', 'diagnostics') }
        'Svg' { $arguments = @('-p', 'markdown-svg', '-p', 'markdown-svg-budget', '--lib') }
        'Math' { $arguments = @('--manifest-path', 'crates/markdown-math/Cargo.toml', '--lib') }
        'Diagrams' { $arguments = @('-p', 'markdown-mermaid', '-p', 'markdown-diagram-platform', '--lib') }
        'All' { $arguments = @('--workspace', '--no-fail-fast', '--features', 'markdown-ffi/diagnostics') }
    }
    & $cargoPath @common @arguments
    $layerExitCode = $LASTEXITCODE
    if ($Layer -eq 'All')
    {
        & $cargoPath @common --manifest-path crates/markdown-math/Cargo.toml --lib
        if ($LASTEXITCODE -ne 0) { throw "Markdown math tests failed with exit code $LASTEXITCODE; workspace exit code: $layerExitCode." }
    }
    if ($layerExitCode -ne 0) { throw "Markdown $Layer tests failed with exit code $layerExitCode." }
}
finally
{
    $env:CARGO_TARGET_DIR = $previousTarget
    $env:RUSTUP_TOOLCHAIN = $previousToolchain
    Pop-Location
}
