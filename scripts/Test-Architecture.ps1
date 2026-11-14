# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

param(
    [ValidateSet('Debug', 'Release')] [string] $Configuration = 'Debug',
    [ValidateSet('x64', 'ARM64')] [string] $Platform = 'ARM64',
    [switch] $NoBuild
)
$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'Test.ps1') -Suite Integration -Group ArchitectureSelfTest, Architecture `
    -Configuration $Configuration -Platform $Platform -NoBuild:$NoBuild
