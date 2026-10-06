# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE file in the project root for license information.
# ---------------------------------------------------------------------------------------------

param([int] $TimeoutSeconds = 120, [ValidateSet('Debug', 'Release')] [string] $Configuration = 'Release', [switch] $Preview, [switch] $BidiPreview, [ValidateSet('All', 'Search', 'NativeStorage', 'RegexMemory', 'Persistence', 'SessionResilience', 'Performance', 'Syntax', 'Diff')] [string] $TestGroup = 'All')
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$recipePath = Join-Path $repo "tests\UwpEditor.Tests\bin\x64\$Configuration\net10.0-windows10.0.26100.0\win-x64\UwpEditor.Tests.build.appxrecipe"
if (!(Test-Path $recipePath)) { throw "Publish tests/UwpEditor.Tests/UwpEditor.Tests.csproj with Configuration=$Configuration and Platform=x64 before running this script." }
[xml] $recipe = Get-Content -LiteralPath $recipePath
$existing = Get-AppxPackage Notepads.Editor.Tests
if ($existing) { Remove-AppxPackage $existing.PackageFullName }
$output = [IO.Path]::GetFullPath((Split-Path $recipePath))
$layout = [IO.Path]::GetFullPath((Join-Path $output '_EditorTestLayout'))
if (!$layout.StartsWith($output + '\', [StringComparison]::OrdinalIgnoreCase)) { throw "Invalid test layout: $layout" }
if (Test-Path -LiteralPath $layout) { Remove-Item -LiteralPath $layout -Recurse -Force }
foreach ($group in $recipe.Project.ItemGroup) {
    foreach ($file in $group.ChildNodes) {
        if (!$file.PackagePath -or !$file.Include) { continue }
        $target = [IO.Path]::GetFullPath((Join-Path $layout $file.PackagePath))
        if (!$target.StartsWith($layout + '\', [StringComparison]::OrdinalIgnoreCase)) { throw "Invalid package path: $target" }
        New-Item -ItemType Directory -Path (Split-Path $target) -Force | Out-Null
        Copy-Item -LiteralPath ([Uri]::UnescapeDataString($file.Include)) -Destination $target -Force
    }
}
$manifest = Join-Path $layout 'AppxManifest.xml'
Add-AppxPackage -Register $manifest
$package = Get-AppxPackage Notepads.Editor.Tests
$resultPath = Join-Path $env:LOCALAPPDATA "Packages\$($package.PackageFamilyName)\LocalState\results.txt"
if (Test-Path $resultPath) { Remove-Item -LiteralPath $resultPath }
Add-Type @'
using System;
using System.Runtime.InteropServices;
[ComImport, Guid("2e941141-7f97-4756-ba1d-9decde894a3d"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IApplicationActivationManager {
    [PreserveSig]
    int ActivateApplication([MarshalAs(UnmanagedType.LPWStr)] string appId, [MarshalAs(UnmanagedType.LPWStr)] string arguments, uint options, out uint processId);
    [PreserveSig]
    int ActivateForFile(IntPtr items, string verb, out uint processId);
    [PreserveSig]
    int ActivateForProtocol(IntPtr items, out uint processId);
}
public static class EditorTestLauncher {
    public static uint Launch(string appId, string arguments) {
        var manager = (IApplicationActivationManager)Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("45ba127d-10a8-46ea-8ab7-56ea9078943c")));
        uint processId;
        Marshal.ThrowExceptionForHR(manager.ActivateApplication(appId, arguments, 0, out processId));
        return processId;
    }
}
'@
$testProcessId = [EditorTestLauncher]::Launch("$($package.PackageFamilyName)!App", $(if ($BidiPreview) { '--bidi-preview' } elseif ($Preview) { '--preview' } elseif ($TestGroup -eq 'Search') { '--search' } elseif ($TestGroup -eq 'NativeStorage') { '--native-storage' } elseif ($TestGroup -eq 'RegexMemory') { '--regex-memory' } elseif ($TestGroup -eq 'Persistence') { '--persistence' } elseif ($TestGroup -eq 'SessionResilience') { '--session-resilience' } elseif ($TestGroup -eq 'Diff') { '--diff' } elseif ($TestGroup -eq 'Syntax') { '--syntax' } elseif ($TestGroup -eq 'Performance') { '--performance' } else { '' }))
$deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
while (!(Test-Path $resultPath)) {
    if (!(Get-Process -Id $testProcessId -ErrorAction SilentlyContinue)) {
        $diagnosticPath = Join-Path (Split-Path $resultPath) 'diagnostic.txt'
        $diagnostic = if (Test-Path $diagnosticPath) { Get-Content -LiteralPath $diagnosticPath -Raw } else { 'No diagnostic checkpoint was written.' }
        throw "Editor test process exited before reporting a result. Last checkpoint: $diagnostic"
    }
    if ([DateTime]::UtcNow -gt $deadline) { throw "Editor tests timed out. Check the Application event log for a crash. Results: $resultPath" }
    Start-Sleep -Milliseconds 500
}
$result = Get-Content -LiteralPath $resultPath -Raw
Write-Output $result
if ($result.Contains('FAIL:')) { throw 'UWP editor integration tests failed.' }
