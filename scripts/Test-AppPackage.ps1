# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

param([Parameter(Mandatory)][string] $Path, [string[]] $ExpectedArchitectures)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$architectures = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)

. (Join-Path $PSScriptRoot 'DevBuildTools.ps1')

function Test-Package($Archive, [string] $Name, [bool] $StoreUpload) {
    $entry = $Archive.GetEntry('AppxManifest.xml')
    if (!$entry) { throw "Missing manifest: $Name" }
    $reader = [IO.StreamReader]::new($entry.Open())
    try { [xml]$manifest = $reader.ReadToEnd() } finally { $reader.Dispose() }
    if ($StoreUpload -and (($Archive.Entries | Where-Object Name -ieq 'Notepads.DocumentAssets.exe' | Select-Object -First 1) -or
        $manifest.SelectNodes("//*[local-name()='Extension' and @Category='windows.fullTrustProcess']").Count -or
        $manifest.SelectNodes("//*[local-name()='Capability' and @Name='runFullTrust']").Count)) {
        throw "DocumentAssets/full-trust declarations are forbidden in Store uploads: $Name"
    }
    $applications = $manifest.SelectNodes("/*[local-name()='Package']/*[local-name()='Applications']/*[local-name()='Application']")
    if ($applications.Count -eq 0) { return } # Resource package.
    $identity = $manifest.SelectSingleNode("/*[local-name()='Package']/*[local-name()='Identity']")
    $architecture = $identity.GetAttribute('ProcessorArchitecture')
    $null = $architectures.Add($architecture)
    if ($Archive.GetEntry('Notepads.exe')) {
        $dependencies = @($manifest.Package.Dependencies.PackageDependency.Name)
        $hasVCLibs = 'Microsoft.VCLibs.140.00' -in $dependencies -or 'Microsoft.VCLibs.140.00.Debug' -in $dependencies
        if ('Microsoft.UI.Xaml.2.8' -notin $dependencies -or
            'Microsoft.Services.Store.Engagement' -notin $dependencies -or !$hasVCLibs) {
            throw "Missing native framework dependency: $Name"
        }
        $families = @($manifest.Package.Dependencies.TargetDeviceFamily)
        if ($families.Count -ne 1 -or $families[0].Name -ne 'Windows.Desktop' -or
            $families[0].MinVersion -ne '10.0.22000.0') {
            throw "Unexpected minimum Windows version: $Name"
        }
        foreach ($required in @('Notepads.Markdown.dll', 'Notepads.Markdown.winmd', 'Notepads.Markdown.pri',
            'Notepads.Markdown/Themes/Generic.xaml', 'ThirdPartyNotices/Notepads.Markdown/LICENSE')) {
            if (!$Archive.GetEntry($required)) { throw "Missing Markdown package asset ${required}: $Name" }
        }
        if (!@($Archive.Entries | Where-Object {
            $_.FullName -like 'ThirdPartyNotices/Notepads.Markdown/*/*' -and $_.Name -like '*LICENSE*'
        }).Count) { throw "Missing Markdown third-party notices: $Name" }
        $metadata = [IO.MemoryStream]::new()
        try {
            $input = $Archive.GetEntry('Notepads.Markdown.winmd').Open()
            try { $input.CopyTo($metadata) }
            finally { $input.Dispose() }
            if ([Text.Encoding]::UTF8.GetString($metadata.ToArray()).Contains('CoreValidation')) {
                throw "Markdown diagnostics leaked into a product package: $Name"
            }
        } finally { $metadata.Dispose() }
    }
    foreach ($app in $applications) {
        if (!$Archive.GetEntry($app.GetAttribute('Executable'))) { throw "Missing executable: $Name" }
    }
    foreach ($file in $Archive.Entries) {
        if ($file.Name -match '^(coreclr|clrjit)\.dll$') { throw "Managed runtime in Native AOT package: $Name" }
        if ($file.Name -match '\.(exe|dll)$') { Assert-DevNativeImage ($file.Open()) $file.FullName $architecture }
    }
    Write-Output "PASS: $Name contains native executables and libraries without managed assemblies or CoreCLR."
}

function Test-Archive($Archive, [string] $Name, [bool] $StoreUpload = $false) {
    $StoreUpload = $StoreUpload -or $Name -match '\.(msixupload|appxupload)$'
    if ($Archive.GetEntry('AppxManifest.xml')) { Test-Package $Archive $Name $StoreUpload }
    else {
        $packages = @($Archive.Entries | Where-Object { $_.Name -match '\.(msixbundle|appxbundle|msix|appx)$' })
        if (!$packages.Count) { throw "No packages found in $Name." }
        if ($Name -match '\.(msixupload|appxupload)$' -and !($Archive.Entries | Where-Object { $_.Name -match '\.(appxsym|msixsym)$' })) {
            throw "Missing publishing symbols: $Name"
        }
        foreach ($entry in $packages) {
            $package = [IO.Compression.ZipArchive]::new($entry.Open(), [IO.Compression.ZipArchiveMode]::Read)
            try { Test-Archive $package $entry.Name $StoreUpload } finally { $package.Dispose() }
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
