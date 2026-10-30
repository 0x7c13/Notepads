# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Exercise the distributed entry point without downloads, elevation or registration.
param([string] $EvidenceRoot = '')
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
. (Join-Path $repo 'scripts/DevBuildTools.ps1')
if (!$EvidenceRoot) { $EvidenceRoot = Join-Path $repo ('artifacts/offline-contracts/' + [Guid]::NewGuid().ToString('N')) }
$root = Join-Path $EvidenceRoot 'bundle'
$architecture = Get-DevHostPlatform
$layout = Join-Path $root "App/$architecture"
$frameworks = Join-Path $root "Dependencies/$architecture"
$null = New-Item -ItemType Directory -Path $layout, $frameworks -Force
Copy-Item -LiteralPath (Join-Path $repo 'scripts/Install-Offline.ps1') -Destination (Join-Path $root 'Install.ps1')
$helper = Get-Content -LiteralPath (Join-Path $repo 'scripts/DevBuildTools.ps1') -Raw
$boundary = @'
function Enable-DevRegistration { [IO.File]::WriteAllText((Join-Path $PSScriptRoot 'enable-called.txt'), 'inert') }
function Test-DevRegistrationEnabled { $false }
function Get-AppxPackage {
    param($Name)
    if ($Name -eq '19282JackieLiu.Notepads-Beta' -and (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'register-called.txt'))) {
        [pscustomobject]@{ Publisher = 'CN=40E66D07-5A3A-4954-9CA3-A1EB15ED0804'; Architecture = (Get-DevHostPlatform);
            InstallLocation = (Join-Path $PSScriptRoot ('App/' + (Get-DevHostPlatform))); PackageFamilyName = 'inert' }
    }
}
function Add-AppxPackage {
    param($Register, $Path, $DependencyPath)
    if ($Path) { [IO.File]::WriteAllText((Join-Path $PSScriptRoot 'framework-called.txt'), $Path); return }
    if ($DependencyPath -or !(Test-Path -LiteralPath (Join-Path $PSScriptRoot 'framework-called.txt'))) {
        throw 'Frameworks must be deployed before loose registration, without Register+DependencyPath.'
    }
    [IO.File]::WriteAllText((Join-Path $PSScriptRoot 'register-called.txt'), $Register)
}
function Start-Process { throw 'UNEXPECTED process launch' }
function Save-DevDownload { throw 'UNEXPECTED network access' }
'@
[IO.File]::WriteAllText((Join-Path $root 'DevBuildTools.ps1'), ($helper + "`r`n" + $boundary))
$minimumWindows = '10.0.17763.0'
$manifest = @"
<Package><Identity Name="19282JackieLiu.Notepads-Beta" Publisher="CN=40E66D07-5A3A-4954-9CA3-A1EB15ED0804" Version="2.0.0.0" ProcessorArchitecture="$architecture" />
<Dependencies><TargetDeviceFamily MinVersion="$minimumWindows" /><PackageDependency Name="Framework.Test" Publisher="CN=Microsoft" MinVersion="1.0.0.0" /></Dependencies>
<Applications><Application Id="App" Executable="Notepads.exe" EntryPoint="Notepads.App"><VisualElements DisplayName="Notepads" /></Application></Applications></Package>
"@
[IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $manifest)
$exe = Join-Path $layout 'Notepads.exe'
[IO.File]::WriteAllText($exe, 'inert executable')
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::Open((Join-Path $frameworks 'framework.appx'), 'Create')
try {
    $writer = [IO.StreamWriter]::new($zip.CreateEntry('AppxManifest.xml').Open())
    try { $writer.Write("<Package><Identity Name=`"Framework.Test`" Publisher=`"CN=Microsoft`" Version=`"1.0.0.0`" ProcessorArchitecture=`"$architecture`" /></Package>") }
    finally { $writer.Dispose() }
} finally { $zip.Dispose() }
$info = [ordered]@{ Format = 1; Version = '2.0.0.0'; Commit = 'fixture'; Platforms = @($architecture); Files = @() }
function Write-Receipt {
    $info.Files = @(Get-ChildItem -LiteralPath $root -Recurse -File | Where-Object Name -ne 'bundle.json' | ForEach-Object {
        [ordered]@{ Path = $_.FullName.Substring($root.Length + 1).Replace('\', '/'); Sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }
    })
    $info | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $root 'bundle.json') -Encoding utf8
}
$checks = [Collections.Generic.List[string]]::new()
function Check([bool] $Condition, [string] $Name) {
    if (!$Condition) { throw $Name }
    $checks.Add($Name)
}
function Reject([string] $Name, [string] $Pattern) {
    $failure = $null
    try { & (Join-Path $root 'Install.ps1') -CheckOnly | Out-Null } catch { $failure = $_ }
    Check ($failure -and $failure.Exception.Message -like $Pattern) $Name
}
Write-Receipt
$result = & (Join-Path $root 'Install.ps1') -CheckOnly
Check ($result -like 'PASS:*') 'Distributed CheckOnly validates the complete offline payload'
Check (!(Test-Path -LiteralPath (Join-Path $root 'enable-called.txt')) -and
    !(Test-Path -LiteralPath (Join-Path $root 'register-called.txt'))) 'CheckOnly never enables Developer Mode or registers a package'
$savedFiles = $info.Files
$info.Files = @($info.Files | Where-Object Path -ne 'DevBuildTools.ps1')
$info | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $root 'bundle.json') -Encoding utf8
Reject 'An unhashed executable helper is rejected before loading it' '*unlisted file*'
$info.Files = $savedFiles
Write-Receipt
[IO.File]::WriteAllText($exe, 'damaged')
Reject 'Corrupt payload is rejected before any OS boundary' '*incomplete or damaged*'
[IO.File]::WriteAllText($exe, 'inert executable')
$info.Files += @{ Path = '../outside.txt'; Sha256 = ('a' * 64) }
$info | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $root 'bundle.json') -Encoding utf8
Reject 'Escaping receipt path is rejected' '*incomplete or damaged*'
$info.Platforms = @('unsupported')
Write-Receipt
Reject 'Unsupported host architecture is rejected' '*does not include*'
$info.Platforms = @($architecture)
# Modify the dependency identity inside the private fixture instead of deleting
# outside a known path. Rehashing keeps the corruption guard from masking this oracle.
[IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $manifest.Replace('Framework.Test', 'Missing.Framework'))
Write-Receipt
Reject 'A framework missing from the ZIP fails even on a provisioned machine' '*Missing framework package Missing.Framework*'
[IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $manifest.Replace('19282JackieLiu.Notepads-Beta', 'Notepads-Dev'))
Write-Receipt
Reject 'Offline installation requires the unchanged Production identity' '*Only the matching 19282JackieLiu.Notepads-Beta*'
[IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $manifest.Replace('DisplayName="Notepads"', 'DisplayName="Notepads-Dev"'))
Write-Receipt
Reject 'A Dev display name is rejected' '*display name*'
[IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $manifest.Replace('Version="2.0.0.0"', 'Version="2.0.1.0"'))
Write-Receipt
Reject 'Receipt version must match the payload' '*version or display name*'
[IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $manifest)
Write-Receipt
Check (!(Test-Path -LiteralPath (Join-Path $root 'enable-called.txt')) -and
    !(Test-Path -LiteralPath (Join-Path $root 'register-called.txt'))) 'Every rejected payload leaves system mutation boundaries untouched'
& (Join-Path $root 'Install.ps1') -NoLaunch
Check ((Get-Content -LiteralPath (Join-Path $root 'register-called.txt') -Raw) -eq (Join-Path $layout 'AppxManifest.xml')) 'NoLaunch registers the validated local layout without launching a process'
Check (Test-Path -LiteralPath (Join-Path $root 'enable-called.txt')) 'Installation checks/enables Developer Mode before registration'
Check (Test-Path -LiteralPath (Join-Path $root 'framework-called.txt')) 'A clean-machine install deploys the bundled signed framework before loose registration'

# Exercise the real CI export path against inert native-image ZIPs. Recompiling
# is forbidden here: the payload must come from the existing upload package.
$fixture = Join-Path $EvidenceRoot 'export'
$scripts = Join-Path $fixture 'scripts'
$null = New-Item -ItemType Directory -Path $scripts, (Join-Path $fixture 'src') -Force
foreach ($name in @('New-OfflinePackage.ps1', 'Install-Offline.ps1', 'DevBuildTools.ps1', 'TestBuildIdentity.ps1', 'Test-AppPackage.ps1')) {
    Copy-Item -LiteralPath (Join-Path $repo "scripts/$name") -Destination $scripts
}
Copy-Item -LiteralPath (Join-Path $repo 'LICENSE.txt') -Destination $fixture
[IO.File]::WriteAllText((Join-Path $fixture 'src/fixture.cs'), '// inert product source')
@{ 'src/fixture.cs' = (Get-FileHash (Join-Path $fixture 'src/fixture.cs')).Hash } | ConvertTo-Json |
    Set-Content -LiteralPath (Join-Path $fixture 'sources.json') -Encoding utf8
[IO.File]::WriteAllText((Join-Path $scripts 'Run-Dev.ps1'), "throw 'UNEXPECTED rebuild'")
function New-TestArchive([string] $Path, [hashtable] $Files) {
    $archive = [IO.Compression.ZipFile]::Open($Path, 'Create')
    try {
        foreach ($name in $Files.Keys) {
            $stream = $archive.CreateEntry($name).Open()
            try {
                $bytes = if ($Files[$name] -is [byte[]]) { $Files[$name] } else { [Text.Encoding]::UTF8.GetBytes($Files[$name]) }
                $stream.Write($bytes, 0, $bytes.Length)
            } finally { $stream.Dispose() }
        }
    } finally { $archive.Dispose() }
}
function New-TestNativeImage([string] $Architecture) {
    $image = [byte[]]::new(512)
    $image[0] = 0x4d; $image[1] = 0x5a; $image[60] = 128
    $image[128] = 0x50; $image[129] = 0x45
    $machine = switch ($Architecture) { 'x86' { 0x014c } 'x64' { 0x8664 } 'ARM64' { 0xaa64 } }
    [BitConverter]::GetBytes([uint16]$machine).CopyTo($image, 132)
    $magic = if ($Architecture -eq 'x86') { 0x010b } else { 0x020b }
    [BitConverter]::GetBytes([uint16]$magic).CopyTo($image, 152)
    return ,$image
}
$image = New-TestNativeImage $architecture
$payloadManifest = $manifest.Replace('<PackageDependency Name="Framework.Test" Publisher="CN=Microsoft" MinVersion="1.0.0.0" />',
    '<PackageDependency Name="Microsoft.UI.Xaml.2.8" Publisher="CN=Microsoft" MinVersion="1.0.0.0" /><PackageDependency Name="Microsoft.VCLibs.140.00" Publisher="CN=Microsoft" MinVersion="1.0.0.0" /><PackageDependency Name="Microsoft.Services.Store.Engagement" Publisher="CN=Microsoft" MinVersion="1.0.0.0" />')

# A correct manifest alone cannot prove that native DLLs match its architecture.
# Exercise the actual package validator with mixed-machine and missing-framework
# payloads as well as valid PE32/PE32+ images for every shipping architecture.
foreach ($target in @('x86', 'x64', 'ARM64')) {
    foreach ($case in @('valid', 'wrong-dll', 'wrong-header', 'managed-dll', 'missing-winui', 'missing-vclibs', 'missing-engagement')) {
        $targetManifest = $payloadManifest.Replace("ProcessorArchitecture=`"$architecture`"", "ProcessorArchitecture=`"$target`"")
        $targetImage = New-TestNativeImage $target
        $library = New-TestNativeImage $target
        $expectedError = ''
        switch ($case) {
            'wrong-dll' {
                $other = if ($target -eq 'x86') { 'x64' } else { 'x86' }
                $library = New-TestNativeImage $other
                $expectedError = '*Native image architecture does not match*fixture.dll*'
            }
            'wrong-header' {
                $library[153] = if ($target -eq 'x86') { 2 } else { 1 }
                $expectedError = '*Invalid optional header*fixture.dll*'
            }
            'managed-dll' {
                $directory = if ($target -eq 'x86') { 96 } else { 112 }
                $library[152 + $directory + 14 * 8] = 1
                $expectedError = '*Managed image in Native AOT package*fixture.dll*'
            }
        }
        if ($case -like 'missing-*') {
            $dependency = switch ($case) {
                'missing-winui' { 'Microsoft.UI.Xaml.2.8' }
                'missing-vclibs' { 'Microsoft.VCLibs.140.00' }
                'missing-engagement' { 'Microsoft.Services.Store.Engagement' }
            }
            $targetManifest = $targetManifest.Replace("<PackageDependency Name=`"$dependency`" Publisher=`"CN=Microsoft`" MinVersion=`"1.0.0.0`" />", '')
            $expectedError = '*Missing native framework dependency*'
        }
        $package = Join-Path $fixture "$target-$case.msix"
        New-TestArchive $package @{ 'AppxManifest.xml' = $targetManifest; 'Notepads.exe' = $targetImage; 'fixture.dll' = $library }
        $failure = $null
        try { & (Join-Path $repo 'scripts/Test-AppPackage.ps1') -Path $package -ExpectedArchitectures $target | Out-Null }
        catch { $failure = $_ }
        Check ((!$expectedError -and !$failure) -or
            ($expectedError -and $failure -and $failure.Exception.Message -like $expectedError)) "Native package validation: $target/$case ($failure)"
    }
}
foreach ($case in @('complete', 'split-resources', 'wrong-architecture', 'stale-source')) {
    $caseRoot = Join-Path $fixture $case
    $null = New-Item -ItemType Directory -Path $caseRoot
    $app = Join-Path $caseRoot 'app.msix'
    New-TestArchive $app @{ 'AppxManifest.xml' = $payloadManifest; 'Notepads.exe' = $image }
    foreach ($name in @('Microsoft.UI.Xaml.2.8', 'Microsoft.VCLibs.140.00', 'Microsoft.Services.Store.Engagement')) {
        New-TestArchive (Join-Path $caseRoot ($name + '.appx')) @{
            'AppxManifest.xml' = "<Package><Identity Name=`"$name`" Publisher=`"CN=Microsoft`" Version=`"1.0.0.0`" ProcessorArchitecture=`"$architecture`" /></Package>"
        }
    }
    $bundleManifest = "<Bundle><Packages><Package Type=`"application`" Architecture=`"$architecture`" FileName=`"app.msix`" />{0}</Packages></Bundle>"
    if ($case -eq 'wrong-architecture') { $bundleManifest = $bundleManifest.Replace($architecture, 'unknown') }
    $resource = if ($case -eq 'split-resources') { '<Package Type="resource" Architecture="neutral" FileName="resources.msix" />' } else { '' }
    $bundlePath = Join-Path $caseRoot 'app.msixbundle'
    New-TestArchive $bundlePath @{ 'AppxMetadata/AppxBundleManifest.xml' = ($bundleManifest -f $resource); 'app.msix' = [IO.File]::ReadAllBytes($app) }
    New-TestArchive (Join-Path $caseRoot 'app.msixupload') @{ 'app.msixbundle' = [IO.File]::ReadAllBytes($bundlePath) }
    $destination = Join-Path $caseRoot 'download'
    if ($case -eq 'stale-source') { [IO.File]::WriteAllText((Join-Path $fixture 'src/fixture.cs'), '// changed source') }
    $failure = $null
    try { $export = & (Join-Path $scripts 'New-OfflinePackage.ps1') -PackageRoot $caseRoot -Platform $architecture -OutputDirectory $destination -AsDirectory }
    catch { $failure = $_ }
    if ($case -eq 'complete') {
        Check (!$failure -and $export -eq $destination -and (Test-Path (Join-Path $destination 'Start.cmd'))) 'CI export creates a ready-to-extract installer without rebuilding'
        Check ([Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $destination "App/$architecture/Notepads.exe"))) -ceq
            [Convert]::ToBase64String($image)) 'CI export preserves the native application payload bytes'
        $duplicate = $null
        try { & (Join-Path $scripts 'New-OfflinePackage.ps1') -PackageRoot $caseRoot -Platform $architecture -OutputDirectory $destination -AsDirectory }
        catch { $duplicate = $_ }
        Check ($duplicate -and $duplicate.ToString() -like '*already exists*') 'An existing offline output is never mixed with a later build'
    }
    elseif ($case -eq 'stale-source') { Check ($failure -and $failure.ToString() -like '*do not match the current product sources*') 'Stale CI builds cannot be relabeled with current source metadata' }
    else { Check ($failure -and $failure.ToString() -like '*disable auto resource splitting*') "CI export rejects $case before producing an incomplete installer" }
}
$checks | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'checks.json') -Encoding utf8
Write-Output "PASS: Offline distribution contracts ($($checks.Count) checks); evidence: $EvidenceRoot"
