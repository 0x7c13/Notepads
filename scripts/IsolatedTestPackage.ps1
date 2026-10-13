# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# One registration/desktop owner shared by ordinary, UIA and drag integration runs.
if (!('IsolatedTestProcess' -as [type])) { Add-Type -Path (Join-Path $PSScriptRoot 'IsolatedTestProcess.cs') }

function Assert-IsolatedTestPreservation {
    param([hashtable] $Session)
    foreach ($path in $Session.PriorDirectories) {
        if (!(Test-Path -LiteralPath $path -PathType Container)) { throw "Pre-existing test directory disappeared: $path" }
    }
    foreach ($path in $Session.Before.Keys) {
        if (!(Test-Path -LiteralPath $path -PathType Leaf) -or
            (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $Session.Before[$path]) {
            throw "Pre-existing test data changed: $path"
        }
    }
}

function Assert-IsolatedTestRunOwnership {
    param([hashtable] $Session)
    $registered = Get-AppxPackage -Name Notepads.Editor.Tests
    if (!$registered -or $registered.PackageFullName -ne $Session.Package.PackageFullName -or
        [IO.Path]::GetFullPath($registered.InstallLocation) -ne $Session.Layout) {
        throw 'The isolated test registration changed. No further process may be launched.'
    }
    if ((Get-Content -LiteralPath (Join-Path $Session.RunRoot 'runner-sentinel.txt') -Raw) -ne $Session.RunId) {
        throw 'The owned test sentinel changed.'
    }
}

function Get-IsolatedTestProcess {
    param([hashtable] $Identity)
    if ($Identity.Verified -and $Identity.Process -and $Identity.Process.HasExited) { return $null }
    $process = Get-Process -Id $Identity.Id -ErrorAction SilentlyContinue
    if ($process) {
        try {
            if (!$Identity.Verified -or !$Identity.Process -or $process.Path -ne $Identity.Path -or
                $process.StartTime.ToUniversalTime().Ticks -ne $Identity.StartTicks) {
                throw 'The activated PID no longer has the verified executable and creation time; it was left untouched.'
            }
            return $Identity.Process
        }
        finally { $process.Dispose() }
    }
    return $null
}

function Assert-IsolatedTestExit {
    param([hashtable] $Identity)
    if (!$Identity.Verified -or !$Identity.Process -or !$Identity.Process.HasExited) {
        throw 'The verified test process has not exited.'
    }
    $Identity.ExitCode = $Identity.Process.get_ExitCode()
    if (!$Identity.ExpectedTermination -and $Identity.ExitCode -ne 0) {
        throw "The test process exited with code $($Identity.ExitCode); a published PASS cannot override a process failure."
    }
}

function Close-IsolatedTestProcess {
    param([hashtable] $Session, [hashtable] $Identity)
    if ($Identity.Process) { $Identity.Process.Dispose(); $Identity.Process = $null }
    $null = $Session.Processes.Remove($Identity)
}

function Start-IsolatedTestProcess {
    param([hashtable] $Session, [string] $Argument, [switch] $ExpectedTermination)
    Assert-IsolatedTestRunOwnership $Session
    $id = [PreservingUwpTestLauncher]::Launch("$($Session.Package.PackageFamilyName)!App", "--test-run $($Session.RunId) $Argument")
    $identity = @{ Id = $id; Path = $Session.Executable; StartTicks = 0L; Verified = $false; ExpectedTermination = [bool]$ExpectedTermination; Process = $null }
    $Session.Processes.Add($identity)
    $process = [IsolatedTestProcess]::new($id)
    $identity.Process = $process
    # The wrapper retains an exact handle with limited rights. Metadata, waiting,
    # exit-code reads and termination all refer to that same process instance.
    if ($process.Path -ne $Session.Executable) { throw 'Activation returned an unowned executable.' }
    $identity.StartTicks = $process.StartTime.ToUniversalTime().Ticks
    $identity.Verified = $true
    return $identity
}

function Stop-IsolatedTestProcess {
    param([hashtable] $Session, [hashtable] $Identity, [switch] $CleanupAfterFailure)
    $process = Get-IsolatedTestProcess $Identity
    if ($process) {
        if (!$Identity.ExpectedTermination -and !$CleanupAfterFailure) {
            throw 'An ordinary test did not exit cleanly; forced termination cannot count as PASS.'
        }
        $process.Kill()
        if (!$process.WaitForExit(5000)) { throw 'The exact owned test process did not exit.' }
    }
    Close-IsolatedTestProcess $Session $Identity
}

function Invoke-IsolatedTestPhase {
    param([hashtable] $Session, [string] $Argument, [int] $TimeoutSeconds = 180, [switch] $WaitForUserExit)
    $Session.Phase++
    $resultPath = Join-Path $Session.RunRoot 'results.txt'
    $reportPath = Join-Path $Session.RunRoot 'report.json'
    foreach ($path in @($resultPath, $reportPath)) { if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path } }
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $identity = $null
    try {
        $identity = Start-IsolatedTestProcess $Session $Argument
        $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
        while (!(Test-Path -LiteralPath $resultPath)) {
            if (!(Get-IsolatedTestProcess $identity)) {
                # Atomic publication can complete after the first file check
                # but before process exit is observed. Validate the published
                # result, report and retained exit code through the normal path.
                if (Test-Path -LiteralPath $resultPath) { break }
                throw "Test exited without a result: $Argument; $($Session.RunRoot)"
            }
            if ([DateTime]::UtcNow -ge $deadline) { throw "Test phase timed out: $Argument; $($Session.RunRoot)" }
            Start-Sleep -Milliseconds 100
        }
        $process = $identity.Process
        if ($WaitForUserExit -and $process) {
            Write-Output 'Preview is ready. Close its window to finish and restore the previous test registration.'
            while (!$process.WaitForExit(1000)) { $null = Get-IsolatedTestProcess $identity }
        }
        elseif ($process -and !$process.WaitForExit(10000)) {
            throw "Test published a result but failed to exit cleanly: $Argument"
        }
        Assert-IsolatedTestExit $identity
        Close-IsolatedTestProcess $Session $identity
        $result = Get-Content -LiteralPath $resultPath -Raw
        if (!(Test-Path -LiteralPath $reportPath)) { throw "Test phase omitted its structured completion report: $Argument" }
        $report = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
        if ($report.Status -ne 'PASS' -or $report.Phase -ne $Argument -or $result -match 'FAIL:' -or $result -notmatch '(?m)^PASS(?::| )') {
            throw "Isolated phase failed: $Argument`n$result"
        }
        # Hash prior data once when taking the baseline and once after restoration.
        # Rehashing a large retained test history for every phase adds seconds per
        # process without strengthening the final preservation contract.
        Assert-IsolatedTestRunOwnership $Session
        Write-Output $result
        Write-Output ("PASS phase {0}: {1:N0} ms" -f $Argument, $watch.Elapsed.TotalMilliseconds)
    }
    finally {
        # Preserve this phase before cleanup starts another process and replaces
        # the shared protocol files, including PASS followed by a native crash.
        foreach ($name in @('results.txt', 'report.json')) {
            $source = Join-Path $Session.RunRoot $name
            if (Test-Path -LiteralPath $source) {
                Copy-Item -LiteralPath $source -Destination (Join-Path $Session.Root "phase-$($Session.Phase)-$name")
            }
        }
        if ($identity -and $identity.Process -and $identity.Process.HasExited) { $identity.ExitCode = $identity.Process.get_ExitCode() }
        if ($identity -and $identity.ContainsKey('ExitCode')) {
            @{ ProcessId = $identity.Id; ExitCode = $identity.ExitCode } |
                ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Session.Root "phase-$($Session.Phase)-process.json") -Encoding utf8
        }
    }
}

