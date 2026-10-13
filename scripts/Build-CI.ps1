# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

#requires -Version 7.4
param([Parameter(Mandatory)][ValidateSet('x86', 'x64', 'ARM64')][string] $Platform)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'TestBuildIdentity.ps1')
$repo = Split-Path $PSScriptRoot -Parent
$runRoot = Join-Path $repo ("artifacts/ci/$Platform/" + [Guid]::NewGuid().ToString('N'))
$packageRoot = Join-Path $runRoot 'packages'
$null = New-Item -ItemType Directory -Path $packageRoot -Force
$report = [ordered]@{ Platform = $Platform; Configuration = 'Production'; Status = 'RUNNING'; Error = $null; Packages = @() }
$priorPath = $env:PATH

function Invoke-BuildStep([string] $Name, [string] $Executable, [string[]] $Arguments) {
    $log = Join-Path $runRoot "$Name.log"
    Write-Host "$Name ($Platform): $log"
    & $Executable @Arguments *> $log
    if ($LASTEXITCODE -ne 0) {
        Get-Content -LiteralPath $log -Tail 60 | Write-Host
        throw "$Name failed with exit code $LASTEXITCODE. See $log"
    }
}

try {
    $before = Get-TestSourceHashes Product
    $before | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $runRoot 'sources.json')
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $installation = & $vswhere -latest -products '*' -version '[18.0,19.0)' -property installationPath
    if ($LASTEXITCODE -ne 0 -or !$installation) { throw 'Visual Studio 2026 with UWP/native tools is required.' }
    $hostDirectory = if ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64') { 'arm64/' } else { '' }
    $msbuild = Join-Path $installation "MSBuild/Current/Bin/${hostDirectory}MSBuild.exe"
    $env:PATH = (Split-Path $vswhere -Parent) + ';' + $priorPath
    $solutionDirectory = [IO.Path]::GetFullPath((Join-Path $repo 'src/'))
    $common = @('/nologo', '/v:minimal', '/p:Configuration=Production', "/p:Platform=$Platform", "/p:SolutionDir=$solutionDirectory")

    # Solution restore also restores the native packages.config into src/packages.
    Invoke-BuildStep Restore $msbuild (@((Join-Path $repo 'src/Notepads.sln'), '/t:Restore', '/p:RestorePackagesConfig=true') + $common)
    Invoke-BuildStep Publish $msbuild (@((Join-Path $repo 'src/Notepads/Notepads.csproj'), '/restore', '/t:Publish',
        '/p:GenerateAppxPackageOnBuild=true', '/p:PublishAppxPackage=false',
        '/p:IncludePublishItemsOutputGroup=true', '/p:UapAppxPackageBuildMode=StoreUpload',
        '/p:AppxBundle=Always', "/p:AppxBundlePlatforms=$Platform",
        '/p:AppxPackageSigningEnabled=false', '/p:AppxSymbolPackageEnabled=true',
        "/p:AppxPackageDir=$packageRoot\", "/bl:$(Join-Path $runRoot 'publish.binlog')") + $common)
    $null = Write-TestBuildIdentity Product Production $Platform $before
    $assembly = @(Get-TestArtifactFiles Product Production $Platform)
    Invoke-BuildStep BuildArchitectureTests dotnet @('build', (Join-Path $repo 'tests/IntegrationTests/IntegrationTests.csproj'),
        '-c', 'Release', '/p:IntegrationHost=Headless', '--nologo')
    Invoke-BuildStep Architecture dotnet @((Join-Path $repo 'tests/IntegrationTests/bin/Headless/Release/net10.0/NotepadsIntegrationTests.Headless.dll'),
        '--architecture', $assembly[0])

    $uploads = @(Get-ChildItem -LiteralPath $packageRoot -Recurse -File | Where-Object Extension -in '.msixupload', '.appxupload')
    if ($uploads.Count -ne 1) { throw "Expected exactly one fresh upload package; found $($uploads.Count)." }
    & (Join-Path $PSScriptRoot 'Test-AppPackage.ps1') -Path $uploads[0].FullName -ExpectedArchitectures @($Platform) |
        Tee-Object -FilePath (Join-Path $runRoot 'Package.log')
    $report.Packages = @($uploads | ForEach-Object {
        [ordered]@{ Name = $_.Name; Length = $_.Length; Sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }
    })
    $report.Status = 'PASS'
}
catch { $report.Status = 'FAIL'; $report.Error = $_.ToString(); throw }
finally {
    $env:PATH = $priorPath
    $report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $runRoot 'results.json')
    if ($env:GITHUB_STEP_SUMMARY) {
        "Native AOT / ${Platform}: $($report.Status). Unsigned Production package; no deployment or publication." |
            Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY
    }
    Write-Host "Build report: $runRoot"
}
