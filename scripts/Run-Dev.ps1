# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

#requires -Version 5.1
[CmdletBinding()]
param(
    [ValidateSet('Auto', 'x86', 'x64', 'ARM64')][string] $Platform = 'Auto',
    [ValidateSet('Release', 'Debug', 'Production')][string] $Configuration = 'Release',
    [switch] $CheckOnly,
    [switch] $NoInstall,
    [switch] $BuildOnly,
    [switch] $PassThru
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'DevBuildTools.ps1')
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if ([Environment]::OSVersion.Platform -ne 'Win32NT') { throw 'Run-Dev.ps1 requires Windows.' }
if ($Configuration -eq 'Production' -and !$BuildOnly) { throw 'Production requires -BuildOnly; Run-Dev only registers the separate development identity.' }
$packageName = if ($Configuration -eq 'Production') { '19282JackieLiu.Notepads-Beta' } else { 'Notepads-Dev' }
$hostPlatform = Get-DevHostPlatform
if ($Platform -eq 'Auto') { $Platform = $hostPlatform }
if ($Platform -eq 'ARM64' -and $hostPlatform -ne 'ARM64' -and !$BuildOnly -and !$CheckOnly) {
    throw 'An ARM64 application cannot run on an x64 host. Use -BuildOnly to cross-compile.'
}
$tools = Join-Path $repo '.tools/notepads-dev'
$lock = $null
$previousEnvironment = @{}
foreach ($name in @('PATH', 'DOTNET_ROOT', 'DOTNET_MSBUILD_SDK_RESOLVER_CLI_DIR', 'MSBuildSDKsPath')) {
    $previousEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
Push-Location -LiteralPath $repo
try {
    if (!$CheckOnly) {
        $null = New-Item -ItemType Directory -Path $tools -Force
        try { $lock = [IO.File]::Open((Join-Path $tools 'run.lock'), 'OpenOrCreate', 'ReadWrite', 'None') }
        catch { throw 'Another Run-Dev invocation owns this checkout. Wait for it to finish.' }
    }
    # Avoid an inherited Developer Shell pin overriding global.json, including
    # during discovery. All process environment changes are restored in finally.
    [Environment]::SetEnvironmentVariable('MSBuildSDKsPath', $null, 'Process')
    $dotnet = Find-DevDotnet $repo $tools
    if (!$dotnet -and !$CheckOnly -and !$NoInstall) { $dotnet = Install-DevDotnet $repo $tools $hostPlatform }
    $build = Find-DevMSBuild $repo $Platform $hostPlatform
    if (!$build -and !$CheckOnly -and !$NoInstall) {
        Install-DevBuildTools $tools $Platform
        $build = Find-DevMSBuild $repo $Platform $hostPlatform
    }
    $requiredSdk = (Get-Content -LiteralPath (Join-Path $repo 'global.json') -Raw | ConvertFrom-Json).sdk.version
    if (!$dotnet) { Write-Host "Missing .NET SDK compatible with global.json ($requiredSdk)." }
    else { Write-Host ".NET SDK $($dotnet.Version): $($dotnet.Path)" }
    if (!$build) { Write-Host 'Missing Build Tools 2026 with v145, UWP .NET/C++, and Windows SDK 10.0.26100.' }
    else { Write-Host "MSBuild $($build.Version): $($build.Path)" }
    if (Test-DevRegistrationEnabled) { Write-Host 'Windows Developer Mode: enabled.' }
    else { Write-Host 'Windows Developer Mode: disabled (required for launch; enabled with administrator approval before registration).' }
    if (!$dotnet -or !$build) { throw 'Toolchain is incomplete. Run without -CheckOnly/-NoInstall to install missing tools.' }
    if ($CheckOnly) { Write-Output "READY: $Configuration / $Platform (no installation, build or deployment performed)."; return }

    $dotnetRoot = Split-Path $dotnet.Path -Parent
    $env:PATH = $dotnetRoot + ';' + (Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer') + ';' + $env:PATH
    $env:DOTNET_ROOT = $dotnetRoot
    $env:DOTNET_MSBUILD_SDK_RESOLVER_CLI_DIR = $dotnetRoot
    $run = Join-Path $repo ('artifacts/dev/' + [Guid]::NewGuid().ToString('N'))
    $packages = Join-Path $run 'packages'
    $null = New-Item -ItemType Directory -Path $packages -Force
    $common = @('/nologo', '/v:minimal', "/p:Configuration=$Configuration", "/p:Platform=$Platform",
        "/p:SolutionDir=$([IO.Path]::GetFullPath((Join-Path $repo 'src/')))")
    if ($Configuration -eq 'Debug') { $common += '/p:PublishAot=false' }
    Write-Host "Restoring native and managed packages. Logs: $run"
    & $build.Path (Join-Path $repo 'src/Notepads.sln') /t:Restore /p:RestorePackagesConfig=true @common *> (Join-Path $run 'restore.log')
    if ($LASTEXITCODE -ne 0) { throw "Restore failed ($LASTEXITCODE). See $run/restore.log" }
    $target = if ($Configuration -eq 'Debug') { '/t:Build' } else { '/t:Publish' }
    Write-Host "Building $packageName ($Configuration / $Platform)..."
    & $build.Path (Join-Path $repo 'src/Notepads/Notepads.csproj') /restore $target @common `
        /p:GenerateAppxPackageOnBuild=true /p:PublishAppxPackage=false /p:AppxBundle=Never `
        /p:UapAppxPackageBuildMode=SideloadOnly /p:AppxPackageSigningEnabled=false /p:AppxSymbolPackageEnabled=false `
        "/p:AppxPackageDir=$packages\" *> (Join-Path $run 'build.log')
    if ($LASTEXITCODE -ne 0) { throw "Build failed ($LASTEXITCODE). See $run/build.log" }
    $app = @(Get-ChildItem -LiteralPath $packages -Recurse -File | Where-Object { $_.Extension -in @('.msix', '.appx') } |
        Where-Object { (Read-DevPackageManifest $_.FullName).Package.Identity.Name -eq $packageName })
    if ($app.Count -ne 1) { throw "Expected one fresh $packageName package; found $($app.Count). See $packages" }
    Assert-DevManifest (Read-DevPackageManifest $app[0].FullName) $Platform $packageName
    $layout = Join-Path $run 'app'
    Expand-DevArchive $app[0].FullName $layout
    Write-Host "Built development layout: $layout"
    $result = [pscustomobject]@{ Platform = $Platform; Configuration = $Configuration; Layout = $layout; Package = $app[0].FullName; PackageRoot = $packages; RunRoot = $run }
    if ($BuildOnly) { if ($PassThru) { $result }; return }
    $package = Register-DevLayout $layout $packages $Platform
    $appId = $package.PackageFamilyName + '!App'
    Start-Process -FilePath (Join-Path $env:SystemRoot 'explorer.exe') -ArgumentList (ConvertTo-DevProcessArgument "shell:AppsFolder\$appId") -WindowStyle Hidden
    Write-Output "Launched $appId. Keep $layout while this development build is registered."
    if ($PassThru) { $result }
}
finally {
    foreach ($name in $previousEnvironment.Keys) { [Environment]::SetEnvironmentVariable($name, $previousEnvironment[$name], 'Process') }
    if ($lock) { $lock.Dispose() }
    Pop-Location
}
