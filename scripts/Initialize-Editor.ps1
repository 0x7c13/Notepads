# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE file in the project root for license information.
# ---------------------------------------------------------------------------------------------

param([string] $NuGetExe = 'nuget.exe')
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$editor = Join-Path $repo 'src\Notepads.Native'
$packageFile = Join-Path $editor 'WinUIEditor\packages.config'
if (!(Test-Path -LiteralPath $packageFile)) { throw 'Internal editor source is missing from this checkout.' }
[xml] $packages = Get-Content -LiteralPath $packageFile -Raw
foreach ($dependency in $packages.packages.package) {
    & $NuGetExe install $dependency.id -Version $dependency.version -OutputDirectory (Join-Path $repo 'src\packages') -NonInteractive -Source 'https://api.nuget.org/v3/index.json'
    if ($LASTEXITCODE) { throw "Could not restore $($dependency.id)." }
}
Write-Output 'Internal UWP editor dependencies are ready. Restore/build src/Notepads.sln next.'
