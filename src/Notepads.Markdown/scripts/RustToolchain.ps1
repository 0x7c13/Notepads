# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE file in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Shared policy/discovery only; callers decide whether to verify or install tools.
function Read-MarkdownRustPolicy([string] $ModuleRoot)
{
    $text = [IO.File]::ReadAllText((Join-Path $ModuleRoot 'rust-toolchain.toml'))
    $channels = [regex]::Matches($text, '(?m)^\s*channel\s*=\s*"(?<channel>\d+\.\d+\.\d+)"\s*$')
    if ($channels.Count -ne 1) { throw 'The Markdown Rust toolchain must specify one exact stable channel.' }
    return [pscustomobject]@{ Text = $text; Channel = $channels[0].Groups['channel'].Value }
}

# An x64 tools install also supplies the x86 desktop tools for an i686 Rust host.
function Get-MarkdownRustDevShellArchitecture([string[]] $CargoVersion)
{
    $cargoHost = @($CargoVersion | Where-Object { $_ -clike 'host: *' })
    if ($cargoHost.Count -ne 1) { throw 'Cargo must report exactly one host triple.' }
    switch -CaseSensitive ($cargoHost[0])
    {
        'host: aarch64-pc-windows-msvc' { 'arm64' }
        'host: x86_64-pc-windows-msvc' { 'x64' }
        'host: i686-pc-windows-msvc' { 'x64' }
        default { throw "Unsupported Markdown Rust $($cargoHost[0])." }
    }
}

function Find-MarkdownCargo
{
    $cargo = Get-Command cargo.exe -ErrorAction SilentlyContinue
    if ($cargo) { return $cargo.Source }
    $path = Join-Path $env:USERPROFILE '.cargo/bin/cargo.exe'
    if (!(Test-Path -LiteralPath $path -PathType Leaf))
    {
        throw 'Install rustup with the toolchain pinned in rust-toolchain.toml, or put cargo.exe on PATH.'
    }
    return $path
}
