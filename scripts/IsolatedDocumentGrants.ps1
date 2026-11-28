# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Issue and revoke single-file WinRT grants from the desktop. Windows PowerShell
# supplies the inbox WinRT projection, including on machines without a .NET build.
param(
    [Parameter(Mandatory)] [string] $FixtureRoot,
    [Parameter(Mandatory)] [string] $Protocol,
    [switch] $Release
)
$ErrorActionPreference = 'Stop'
$FixtureRoot = [IO.Path]::GetFullPath($FixtureRoot).TrimEnd('\', '/')
if ([IO.Path]::GetDirectoryName($FixtureRoot).TrimEnd('\', '/') -ine [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') -or
    [IO.Path]::GetFileName($FixtureRoot) -notmatch '^NotepadsBrokered-[a-f0-9]{32}$' -or
    ([IO.File]::GetAttributes($FixtureRoot) -band [IO.FileAttributes]::ReparsePoint)) {
    throw 'Invalid temporary document grant fixture.'
}
$ledger = Join-Path $FixtureRoot 'issued.txt'
$manager = [Windows.ApplicationModel.DataTransfer.SharedStorageAccessManager,Windows.ApplicationModel.DataTransfer,ContentType=WindowsRuntime]
if ($Release) {
    if ([IO.File]::Exists($ledger)) {
        $errors = @()
        foreach ($token in [IO.File]::ReadAllLines($ledger)) {
            try { $manager::RemoveFile($token) }
            catch { $errors += $_.Exception.GetBaseException().HResult.ToString('X8') }
        }
        if ($errors.Count) { throw ("Failed to revoke document grants (HRESULT " + ($errors -join ', ') + ').') }
    }
    return
}

Add-Type -AssemblyName System.Runtime.WindowsRuntime
$asTask = [System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
    $_.Name -eq 'AsTask' -and $_.IsGenericMethod -and $_.GetParameters().Count -eq 1 -and
    $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
} | Select-Object -First 1
$storageFile = [Windows.Storage.StorageFile,Windows.Storage,ContentType=WindowsRuntime]
$getTask = $asTask.MakeGenericMethod($storageFile)
$grants = [ordered]@{ fixtureName = [IO.Path]::GetFileName($FixtureRoot) }
$paths = [ordered]@{ canonical = 'canonical.txt'; casing = 'CASE-ENTRY.TXT'; junction = 'alias\junction.txt' }
if ([IO.File]::Exists((Join-Path $FixtureRoot 'leaf-link.txt'))) { $paths.link = 'leaf-link.txt' }
else { $grants.linkUnavailable = 'file symbolic links unavailable on the desktop' }
foreach ($entry in $paths.GetEnumerator()) {
    $path = Join-Path $FixtureRoot $entry.Value
    $task = $getTask.Invoke($null, @($storageFile::GetFileFromPathAsync($path)))
    $file = $task.GetAwaiter().GetResult()
    try { $token = $manager::AddFile($file) }
    catch {
        $cause = $_.Exception.GetBaseException()
        $code = $cause.HResult.ToString('X8')
        if ($entry.Key -in @('junction', 'link') -and $code -in @('C1130008', '800701C0')) {
            $grants[($entry.Key + 'Unavailable')] = "desktop AddFile rejected the reparse path (0x$code)"
            continue
        }
        throw "Could not issue the $($entry.Key) grant (HRESULT $code)."
    }
    try { [IO.File]::AppendAllLines($ledger, [string[]]@($token)) }
    catch { $manager::RemoveFile($token); throw }
    $grants[$entry.Key] = $token
}
[IO.File]::WriteAllText($Protocol, ($grants | ConvertTo-Json -Compress))
