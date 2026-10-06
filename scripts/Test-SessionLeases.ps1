# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE file in the project root for license information.
# ---------------------------------------------------------------------------------------------

param([int] $TimeoutSeconds = 60)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$package = Get-AppxPackage Notepads.Editor.Tests
if (!$package) { throw 'Register the current editor test package first (Test-Editor.ps1).' }
$scope = [Guid]::NewGuid().ToString('N')
$state = Join-Path $env:LOCALAPPDATA "Packages\$($package.PackageFamilyName)\LocalState\LeaseTests\$scope"
Add-Type @'
using System;
using System.Runtime.InteropServices;
[ComImport, Guid("2e941141-7f97-4756-ba1d-9decde894a3d"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface ISessionLeaseActivationManager {
    [PreserveSig] int ActivateApplication([MarshalAs(UnmanagedType.LPWStr)] string appId, [MarshalAs(UnmanagedType.LPWStr)] string args, uint options, out uint processId);
    [PreserveSig] int ActivateForFile(IntPtr items, string verb, out uint processId);
    [PreserveSig] int ActivateForProtocol(IntPtr items, out uint processId);
}
public static class SessionLeaseActivation {
    public static uint Launch(string appId, string args) {
        var manager = (ISessionLeaseActivationManager)Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("45ba127d-10a8-46ea-8ab7-56ea9078943c")));
        uint processId;
        Marshal.ThrowExceptionForHR(manager.ActivateApplication(appId, args, 0, out processId));
        return processId;
    }
}
'@
$applicationId = "$($package.PackageFamilyName)!App"
$holderProcess = $null
$probeProcess = $null
$deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
function Wait-LeaseMarker([string] $Name) {
    while (!(Test-Path -LiteralPath (Join-Path $state $Name))) {
        $result = Join-Path $state 'result.txt'
        if (Test-Path -LiteralPath $result) { throw (Get-Content -LiteralPath $result -Raw) }
        if ([DateTime]::UtcNow -ge $deadline) { throw "Lease test timed out awaiting $Name." }
        Start-Sleep -Milliseconds 100
    }
}
function Stop-TestProcess($ProcessId) {
    if (!$ProcessId) { return }
    $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if (!$process) { return }
    $expected = Join-Path $package.InstallLocation 'NotepadsEditorTests.exe'
    if ($process.Path -ne $expected) { throw 'Refusing to terminate a process outside the editor test package.' }
    Stop-Process -Id $ProcessId -Force
}
try {
    $holderProcess = [SessionLeaseActivation]::Launch($applicationId, "--lease-holder=$scope")
    Wait-LeaseMarker 'holder-ready'
    $probeProcess = [SessionLeaseActivation]::Launch($applicationId, "--lease-probe=$scope")
    if ($holderProcess -eq $probeProcess) { throw 'The test package did not activate separate processes.' }
    Wait-LeaseMarker 'terminate-holder'
    Stop-TestProcess $holderProcess
    Wait-LeaseMarker 'result.txt'
    $result = Get-Content -LiteralPath (Join-Path $state 'result.txt') -Raw
    Write-Output $result
    if (!$result.StartsWith('PASS:')) { throw 'Packaged multi-process lease test failed.' }
}
finally {
    Stop-TestProcess $holderProcess
    Stop-TestProcess $probeProcess
}
