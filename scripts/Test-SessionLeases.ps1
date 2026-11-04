# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Both processes run in one explicitly isolated data root; the prior package is restored.
param(
    [int] $TimeoutSeconds = 60,
    [ValidateSet('x64', 'ARM64')] [string] $Platform = 'ARM64',
    [ValidateSet('Debug', 'Release')] [string] $Configuration = 'Release',
    [string] $SourceLayout = '',
    [switch] $NoBuild
)
$ErrorActionPreference = 'Stop'
if ($SourceLayout) {
    & (Join-Path $PSScriptRoot 'Test-IsolatedUwp.ps1') -Platform $Platform -Configuration $Configuration `
        -LeaseProbe -SourceLayout $SourceLayout -TimeoutSeconds $TimeoutSeconds
}
else {
    & (Join-Path $PSScriptRoot 'Test.ps1') -Suite Integration -Group Leases -Platform $Platform `
        -Configuration $Configuration -NoBuild:$NoBuild -TimeoutSeconds $TimeoutSeconds
}
