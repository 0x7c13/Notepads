# Copyright (c) 2026, Jiaqi (0x7c13) Liu. See LICENSE.
# Both processes run in one explicitly isolated data root; the prior package is restored.
param(
    [int] $TimeoutSeconds = 60,
    [ValidateSet('x64', 'ARM64')] [string] $Platform = 'ARM64',
    [ValidateSet('Debug', 'Release')] [string] $Configuration = 'Release',
    [string] $SourceLayout = ''
)
$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'Test-IsolatedUwp.ps1') -Project Editor -Platform $Platform -Configuration $Configuration `
    -LeaseProbe -SourceLayout $SourceLayout -TimeoutSeconds $TimeoutSeconds
