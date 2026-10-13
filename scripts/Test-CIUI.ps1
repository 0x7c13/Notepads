# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

#requires -Version 7.4
$ErrorActionPreference = 'Stop'
if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted') { throw 'UI CI requires a disposable GitHub-hosted runner.' }
$appGroups = @(& (Join-Path $PSScriptRoot 'Test.ps1') -Suite Integration -List |
    Where-Object { $_.Profile -eq 'App' -and $_.Default } | ForEach-Object Group)
if (!$appGroups.Count) { throw 'No default App test groups were discovered.' }
$groups = @('UIAutomation', 'DragDrop') + $appGroups
Write-Host ('UI groups: ' + ($groups -join ', '))
& (Join-Path $PSScriptRoot 'Test.ps1') -Suite Integration -Group $groups -Configuration Release -Platform x64
