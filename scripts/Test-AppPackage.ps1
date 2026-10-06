# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE file in the project root for license information.
# ---------------------------------------------------------------------------------------------

param([Parameter(Mandatory)][string] $Path, [string[]] $ExpectedArchitectures)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$architectures = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)

function Test-NativeImage($Entry) {
    $stream = $Entry.Open()
    $reader = [IO.BinaryReader]::new($stream)
    try {
        if ($reader.ReadUInt16() -ne 0x5A4D) { throw "Invalid PE image: $($Entry.FullName)" }
        $null = $reader.ReadBytes(58)
        $offset = $reader.ReadUInt32()
        if ($offset -lt 64 -or $offset -gt 1048576) { throw 'Invalid PE header offset.' }
        $null = $reader.ReadBytes($offset - 64)
        if ($reader.ReadUInt32() -ne 0x4550) { throw 'Invalid PE signature.' }
        $null = $reader.ReadBytes(20)
        $magic = $reader.ReadUInt16()
        $directoryOffset = switch ($magic) { 0x10B { 96 } 0x20B { 112 } default { throw 'Invalid optional header.' } }
        $null = $reader.ReadBytes($directoryOffset - 2 + 14 * 8)
        if ($reader.ReadUInt32() -ne 0) { throw "Managed image in Native AOT package: $($Entry.FullName)" }
    } finally { $reader.Dispose() }
}

function Test-Package($Archive, [string] $Name) {
    $entry = $Archive.GetEntry('AppxManifest.xml')
    if (!$entry) { throw "Missing manifest: $Name" }
    $reader = [IO.StreamReader]::new($entry.Open())
    try { [xml]$manifest = $reader.ReadToEnd() } finally { $reader.Dispose() }
    $applications = $manifest.SelectNodes("/*[local-name()='Package']/*[local-name()='Applications']/*[local-name()='Application']")
    if ($applications.Count -eq 0) { return } # Resource package.
    $identity = $manifest.SelectSingleNode("/*[local-name()='Package']/*[local-name()='Identity']")
    $null = $architectures.Add($identity.GetAttribute('ProcessorArchitecture'))
    if ($Archive.GetEntry('Notepads.exe')) {
        $dependencies = @($manifest.Package.Dependencies.PackageDependency.Name)
        $hasVCLibs = 'Microsoft.VCLibs.140.00' -in $dependencies -or 'Microsoft.VCLibs.140.00.Debug' -in $dependencies
        if ('Microsoft.Services.Store.Engagement' -notin $dependencies -or !$hasVCLibs) {
            throw "Missing native framework dependency: $Name"
        }
        if ($manifest.Package.Dependencies.TargetDeviceFamily.MinVersion -ne '10.0.17763.0') {
            throw "Unexpected minimum Windows version: $Name"
        }
    }
    foreach ($app in $applications) {
        if (!$Archive.GetEntry($app.GetAttribute('Executable'))) { throw "Missing executable: $Name" }
    }
    foreach ($file in $Archive.Entries) {
        if ($file.Name -match '^(coreclr|clrjit)\.dll$') { throw "Managed runtime in Native AOT package: $Name" }
        if ($file.Name -match '\.(exe|dll)$') { Test-NativeImage $file }
    }
    Write-Output "PASS: $Name contains native executables and libraries without managed assemblies or CoreCLR."
}

function Test-Archive($Archive, [string] $Name) {
    if ($Archive.GetEntry('AppxManifest.xml')) { Test-Package $Archive $Name }
    else {
        $packages = @($Archive.Entries | Where-Object { $_.Name -match '\.(msixbundle|appxbundle|msix|appx)$' })
        if (!$packages.Count) { throw "No packages found in $Name." }
        if ($Name -match '\.(msixupload|appxupload)$' -and !($Archive.Entries | Where-Object { $_.Name -match '\.(appxsym|msixsym)$' })) {
            throw "Missing publishing symbols: $Name"
        }
        foreach ($entry in $packages) {
            $package = [IO.Compression.ZipArchive]::new($entry.Open(), [IO.Compression.ZipArchiveMode]::Read)
            try { Test-Archive $package $entry.Name } finally { $package.Dispose() }
        }
    }
}

$archive = [IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $Path).Path)
try {
    Test-Archive $archive ([IO.Path]::GetFileName($Path))
} finally { $archive.Dispose() }

if ($ExpectedArchitectures -and !$architectures.SetEquals($ExpectedArchitectures)) {
    throw "Package architectures ($($architectures -join ', ')) do not match the requested platforms ($($ExpectedArchitectures -join ', '))."
}
