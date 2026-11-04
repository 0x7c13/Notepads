# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

param(
    [int] $TimeoutSeconds = 120,
    [ValidateSet('Debug', 'Release')] [string] $Configuration = 'Release',
    [ValidateSet('x64', 'ARM64')] [string] $Platform = 'x64',
    [switch] $Preview,
    [switch] $BidiPreview,
    [switch] $NoBuild,
    [ValidateSet('All', 'Search', 'NativeStorage', 'RegexMemory', 'Persistence', 'SessionResilience', 'Performance', 'Syntax', 'Diff')]
    [string] $TestGroup = 'All'
)
$ErrorActionPreference = 'Stop'
if ($Preview -or $BidiPreview) {
    . (Join-Path $PSScriptRoot 'TestBuildIdentity.ps1')
    $null = Assert-TestBuildIdentity App $Configuration $Platform
    $argument = if ($BidiPreview) { '--bidi-preview' } else { '--preview' }
    & (Join-Path $PSScriptRoot 'Test-IsolatedUwp.ps1') -Platform $Platform -Configuration $Configuration `
        -TestArguments @($argument) -TimeoutSeconds $TimeoutSeconds -WaitForUserExit
}
else {
    $group = if ($TestGroup -eq 'All') { @('Editor', 'Persistence') } else { @($TestGroup) }
    & (Join-Path $PSScriptRoot 'Test.ps1') -Suite Integration -Group $group -Platform $Platform `
        -Configuration $Configuration -NoBuild:$NoBuild -TimeoutSeconds $TimeoutSeconds
}
