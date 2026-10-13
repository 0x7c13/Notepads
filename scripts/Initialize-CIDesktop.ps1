# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Configures only an ephemeral hosted runner; local desktops use their own setup.
#requires -Version 7.4
param([ValidateSet('x64', 'ARM64')][string] $Platform = 'x64')
$ErrorActionPreference = 'Stop'
if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted') { throw 'Desktop CI setup requires a disposable GitHub-hosted runner.' }
$repo = Split-Path $PSScriptRoot -Parent
$evidence = Join-Path $repo 'artifacts/ci-desktop'
$null = New-Item -ItemType Directory -Path $evidence -Force
Add-Type -AssemblyName System.Windows.Forms
$session = (Get-Process -Id $PID).SessionId
[ordered]@{
    OS = [Environment]::OSVersion.VersionString
    Architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    Interactive = [Environment]::UserInteractive
    SessionId = $session
    Elevated = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    Screens = @([Windows.Forms.Screen]::AllScreens | ForEach-Object { $_.Bounds.ToString() })
} | ConvertTo-Json | Tee-Object -FilePath (Join-Path $evidence 'environment.json')
if (![Environment]::UserInteractive -or $session -eq 0) { throw 'UI tests require an interactive desktop session.' }

# This disposable desktop belongs to the input tests. Let their driver switch
# between its two verified windows without the user's foreground-lock delay.
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class CiDesktopSettings {
    [DllImport("user32.dll", EntryPoint = "SystemParametersInfoW", SetLastError = true)]
    public static extern bool Get(uint action, uint parameter, out uint value, uint flags);
    [DllImport("user32.dll", EntryPoint = "SystemParametersInfoW", SetLastError = true)]
    public static extern bool Set(uint action, uint parameter, IntPtr value, uint flags);
}
'@
$before = 0u
$after = 0u
if (![CiDesktopSettings]::Get(0x2000, 0, [ref]$before, 0) -or
    ![CiDesktopSettings]::Set(0x2001, 0, [IntPtr]::Zero, 0) -or
    ![CiDesktopSettings]::Get(0x2000, 0, [ref]$after, 0) -or $after -ne 0) {
    throw 'Cannot configure the hosted desktop foreground-lock timeout for input tests.'
}
@{ ForegroundLockMillisecondsBefore = $before; ForegroundLockMillisecondsAfter = $after } |
    ConvertTo-Json | Tee-Object -FilePath (Join-Path $evidence 'foreground.json')

$policy = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock'
if (!(Test-Path -LiteralPath $policy)) { $null = New-Item -Path $policy -Force }
$null = New-ItemProperty -Path $policy -Name AllowDevelopmentWithoutDevLicense -Value 1 -PropertyType DWord -Force
[xml] $versions = Get-Content -LiteralPath (Join-Path $repo 'Directory.Packages.props')
$xamlVersion = @($versions.Project.ItemGroup.PackageVersion | Where-Object Include -eq 'Microsoft.UI.Xaml').Version
if (!$xamlVersion) { throw 'The central Microsoft.UI.Xaml package version is missing.' }
$packages = if ($env:NUGET_PACKAGES) { $env:NUGET_PACKAGES } else { Join-Path $env:USERPROFILE '.nuget/packages' }
$xaml = Join-Path $packages "microsoft.ui.xaml/$xamlVersion/tools/AppX/$Platform/Release/Microsoft.UI.Xaml.2.8.appx"
$sdk = Join-Path ${env:ProgramFiles(x86)} 'Microsoft SDKs/Windows Kits/10/ExtensionSDKs/Microsoft.VCLibs/14.0/Appx'
foreach ($package in @(
    (Join-Path $sdk "Retail/$Platform/Microsoft.VCLibs.$Platform.14.00.appx"),
    (Join-Path $sdk "Debug/$Platform/Microsoft.VCLibs.$Platform.Debug.14.00.appx"), $xaml)) {
    if (!(Test-Path -LiteralPath $package -PathType Leaf)) { throw "Missing test framework: $package" }
    $archive = [IO.Compression.ZipFile]::OpenRead($package)
    try {
        $reader = [IO.StreamReader]::new($archive.GetEntry('AppxManifest.xml').Open())
        try { [xml] $manifest = $reader.ReadToEnd() } finally { $reader.Dispose() }
    }
    finally { $archive.Dispose() }
    $identity = $manifest.Package.Identity
    $installed = @(Get-AppxPackage -Name $identity.Name | Where-Object {
        $_.Architecture -ieq $Platform -and $_.Publisher -eq $identity.Publisher -and
        [version]$_.Version -ge [version]$identity.Version
    })
    if (!$installed.Count) { Add-AppxPackage -Path $package }
}
Write-Output 'UI test desktop and framework packages are ready.'
