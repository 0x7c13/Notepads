# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Verification is read-only. Hosted CI explicitly opts into installing the checked-in policy.
param([switch] $InstallMissing, [string] $ModuleRoot = (Join-Path $PSScriptRoot '../src/Notepads.Markdown'))
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../src/Notepads.Markdown/scripts/RustToolchain.ps1')
$rustPolicy = Read-MarkdownRustPolicy $ModuleRoot
$policy = $rustPolicy.Text
function Read-RustPolicyValue([string] $Name) {
    $matches = [regex]::Matches($policy, '(?m)^\s*' + [regex]::Escape($Name) + '\s*=\s*"(?<value>[^"\r\n]+)"\s*$')
    if ($matches.Count -ne 1) { throw "Expected one Rust toolchain $Name setting." }
    return $matches[0].Groups['value'].Value
}
function Read-RustPolicyList([string] $Name) {
    $matches = [regex]::Matches($policy, '(?m)^\s*' + [regex]::Escape($Name) + '\s*=\s*\[(?<values>[^\]]*)\]\s*$')
    if ($matches.Count -ne 1) { throw "Expected one Rust toolchain $Name list." }
    $values = @([regex]::Matches($matches[0].Groups['values'].Value, '"(?<value>[a-zA-Z0-9_-]+)"') |
        ForEach-Object { $_.Groups['value'].Value })
    if (!$values.Count -or ($matches[0].Groups['values'].Value -replace '"[a-zA-Z0-9_-]+"|,|\s', '')) {
        throw "Invalid Rust toolchain $Name list."
    }
    return $values
}
$channel = $rustPolicy.Channel
$profile = Read-RustPolicyValue profile
$components = @(Read-RustPolicyList components)
$targets = @(Read-RustPolicyList targets)
if ($channel -notmatch '^\d+\.\d+\.\d+$' -or $profile -notin @('minimal', 'default', 'complete') -or
    @($targets | Where-Object { $_ -notmatch '^(aarch64|i686|x86_64)-pc-windows-msvc$' }).Count) {
    throw 'The Rust policy requires an exact stable version, supported profile and Windows MSVC targets.'
}
$rustup = Get-Command rustup.exe -ErrorAction SilentlyContinue
if (!$rustup) {
    $rustup = Join-Path $env:USERPROFILE '.cargo/bin/rustup.exe'
    if (!(Test-Path -LiteralPath $rustup -PathType Leaf)) { throw 'Install rustup before verifying the Markdown Rust toolchain.' }
}
if ($InstallMissing) {
    & $rustup toolchain install $channel --profile $profile --component ($components -join ',') --target ($targets -join ',')
    if ($LASTEXITCODE -ne 0) { throw "Rust toolchain installation failed: $LASTEXITCODE" }
}
$compiler = @(& $rustup run $channel rustc -vV)
if ($LASTEXITCODE -ne 0 -or @($compiler | Where-Object { $_ -ceq "release: $channel" }).Count -ne 1) {
    throw "The pinned Rust $channel compiler is unavailable. Run Initialize-RustToolchain.ps1 -InstallMissing."
}
$installedTargets = @(& $rustup target list --installed --toolchain $channel)
if ($LASTEXITCODE -ne 0 -or @($targets | Where-Object { $_ -notin $installedTargets }).Count) {
    throw "Pinned Rust $channel is missing required MSVC targets. Run Initialize-RustToolchain.ps1 -InstallMissing."
}
$installedComponents = @(& $rustup component list --installed --toolchain $channel)
if ($LASTEXITCODE -ne 0) { throw "Cannot verify pinned Rust $channel components." }
foreach ($component in $components) {
    if (!@($installedComponents | Where-Object { $_ -eq $component -or $_.StartsWith($component + '-', [StringComparison]::Ordinal) }).Count) {
        throw "Pinned Rust $channel is missing $component. Run Initialize-RustToolchain.ps1 -InstallMissing."
    }
}
Write-Output "Rust $channel with the checked-in components and MSVC targets is available."