function Wait-LeaseMarker([hashtable] $Session, [string] $State, [string] $Name, [DateTime] $Deadline) {
    $marker = Join-Path $State $Name
    while (!(Test-Path -LiteralPath $marker)) {
        $failure = Join-Path $State 'result.txt'
        if ($Name -cne 'result.txt' -and (Test-Path -LiteralPath $failure)) { throw (Get-Content -LiteralPath $failure -Raw) }
        foreach ($identity in $Session.Processes) {
            if (!(Get-IsolatedTestProcess $identity)) {
                # Only the final result may accompany process exit. Readiness
                # markers still require the participant to remain alive.
                if ($Name -ceq 'result.txt' -and (Test-Path -LiteralPath $marker)) { return }
                throw "Lease process $($identity.Id) exited before $Name."
            }
        }
        if ([DateTime]::UtcNow -ge $Deadline) { throw "Lease test timed out awaiting $Name; data: $State" }
        Start-Sleep -Milliseconds 100
    }
}

function Invoke-LeaseProbe([hashtable] $Session, [int] $TimeoutSeconds = 60) {
    $scope = [Guid]::NewGuid().ToString('N')
    $state = Join-Path $Session.RunRoot "LeaseTests\$scope"
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    try {
        $holder = Start-IsolatedTestProcess $Session "--lease-holder=$scope" -ExpectedTermination
        Wait-LeaseMarker $Session $state 'holder-ready' $deadline
        if ([uint](Get-Content -LiteralPath (Join-Path $state 'holder-ready') -Raw) -ne $holder.Id) {
            throw 'The lease marker does not belong to the exact activated holder.'
        }
        $probe = Start-IsolatedTestProcess $Session "--lease-probe=$scope"
        if ($probe.Id -eq $holder.Id) { throw 'Lease activation did not create two separate processes.' }
        @($holder, $probe) | ForEach-Object {
            @{ Id = $_.Id; Path = $_.Path; StartTicks = $_.StartTicks; Verified = $_.Verified; ExpectedTermination = $_.ExpectedTermination }
        } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Session.Root 'lease-processes.json') -Encoding utf8
        Wait-LeaseMarker $Session $state 'terminate-holder' $deadline
        Stop-IsolatedTestProcess $Session $holder
        Wait-LeaseMarker $Session $state 'result.txt' $deadline
        $process = $probe.Process
        if ($process -and !$process.WaitForExit(10000)) { throw 'The successful lease probe did not exit cleanly.' }
        Assert-IsolatedTestExit $probe
        Close-IsolatedTestProcess $Session $probe
        $result = Get-Content -LiteralPath (Join-Path $state 'result.txt') -Raw
        Copy-Item -LiteralPath (Join-Path $state 'result.txt') -Destination (Join-Path $Session.Root 'lease-results.txt')
        Write-Output $result
        if (!$result.StartsWith('PASS:', [StringComparison]::Ordinal) -or $result.Contains('FAIL:')) { throw 'Packaged multi-process lease test failed.' }
        Assert-IsolatedTestRunOwnership $Session
    }
    finally {
        if (Test-Path -LiteralPath $state) {
            $evidence = Join-Path $Session.Root 'lease-protocol'
            New-Item -ItemType Directory -Path $evidence -Force | Out-Null
            Get-ChildItem -LiteralPath $state -File | Copy-Item -Destination $evidence
        }
    }
}

