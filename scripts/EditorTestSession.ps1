# ---------------------------------------------------------------------------------------------
# Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
# See LICENSE file in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Shared lifecycle for isolated, interactive editor tests. A pre-existing test
# package is deliberately preserved; production packages are never considered.
function Read-EditorTestFile {
    param([string] $Path, [switch] $AsLines, [switch] $CompleteLinesOnly)
    # Protocol files are published by atomic rename, and timelines are appended
    # concurrently. Readers must permit both writers and deletion/rename handles.
    $sharing = [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete
    $stream = [IO.FileStream]::new($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, $sharing)
    $reader = $null
    try {
        $reader = [IO.StreamReader]::new($stream, [Text.UTF8Encoding]::new($false, $true), $true)
        $text = $reader.ReadToEnd()
        if ($AsLines) {
            if ($CompleteLinesOnly) {
                # An append can be observed halfway through its last TSV record.
                $lastNewline = $text.LastIndexOf("`n")
                if ($lastNewline -lt 0) { return ,@() }
                $text = $text.Substring(0, $lastNewline + 1)
            }
            return ,($text -split "`r`n|`n|`r")
        }
        return $text
    }
    finally {
        if ($reader) { $reader.Dispose() } else { $stream.Dispose() }
    }
}

function Start-EditorTestSession {
    param([ValidateSet('x64', 'ARM64')] [string] $Platform = 'ARM64', [int] $TimeoutSeconds = 40)
    if (Get-AppxPackage -Name Notepads.Editor.Tests) {
        throw 'Notepads.Editor.Tests is already registered. Its registration and processes were left untouched.'
    }
    $repo = Split-Path $PSScriptRoot -Parent
    $runtime = 'win-' + $Platform.ToLowerInvariant()
    $recipePath = Join-Path $repo "tests\UwpEditor.Tests\bin\$Platform\Release\net10.0-windows10.0.26100.0\$runtime\UwpEditor.Tests.build.appxrecipe"
    if (!(Test-Path -LiteralPath $recipePath)) {
        throw "Publish UwpEditor.Tests.csproj with Configuration=Release, Platform=$Platform first. Missing $recipePath"
    }
    $runId = [Guid]::NewGuid().ToString('N')
    $root = [IO.Path]::GetFullPath((Join-Path $repo "artifacts\editor-automation\$runId"))
    $layout = [IO.Path]::GetFullPath((Join-Path $root 'layout'))
    $session = @{ RunId = $runId; Root = $root; Layout = $layout; Sequence = 0; ProcessId = 0; Package = $null }
    try {
        New-Item -ItemType Directory -Path $layout -Force | Out-Null
        [xml] $recipe = Read-EditorTestFile $recipePath
        foreach ($group in $recipe.Project.ItemGroup) {
            foreach ($file in $group.ChildNodes) {
                if (!$file.PackagePath -or !$file.Include) { continue }
                $target = [IO.Path]::GetFullPath((Join-Path $layout $file.PackagePath))
                if (!$target.StartsWith($layout + '\', [StringComparison]::OrdinalIgnoreCase)) {
                    throw "Package entry escapes the owned layout: $target"
                }
                New-Item -ItemType Directory -Path (Split-Path $target) -Force | Out-Null
                Copy-Item -LiteralPath ([Uri]::UnescapeDataString($file.Include)) -Destination $target
            }
        }
        $manifestPath = Join-Path $layout 'AppxManifest.xml'
        [xml] $manifest = Read-EditorTestFile $manifestPath
        if ($manifest.Package.Identity.Name -ne 'Notepads.Editor.Tests') {
            throw 'Refusing to register a manifest other than Notepads.Editor.Tests.'
        }
        # Check again immediately before registering; never replace another run.
        if (Get-AppxPackage -Name Notepads.Editor.Tests) { throw 'Another test package appeared while preparing the layout.' }
        Add-AppxPackage -Register $manifestPath
        $package = Get-AppxPackage -Name Notepads.Editor.Tests
        if (!$package -or [IO.Path]::GetFullPath($package.InstallLocation) -ne $layout) {
            throw 'Test registration does not belong to this run.'
        }
        $session.Package = $package.PackageFullName
        $session.Channel = Join-Path $env:LOCALAPPDATA "Packages\$($package.PackageFamilyName)\LocalState\InteractiveTests\$runId"
        if (!('IsolatedEditorTestLauncher' -as [type])) {
            Add-Type @'
using System;
using System.Runtime.InteropServices;
[ComImport, Guid("2e941141-7f97-4756-ba1d-9decde894a3d"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IIsolatedEditorActivationManager {
    [PreserveSig] int ActivateApplication([MarshalAs(UnmanagedType.LPWStr)] string appId, [MarshalAs(UnmanagedType.LPWStr)] string arguments, uint options, out uint processId);
    [PreserveSig] int ActivateForFile(IntPtr items, string verb, out uint processId);
    [PreserveSig] int ActivateForProtocol(IntPtr items, out uint processId);
}
public static class IsolatedEditorTestLauncher {
    public static uint Launch(string appId, string arguments) {
        var manager = (IIsolatedEditorActivationManager)Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("45ba127d-10a8-46ea-8ab7-56ea9078943c")));
        try {
            uint processId;
            Marshal.ThrowExceptionForHR(manager.ActivateApplication(appId, arguments, 0, out processId));
            return processId;
        } finally { Marshal.ReleaseComObject(manager); }
    }
}
'@
        }
        $session.ProcessId = [IsolatedEditorTestLauncher]::Launch("$($package.PackageFamilyName)!App", "--interactive-test $runId")
        $process = Get-Process -Id $session.ProcessId
        if (!$process.Path.StartsWith($layout + '\', [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Activation returned a process outside the owned test layout.'
        }
        $session.ProcessStartTicks = $process.StartTime.ToUniversalTime().Ticks
        $session | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'session.json') -Encoding UTF8
        $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
        while (!(Test-Path -LiteralPath (Join-Path $session.Channel 'ready.txt'))) {
            Assert-EditorTestAlive $session
            if ([DateTime]::UtcNow -gt $deadline) { throw "Interactive editor startup timed out. Evidence: $root" }
            Start-Sleep -Milliseconds 100
        }
        return $session
    }
    catch {
        Stop-EditorTestSession $session
        throw
    }
}

function Assert-EditorTestAlive {
    param([hashtable] $Session)
    $process = Get-Process -Id $Session.ProcessId -ErrorAction SilentlyContinue
    if (!$process) { throw "Isolated editor process exited. Evidence: $($Session.Root)" }
    if (!$process.Path.StartsWith($Session.Layout + '\', [StringComparison]::OrdinalIgnoreCase) -or
        $process.StartTime.ToUniversalTime().Ticks -ne $Session.ProcessStartTicks) {
        throw 'The test process identity changed.'
    }
    $fatal = Join-Path $Session.Channel 'fatal.txt'
    if (Test-Path -LiteralPath $fatal) { throw (Read-EditorTestFile $fatal) }
}

function Invoke-EditorTestCommand {
    param([hashtable] $Session, [string] $Command, [int] $TimeoutSeconds = 15)
    Assert-EditorTestAlive $Session
    $Session.Sequence++
    $id = '{0:D4}' -f $Session.Sequence
    $commandPath = Join-Path $Session.Channel "command-$id.txt"
    [IO.File]::WriteAllText($commandPath + '.tmp', $Command, [Text.UTF8Encoding]::new($false))
    [IO.File]::Move($commandPath + '.tmp', $commandPath)
    $response = Join-Path $Session.Channel "response-$id.txt"
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while (!(Test-Path -LiteralPath $response)) {
        Assert-EditorTestAlive $Session
        if ([DateTime]::UtcNow -gt $deadline) { throw "Interactive command timed out: $Command" }
        Start-Sleep -Milliseconds 25
    }
    $lines = Read-EditorTestFile $response -AsLines
    if ($lines[0] -ne 'OK') { throw ($lines -join "`n") }
    $state = @{}
    foreach ($line in $lines | Select-Object -Skip 1) {
        $pair = $line.Split(@('='), 2)
        if ($pair.Length -eq 2) { $state[$pair[0]] = $pair[1] }
    }
    return $state
}

function Stop-EditorTestSession {
    param([hashtable] $Session)
    if (!$Session) { return }
    # Keep command/response evidence before unregistering the isolated package.
    if ($Session.Channel -and (Test-Path -LiteralPath $Session.Channel)) {
        try {
            $evidence = Join-Path $Session.Root 'commands'
            New-Item -ItemType Directory -Path $evidence -Force | Out-Null
            Get-ChildItem -LiteralPath $Session.Channel -File | Copy-Item -Destination $evidence -Force
        }
        catch { Write-Warning "Could not copy test evidence: $_" }
    }
    if ($Session.ProcessId) {
        $process = Get-Process -Id $Session.ProcessId -ErrorAction SilentlyContinue
        if ($process) {
            if (!$process.Path.StartsWith($Session.Layout + '\', [StringComparison]::OrdinalIgnoreCase) -or
                ($Session.ProcessStartTicks -and $process.StartTime.ToUniversalTime().Ticks -ne $Session.ProcessStartTicks)) {
                throw 'Refusing to stop a process not owned by this test run.'
            }
            Stop-Process -Id $process.Id
            if (!$process.WaitForExit(5000)) { throw 'Owned test process did not exit.' }
        }
    }
    $package = Get-AppxPackage -Name Notepads.Editor.Tests
    if ($package -and [IO.Path]::GetFullPath($package.InstallLocation) -eq $Session.Layout) {
        Remove-AppxPackage -Package $package.PackageFullName -PreserveApplicationData
    }
    # No recursive deletion: the unique layout and reports are retained as evidence.
}
