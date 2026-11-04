# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

param(
    [ValidateSet('x64', 'ARM64')] [string] $Platform = 'ARM64',
    [ValidateSet('Debug', 'Release')] [string] $Configuration = 'Release',
    [string[]] $TestArguments = @('--workspace-seed', '--workspace', '--workspace-window-close', '--workspace-restart'),
    [int] $TimeoutSeconds = 180,
    [switch] $WaitForUserExit,
    [switch] $LeaseProbe,
    [string] $SourceLayout = '',
    [string] $ExpectedRegistration = ''
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'IsolatedTestPackage.ps1')

if ($LeaseProbe -and $WaitForUserExit) { throw 'LeaseProbe cannot run as an interactive preview.' }
if (!$LeaseProbe -and $TestArguments.Count -eq 0) { throw 'No integration phases were selected.' }
$session = $null
try {
    $session = Start-IsolatedTestPackage -Platform $Platform -Configuration $Configuration `
        -SourceLayout $SourceLayout -ExpectedRegistration $ExpectedRegistration
    if ($LeaseProbe) { Invoke-LeaseProbe $session -TimeoutSeconds $TimeoutSeconds }
    else {
        foreach ($argument in $TestArguments) {
            if (!$argument) { throw 'An empty integration phase is not a valid selection.' }
            Invoke-IsolatedTestPhase $session $argument -TimeoutSeconds $TimeoutSeconds -WaitForUserExit:$WaitForUserExit
        }
    }
}
finally { if ($session) { Complete-IsolatedTestPackage $session } }