function Start-IsolatedTestPackage {
    param(
        [ValidateSet('x64', 'ARM64')] [string] $Platform = 'ARM64',
        [ValidateSet('Debug', 'Release')] [string] $Configuration = 'Release',
        [string] $SourceLayout = '', [string] $ExpectedRegistration = ''
    )
    $repo = Split-Path $PSScriptRoot -Parent
    $runId = [Guid]::NewGuid().ToString('N')
    $root = [IO.Path]::GetFullPath((Join-Path $repo "artifacts\isolated-uwp\$runId"))
    $layout = Join-Path $root 'layout'
    $family = 'Notepads.Editor.Tests_echhpq9pdbte8'
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $mutex = [Threading.Mutex]::new($false, "Global\Notepads.IntegrationTests.$sid")
    $acquired = $false
    try { $acquired = $mutex.WaitOne(0) }
    catch [Threading.AbandonedMutexException] { $acquired = $true }
    if (!$acquired) { $mutex.Dispose(); throw 'Another integration run owns the test registration or desktop. Nothing was changed.' }
    $session = @{
        RunId = $runId; Root = $root; Layout = $layout; Mutex = $mutex; MutexHeld = $true
        Processes = [Collections.Generic.List[object]]::new(); Phase = 0; Before = @{}; PriorDirectories = @()
        Prior = $null; Package = $null; RegisteredOwned = $false; UnregisteredPrior = $false
        RunRoot = $null; Executable = $null; CleanupStarted = $false; Completed = $false
    }
    try {
        if (Get-Process -Name NotepadsEditorTests, NotepadsWorkspaceTests, NotepadsIntegrationTests -ErrorAction SilentlyContinue) {
            throw 'A test process is already running. Its registration and process were left untouched.'
        }
        $prior = Get-AppxPackage -Name Notepads.Editor.Tests
        $session.Prior = $prior
        if ($prior -and (!$prior.IsDevelopmentMode -or $prior.PackageFamilyName -ne $family -or
            !(Test-Path -LiteralPath (Join-Path $prior.InstallLocation 'AppxManifest.xml')))) {
            throw 'The existing test package is not a restorable loose registration.'
        }
        if ($ExpectedRegistration) {
            $expected = Get-Content -LiteralPath $ExpectedRegistration -Raw | ConvertFrom-Json
            if (!$prior -or $prior.PackageFullName -ne $expected.PackageFullName -or $prior.InstallLocation -ne $expected.InstallLocation) {
                throw 'The exact expected test registration is not present. Nothing was changed.'
            }
        }
        New-Item -ItemType Directory -Path $layout -Force | Out-Null
        $localState = Join-Path $env:LOCALAPPDATA "Packages\$family\LocalState"
        # Data can survive Remove-AppxPackage -PreserveApplicationData even when
        # Get-AppxPackage returns nothing. Snapshot it in either state.
        if (Test-Path -LiteralPath $localState) {
            $session.PriorDirectories = @($localState) + @(Get-ChildItem -LiteralPath $localState -Directory -Recurse -Force | ForEach-Object FullName)
            foreach ($file in Get-ChildItem -LiteralPath $localState -File -Recurse -Force) {
                $session.Before[$file.FullName] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
            }
        }
        $session.Before | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $root 'prior-data-hashes.json') -Encoding utf8
        if ($prior) {
            $prior | Select-Object Name, PackageFullName, PackageFamilyName, InstallLocation, Architecture, IsDevelopmentMode |
                ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'prior-registration.json') -Encoding utf8
        }
        if ($SourceLayout) {
            Get-ChildItem -LiteralPath ([IO.Path]::GetFullPath($SourceLayout)) | Copy-Item -Destination $layout -Recurse
        }
        else {
            $runtime = 'win-' + $Platform.ToLowerInvariant()
            $recipePath = Join-Path $repo "tests\IntegrationTests\bin\App\$Platform\$Configuration\net10.0-windows10.0.26100.0\$runtime\IntegrationTests.build.appxrecipe"
            if (!(Test-Path -LiteralPath $recipePath)) { throw "Build the App integration profile first: $recipePath" }
            [xml] $recipe = Get-Content -LiteralPath $recipePath
            foreach ($group in $recipe.Project.ItemGroup) {
                foreach ($file in $group.ChildNodes) {
                    if (!$file.PackagePath -or !$file.Include) { continue }
                    $target = [IO.Path]::GetFullPath((Join-Path $layout $file.PackagePath))
                    if (!$target.StartsWith($layout + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Recipe entry escapes the owned layout.' }
                    New-Item -ItemType Directory -Path (Split-Path $target) -Force | Out-Null
                    Copy-Item -LiteralPath ([Uri]::UnescapeDataString($file.Include)) -Destination $target
                }
            }
        }
        $manifestPath = Join-Path $layout 'AppxManifest.xml'
        $marker = Join-Path $layout 'TestIsolation.txt'
        if (!(Test-Path -LiteralPath $marker) -or (Get-Content -LiteralPath $marker -First 1) -ne 'Notepads.TestStorage/1') {
            throw 'The package lacks the isolated-storage contract.'
        }
        [xml] $manifest = Get-Content -LiteralPath $manifestPath
        $app = @($manifest.Package.Applications.Application)
        if ($manifest.Package.Identity.Name -ne 'Notepads.Editor.Tests' -or
            $manifest.Package.Identity.ProcessorArchitecture -ine $Platform -or
            $manifest.Package.Identity.Publisher -ne 'CN=40E66D07-5A3A-4954-9CA3-A1EB15ED0804' -or
            $app.Count -ne 1 -or $app[0].Extensions -or $app[0].Id -ne 'App' -or
            $app[0].Executable -ne 'NotepadsIntegrationTests.exe' -or $app[0].EntryPoint -ne 'Notepads.App') {
            throw 'Only the integration test identity, executable and entry point without activation extensions may be registered.'
        }
        $session.Executable = Join-Path $layout 'NotepadsIntegrationTests.exe'
        if (!(Test-Path -LiteralPath $session.Executable -PathType Leaf)) { throw 'The isolated executable is missing.' }
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
        $current = Get-AppxPackage -Name Notepads.Editor.Tests
        if (($prior -and (!$current -or $current.PackageFullName -ne $prior.PackageFullName -or $current.InstallLocation -ne $prior.InstallLocation)) -or
            (!$prior -and $current)) { throw 'Test registration changed while staging. Nothing was unregistered.' }
        if (Get-Process -Name NotepadsEditorTests, NotepadsWorkspaceTests, NotepadsIntegrationTests -ErrorAction SilentlyContinue) {
            throw 'A test process started while staging; it was left untouched.'
        }
        if ($prior) {
            Remove-AppxPackage -Package $prior.PackageFullName -PreserveApplicationData
            $session.UnregisteredPrior = $true
        }
        Add-AppxPackage -Register $manifestPath
        $session.Package = Get-AppxPackage -Name Notepads.Editor.Tests
        if (!$session.Package -or [IO.Path]::GetFullPath($session.Package.InstallLocation) -ne $layout -or $session.Package.PackageFamilyName -ne $family) {
            throw 'Registration does not match the owned layout and expected package family.'
        }
        $session.RegisteredOwned = $true
        $session.RunRoot = [IO.Path]::GetFullPath((Join-Path $localState "WorkspaceTests\$runId"))
        New-Item -ItemType Directory -Path $session.RunRoot -Force | Out-Null
        [IO.File]::WriteAllText((Join-Path $session.RunRoot 'runner-sentinel.txt'), $runId)
        Assert-IsolatedTestRunOwnership $session
        return $session
    }
    catch {
        $failure = $_
        try { Complete-IsolatedTestPackage $session -SkipHostCleanup }
        catch { throw "Startup failed: $failure`nPreservation cleanup also failed: $_" }
        throw $failure
    }
}

