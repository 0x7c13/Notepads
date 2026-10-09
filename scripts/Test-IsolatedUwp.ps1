# Copyright (c) 2026, Jiaqi (0x7c13) Liu. See LICENSE.
# Preserves the prior loose test registration and every pre-existing LocalState file.
param(
    [ValidateSet('Editor', 'Workspace')] [string] $Project = 'Workspace',
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
$repo = Split-Path $PSScriptRoot -Parent
$runId = [Guid]::NewGuid().ToString('N')
$evidence = Join-Path $repo "artifacts\isolated-uwp\$runId"
$layout = [IO.Path]::GetFullPath((Join-Path $evidence 'layout'))
$prior = Get-AppxPackage -Name Notepads.Editor.Tests
$ownedProcesses = [Collections.Generic.List[object]]::new()
$ownedExecutable = $null
$registeredOwned = $false
$unregisteredPrior = $false
$localState = $null
$runRoot = $null
$before = @{}
$priorDirectories = @()

function Assert-PreservedFiles {
    foreach ($path in $priorDirectories) {
        if (!(Test-Path -LiteralPath $path -PathType Container)) { throw "A pre-existing test data directory disappeared: $path" }
    }
    foreach ($path in $before.Keys) {
        if (!(Test-Path -LiteralPath $path -PathType Leaf) -or
            (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $before[$path]) {
            throw "A pre-existing test data file changed: $path"
        }
    }
}

function Get-OwnedProcess([hashtable] $Identity) {
    $process = Get-Process -Id $Identity.Id -ErrorAction SilentlyContinue
    if ($process) {
        if (!$Identity.Verified -or $process.Path -ne $Identity.Path -or
            $process.StartTime.ToUniversalTime().Ticks -ne $Identity.StartTicks) {
            throw 'The activated process does not have the verified exact executable path and start time. It was left untouched.'
        }
    }
    return $process
}

function Start-OwnedProcess([string] $Argument) {
    $id = [PreservingUwpTestLauncher]::Launch("$($package.PackageFamilyName)!App", "--test-run $runId $Argument")
    # Track an unverified activation too: cleanup refuses to stop or unregister it
    # if its identity cannot be proven, rather than accidentally stopping a reused PID.
    $identity = @{ Id=$id; Path=$ownedExecutable; StartTicks=0L; Verified=$false }
    $ownedProcesses.Add($identity)
    $process = Get-Process -Id $id -ErrorAction Stop
    if ($process.Path -ne $ownedExecutable) { throw 'Activation returned an unowned executable.' }
    $identity.StartTicks = $process.StartTime.ToUniversalTime().Ticks
    $identity.Verified = $true
    return $identity
}

function Stop-OwnedProcess([hashtable] $Identity) {
    $process = Get-OwnedProcess $Identity
    if ($process) {
        Stop-Process -Id $process.Id
        if (!$process.WaitForExit(5000)) { throw 'The owned test process did not exit.' }
    }
    $null = $ownedProcesses.Remove($Identity)
}

function Wait-LeaseMarker([string] $State, [string] $Name, [DateTime] $Deadline) {
    $marker = Join-Path $State $Name
    while (!(Test-Path -LiteralPath $marker)) {
        $failure = Join-Path $State 'result.txt'
        if (Test-Path -LiteralPath $failure) { throw (Get-Content -LiteralPath $failure -Raw) }
        foreach ($identity in $ownedProcesses) {
            if (!(Get-OwnedProcess $identity)) { throw "Lease process $($identity.Id) exited before $Name." }
        }
        if ([DateTime]::UtcNow -ge $Deadline) { throw "Lease test timed out awaiting $Name; data: $State" }
        Start-Sleep -Milliseconds 100
    }
}

function Invoke-LeaseProbe {
    $scope = [Guid]::NewGuid().ToString('N')
    $state = Join-Path $runRoot "LeaseTests\$scope"
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    try {
        $holder = Start-OwnedProcess "--lease-holder=$scope"
        Wait-LeaseMarker $state 'holder-ready' $deadline
        if ([uint](Get-Content -LiteralPath (Join-Path $state 'holder-ready') -Raw) -ne $holder.Id) {
            throw 'The lease marker does not belong to the exact activated holder.'
        }
        $probe = Start-OwnedProcess "--lease-probe=$scope"
        if ($probe.Id -eq $holder.Id) { throw 'Lease activation did not create two separate processes.' }
        @($holder, $probe) | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidence 'lease-processes.json') -Encoding utf8
        Wait-LeaseMarker $state 'terminate-holder' $deadline
        Stop-OwnedProcess $holder
        Wait-LeaseMarker $state 'result.txt' $deadline
        $process = Get-OwnedProcess $probe
        if ($process -and !$process.WaitForExit(10000)) { Stop-OwnedProcess $probe }
        else { $null = $ownedProcesses.Remove($probe) }
        $result = Get-Content -LiteralPath (Join-Path $state 'result.txt') -Raw
        Copy-Item -LiteralPath (Join-Path $state 'result.txt') -Destination (Join-Path $evidence 'lease-results.txt')
        Write-Output $result
        if (!$result.StartsWith('PASS:', [StringComparison]::Ordinal) -or $result.Contains('FAIL:')) { throw 'Packaged multi-process lease test failed.' }
        Assert-PreservedFiles
    }
    finally {
        if (Test-Path -LiteralPath $state) {
            $protocolEvidence = Join-Path $evidence 'lease-protocol'
            New-Item -ItemType Directory -Path $protocolEvidence -Force | Out-Null
            Get-ChildItem -LiteralPath $state -File | Copy-Item -Destination $protocolEvidence
        }
    }
}

if ($LeaseProbe -and ($Project -ne 'Editor' -or $WaitForUserExit)) { throw 'LeaseProbe requires the Editor project and cannot run as an interactive preview.' }
if (Get-Process -Name NotepadsEditorTests, NotepadsWorkspaceTests -ErrorAction SilentlyContinue) {
    throw 'A test process is already running. Its process and registration were left untouched.'
}
if (!$ExpectedRegistration) {
    $candidate = Join-Path $repo 'artifacts\three-step-review\original-test-registration.json'
    if (Test-Path -LiteralPath $candidate) { $ExpectedRegistration = $candidate }
}
if ($ExpectedRegistration) {
    $expected = Get-Content -LiteralPath $ExpectedRegistration -Raw | ConvertFrom-Json
    if (!$prior -or $prior.PackageFullName -ne $expected.PackageFullName -or
        $prior.InstallLocation -ne $expected.InstallLocation -or $prior.Architecture.ToString() -ne 'X64') {
        throw 'The exact recorded x64 test registration is not present. Nothing was changed.'
    }
}
if ($prior -and (!$prior.IsDevelopmentMode -or !(Test-Path -LiteralPath (Join-Path $prior.InstallLocation 'AppxManifest.xml')))) {
    throw 'The prior package must be a restorable loose registration. Nothing was changed.'
}
New-Item -ItemType Directory -Path $layout -Force | Out-Null
if ($SourceLayout) {
    $source = [IO.Path]::GetFullPath($SourceLayout)
    Get-ChildItem -LiteralPath $source | Copy-Item -Destination $layout -Recurse
}
else {
    $projectName = if ($Project -eq 'Workspace') { 'UwpWorkspace.Tests' } else { 'UwpEditor.Tests' }
    $runtime = 'win-' + $Platform.ToLowerInvariant()
    $recipePath = Join-Path $repo "tests\$projectName\bin\$Platform\$Configuration\net10.0-windows10.0.26100.0\$runtime\$projectName.build.appxrecipe"
    if (!(Test-Path -LiteralPath $recipePath)) { throw "Publish the isolated test project first: $recipePath" }
    [xml] $recipe = Get-Content -LiteralPath $recipePath
    foreach ($group in $recipe.Project.ItemGroup) {
        foreach ($file in $group.ChildNodes) {
            if (!$file.PackagePath -or !$file.Include) { continue }
            $target = [IO.Path]::GetFullPath((Join-Path $layout $file.PackagePath))
            if (!$target.StartsWith($layout + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Recipe entry escapes owned layout.' }
            New-Item -ItemType Directory -Path (Split-Path $target) -Force | Out-Null
            Copy-Item -LiteralPath ([Uri]::UnescapeDataString($file.Include)) -Destination $target
        }
    }
}
$manifestPath = Join-Path $layout 'AppxManifest.xml'
$isolationMarker = Join-Path $layout 'TestIsolation.txt'
if (!(Test-Path -LiteralPath $isolationMarker) -or
    (Get-Content -LiteralPath $isolationMarker -First 1) -ne 'Notepads.TestStorage/1') {
    throw 'The staged package lacks the required isolated-storage contract. Rebuild the test harness before running it.'
}
[xml] $manifest = Get-Content -LiteralPath $manifestPath
if ($manifest.Package.Identity.Name -ne 'Notepads.Editor.Tests' -or $manifest.Package.Applications.Application.Extensions) {
    throw 'Only the test identity without activation extensions may be registered.'
}
$expectedExecutable = if ($Project -eq 'Workspace') { 'NotepadsWorkspaceTests.exe' } else { 'NotepadsEditorTests.exe' }
$expectedEntryPoint = if ($Project -eq 'Workspace') { 'Notepads.App' } else { 'NotepadsEditorTests.App' }
if ($manifest.Package.Applications.Application.Executable -ne $expectedExecutable -or
    $manifest.Package.Applications.Application.EntryPoint -ne $expectedEntryPoint -or
    $manifest.Package.Identity.Publisher -ne 'CN=40E66D07-5A3A-4954-9CA3-A1EB15ED0804') {
    throw 'The staged manifest does not name the expected isolated executable, entry point and test publisher.'
}
$ownedExecutable = [IO.Path]::GetFullPath((Join-Path $layout $manifest.Package.Applications.Application.Executable))
if (!$ownedExecutable.StartsWith($layout + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Executable escapes owned layout.' }
if (!(Test-Path -LiteralPath $ownedExecutable -PathType Leaf)) { throw 'The staged test executable is missing.' }
if ($prior) {
    $prior | Select-Object Name, PackageFullName, PackageFamilyName, InstallLocation, Architecture, IsDevelopmentMode |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidence 'prior-registration.json') -Encoding utf8
    $localState = Join-Path $env:LOCALAPPDATA "Packages\$($prior.PackageFamilyName)\LocalState"
    if (Test-Path -LiteralPath $localState) {
        $priorDirectories = @(Get-ChildItem -LiteralPath $localState -Directory -Recurse -Force | ForEach-Object FullName)
        foreach ($file in Get-ChildItem -LiteralPath $localState -File -Recurse -Force) {
            $before[$file.FullName] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
        }
    }
    $before | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $evidence 'prior-data-hashes.json') -Encoding utf8
}
if (!('PreservingUwpTestLauncher' -as [type])) {
    Add-Type @'
using System;
using System.Runtime.InteropServices;
[ComImport, Guid("2e941141-7f97-4756-ba1d-9decde894a3d"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IPreservingTestActivationManager {
    [PreserveSig] int ActivateApplication([MarshalAs(UnmanagedType.LPWStr)] string appId, [MarshalAs(UnmanagedType.LPWStr)] string arguments, uint options, out uint processId);
    [PreserveSig] int ActivateForFile(IntPtr items, string verb, out uint processId);
    [PreserveSig] int ActivateForProtocol(IntPtr items, out uint processId);
}
public static class PreservingUwpTestLauncher {
    public static uint Launch(string appId, string arguments) {
        var manager = (IPreservingTestActivationManager)Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("45ba127d-10a8-46ea-8ab7-56ea9078943c")));
        try {
            uint processId;
            Marshal.ThrowExceptionForHR(manager.ActivateApplication(appId, arguments, 0, out processId));
            return processId;
        } finally { Marshal.ReleaseComObject(manager); }
    }
}
'@
}
try {
    $current = Get-AppxPackage -Name Notepads.Editor.Tests
    if (($prior -and (!$current -or $current.PackageFullName -ne $prior.PackageFullName -or $current.InstallLocation -ne $prior.InstallLocation)) -or
        (!$prior -and $current)) { throw 'The test registration changed while staging; nothing was unregistered.' }
    if (Get-Process -Name NotepadsEditorTests, NotepadsWorkspaceTests -ErrorAction SilentlyContinue) {
        throw 'A test process started while staging. Its process and registration were left untouched.'
    }
    if ($prior) {
        Remove-AppxPackage -Package $prior.PackageFullName -PreserveApplicationData
        $unregisteredPrior = $true
    }
    Add-AppxPackage -Register $manifestPath
    $package = Get-AppxPackage -Name Notepads.Editor.Tests
    if (!$package -or [IO.Path]::GetFullPath($package.InstallLocation) -ne $layout) { throw 'Registration does not match owned layout.' }
    $registeredOwned = $true
    $localState = Join-Path $env:LOCALAPPDATA "Packages\$($package.PackageFamilyName)\LocalState"
    $runRoot = [IO.Path]::GetFullPath((Join-Path $localState "WorkspaceTests\$runId"))
    New-Item -ItemType Directory -Path $runRoot -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $runRoot 'runner-sentinel.txt'), $runId)
    Assert-PreservedFiles
    if ($LeaseProbe) {
        Invoke-LeaseProbe
    }
    else {
        $phase = 0
        foreach ($argument in $TestArguments) {
            $phase++
            $resultPath = Join-Path $runRoot 'results.txt'
            if (Test-Path -LiteralPath $resultPath) { Remove-Item -LiteralPath $resultPath }
            $identity = Start-OwnedProcess $argument
            $process = Get-OwnedProcess $identity
            $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
            while (!(Test-Path -LiteralPath $resultPath)) {
                if (!(Get-OwnedProcess $identity)) { throw "Test exited without a result; evidence: $runRoot" }
                if ([DateTime]::UtcNow -gt $deadline) { throw "Test timed out; evidence: $runRoot" }
                Start-Sleep -Milliseconds 200
            }
            $result = Get-Content -LiteralPath $resultPath -Raw
            Copy-Item -LiteralPath $resultPath -Destination (Join-Path $evidence "phase-$phase-results.txt")
            Write-Output $result
            if ($WaitForUserExit -and $result -notmatch 'FAIL:') {
                Write-Output 'Preview is ready. Close the test window to finish and restore the prior registration.'
                $process.WaitForExit()
            }
            if (!$process.WaitForExit(10000)) { Stop-OwnedProcess $identity } else { $null = $ownedProcesses.Remove($identity) }
            Assert-PreservedFiles
            if ($result -notmatch 'PASS:' -or $result -match 'FAIL:') { throw "Isolated phase $phase failed." }
        }
        }
}
finally {
    foreach ($identity in $ownedProcesses.ToArray()) { Stop-OwnedProcess $identity }
    $current = Get-AppxPackage -Name Notepads.Editor.Tests
    if ($current -and [IO.Path]::GetFullPath($current.InstallLocation) -eq $layout) {
        Remove-AppxPackage -Package $current.PackageFullName -PreserveApplicationData
    }
    elseif ($registeredOwned -and $current) { throw 'An unexpected registration appeared; it was left untouched.' }
    if ($unregisteredPrior) {
        if (Get-AppxPackage -Name Notepads.Editor.Tests) { throw 'Cannot restore the prior registration over an unexpected package.' }
        Add-AppxPackage -Register (Join-Path $prior.InstallLocation 'AppxManifest.xml')
        $restored = Get-AppxPackage -Name Notepads.Editor.Tests
        if (!$restored -or $restored.PackageFullName -ne $prior.PackageFullName -or
            $restored.InstallLocation -ne $prior.InstallLocation -or $restored.Architecture -ne $prior.Architecture) {
            throw 'The exact prior test registration was not restored.'
        }
    }
    Assert-PreservedFiles
    if ($runRoot -and (Get-Content -LiteralPath (Join-Path $runRoot 'runner-sentinel.txt') -Raw) -ne $runId) {
        throw 'The owned sentinel did not survive registration restoration.'
    }
    "Run=$runId`nData=$runRoot`nPriorFilesVerified=$($before.Count)" |
        Set-Content -LiteralPath (Join-Path $evidence 'preservation.txt') -Encoding utf8
    Write-Output "Isolated evidence: $evidence; data: $runRoot"
}
