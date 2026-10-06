# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE file in the project root for license information.
# ---------------------------------------------------------------------------------------------

param([ValidateSet('Debug', 'Release')] [string] $Configuration = 'Debug')
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$project = Join-Path $repo 'tests\Architecture.Tests\Architecture.Tests.csproj'
$assembly = Join-Path $repo "src\Notepads\obj\x64\$Configuration\net10.0-windows10.0.26100.0\win-x64\Notepads.dll"
if (!(Test-Path -LiteralPath $assembly)) {
    throw "Build the x64 $Configuration app first; the checker needs the managed assembly before Native AOT compilation."
}
& dotnet run --project $project -- --self-test
if ($LASTEXITCODE -ne 0) { throw 'The architecture checker failed its dependency probes.' }
& dotnet run --project $project -- $assembly
if ($LASTEXITCODE -ne 0) { throw 'Notepads violates the declared dependency layers.' }
