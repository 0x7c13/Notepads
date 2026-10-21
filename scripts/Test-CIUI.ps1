# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

#requires -Version 7.4
param([ValidateSet('E2E', 'Faults')][string] $Lane = 'E2E')
$ErrorActionPreference = 'Stop'
if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted') { throw 'UI CI requires a disposable GitHub-hosted runner.' }
$configuration = if ($Lane -eq 'E2E') { 'Release' } else { 'Debug' }
$appGroups = @(& (Join-Path $PSScriptRoot 'Test.ps1') -Suite Integration -Configuration Release -List |
    Where-Object { $_.Profile -eq 'App' -and $_.Default })
if (@($appGroups | Where-Object { $_.Configuration -notin @('Release', 'Debug') }).Count) {
    throw 'A default App test group has no supported CI configuration.'
}
$groups = @($appGroups | Where-Object Configuration -eq $configuration | ForEach-Object Group)
if (!$groups.Count) { throw "No default App test groups were discovered for $configuration." }
if ($Lane -eq 'E2E') { $groups = @('UIAutomation', 'DragDrop', 'AppLifecycle') + $groups }
Write-Host ("$Lane groups ($configuration): " + ($groups -join ', '))
& (Join-Path $PSScriptRoot 'Test.ps1') -Suite Integration -Group $groups -Configuration $configuration -Platform x64