function Complete-IsolatedTestPackage {
    param([hashtable] $Session, [switch] $SkipHostCleanup)
    if (!$Session -or $Session.Completed) { return }
    $errors = [Collections.Generic.List[string]]::new()
    try {
        foreach ($identity in $Session.Processes.ToArray()) {
            try {
                if ((Get-IsolatedTestProcess $identity) -and !$identity.ExpectedTermination) {
                    $errors.Add('An ordinary owned process required forced termination during cleanup.')
                }
                Stop-IsolatedTestProcess $Session $identity -CleanupAfterFailure
            }
            catch { $errors.Add($_.ToString()) }
        }
        if ($Session.Processes.Count -eq 0 -and $Session.RegisteredOwned -and !$SkipHostCleanup) {
            try {
                $Session.CleanupStarted = $true
                Invoke-IsolatedTestPhase $Session '--test-cleanup' -TimeoutSeconds 60
                $beforePath = Join-Path $Session.RunRoot 'grants-before.json'
                $afterPath = Join-Path $Session.RunRoot 'grants-after.json'
                if (!(Test-Path -LiteralPath $beforePath) -or !(Test-Path -LiteralPath $afterPath)) { throw 'Missing file-grant preservation reports.' }
                $beforeMap = Get-Content -LiteralPath $beforePath -Raw | ConvertFrom-Json
                $afterMap = Get-Content -LiteralPath $afterPath -Raw | ConvertFrom-Json
                $beforeEntries = @($beforeMap.PSObject.Properties | Sort-Object Name | ForEach-Object { @($_.Name, $_.Value) | ConvertTo-Json -Compress })
                $afterEntries = @($afterMap.PSObject.Properties | Sort-Object Name | ForEach-Object { @($_.Name, $_.Value) | ConvertTo-Json -Compress })
                if (($beforeEntries -join "`n") -cne ($afterEntries -join "`n")) { throw 'Pre-existing file-grant token/metadata map was not restored.' }
            }
            catch { $errors.Add($_.ToString()) }
        }
        foreach ($identity in $Session.Processes.ToArray()) {
            try { Stop-IsolatedTestProcess $Session $identity -CleanupAfterFailure }
            catch { $errors.Add($_.ToString()) }
        }
        if ($Session.Processes.Count -eq 0) {
            try {
                $current = Get-AppxPackage -Name Notepads.Editor.Tests
                if ($current -and [IO.Path]::GetFullPath($current.InstallLocation) -eq $Session.Layout) {
                    Remove-AppxPackage -Package $current.PackageFullName -PreserveApplicationData
                }
                elseif ($Session.RegisteredOwned -and $current) { throw 'An unexpected registration appeared; it was left untouched.' }
                if ($Session.UnregisteredPrior) {
                    if (Get-AppxPackage -Name Notepads.Editor.Tests) { throw 'Cannot restore the prior registration over an unexpected package.' }
                    Add-AppxPackage -Register (Join-Path $Session.Prior.InstallLocation 'AppxManifest.xml')
                    $restored = Get-AppxPackage -Name Notepads.Editor.Tests
                    if (!$restored -or $restored.PackageFullName -ne $Session.Prior.PackageFullName -or
                        $restored.InstallLocation -ne $Session.Prior.InstallLocation -or $restored.Architecture -ne $Session.Prior.Architecture) {
                        throw 'The exact prior test registration was not restored.'
                    }
                }
                elseif ($Session.RegisteredOwned -and !$Session.Prior -and (Get-AppxPackage -Name Notepads.Editor.Tests)) {
                    throw 'The original absence of a test registration was not restored.'
                }
                Assert-IsolatedTestPreservation $Session
                if ($Session.RunRoot -and (Get-Content -LiteralPath (Join-Path $Session.RunRoot 'runner-sentinel.txt') -Raw) -ne $Session.RunId) {
                    throw 'The owned sentinel did not survive registration restoration.'
                }
            }
            catch { $errors.Add($_.ToString()) }
        }
        else { $errors.Add('An unverified process remains. Registration was left untouched rather than terminating an unowned process.') }
        if (Test-Path -LiteralPath $Session.Root) {
            @{ Run = $Session.RunId; Data = $Session.RunRoot; PriorFileCount = $Session.Before.Count;
               GrantCheck = 'token-and-metadata-only'; Status = $(if ($errors.Count) { 'failed' } else { 'passed' }); Errors = @($errors) } |
                ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $Session.Root 'preservation.json') -Encoding utf8
        }
    }
    finally {
        $Session.Completed = $true
        foreach ($identity in $Session.Processes) {
            if ($identity.Process) { $identity.Process.Dispose(); $identity.Process = $null }
        }
        if ($Session.MutexHeld) { $Session.Mutex.ReleaseMutex(); $Session.MutexHeld = $false }
        $Session.Mutex.Dispose()
    }
    Write-Output "Isolated evidence: $($Session.Root); data: $($Session.RunRoot)"
    if ($errors.Count) { throw ($errors -join "`n") }
}
