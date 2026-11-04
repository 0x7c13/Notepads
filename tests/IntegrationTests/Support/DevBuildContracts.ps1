# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Inert installation/registration boundaries: never download, elevate or register.
# This suite also runs directly in Windows PowerShell 5.1, like Run-Dev.ps1.
param([string] $EvidenceRoot = '')
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
. (Join-Path $repo 'scripts/DevBuildTools.ps1')
if (!$EvidenceRoot) { $EvidenceRoot = Join-Path $repo ('artifacts/dev-contracts/' + [Guid]::NewGuid().ToString('N')) }
$null = New-Item -ItemType Directory -Path $EvidenceRoot -Force
$checks = [Collections.Generic.List[string]]::new()
function Check([bool] $Condition, [string] $Name) {
    if (!$Condition) { throw $Name }
    $checks.Add($Name)
}
function Reject([string] $Name, [string] $Message, [scriptblock] $Action) {
    $failure = $null
    try { & $Action | Out-Null } catch { $failure = $_ }
    Check ($failure -and $failure.Exception.Message -like $Message) $Name
}
function New-Sdk([string] $Version, [string] $Rid = 'win-arm64', [string] $Hash = ('a' * 128)) {
    [pscustomobject]@{ version = $Version; files = @([pscustomobject]@{ rid = $Rid; url = "https://example.invalid/$Version.zip"; hash = $Hash }) }
}
$metadata = [pscustomobject]@{ releases = @([pscustomobject]@{ sdks = @(
    (New-Sdk '10.0.400'), (New-Sdk '10.0.401'), (New-Sdk '10.0.402'), (New-Sdk '10.0.403-preview.1'),
    (New-Sdk '10.0.500'), (New-Sdk '10.1.401')); sdk = $null }) }
$sdk = Get-DevSdkDownload $metadata ([version]'10.0.401') ARM64
Check ($sdk.Version -eq '10.0.402' -and $sdk.Hash -eq ('a' * 128)) 'SDK selection respects latestPatch and excludes previews, older patches and other feature bands'
Reject 'Missing SDK host architecture is rejected' '*No verified win-x64 SDK*' { Get-DevSdkDownload $metadata ([version]'10.0.401') x64 }
Reject 'No compatible SDK is rejected' '*No verified*' { Get-DevSdkDownload $metadata ([version]'11.0.100') ARM64 }
$invalid = [pscustomobject]@{ releases = @([pscustomobject]@{ sdk = New-Sdk '10.0.401' 'win-arm64' 'bad'; sdks = @() }) }
Reject 'Missing SDK integrity hash is rejected' '*No verified*' { Get-DevSdkDownload $invalid ([version]'10.0.401') ARM64 }
Reject 'HTTP download is rejected before network or file writes' '*absolute HTTPS*' { Save-DevDownload 'http://example.invalid/a' (Join-Path $EvidenceRoot 'blocked') }
Reject 'Relative download is rejected before network or file writes' '*absolute HTTPS*' { Save-DevDownload 'relative.zip' (Join-Path $EvidenceRoot 'blocked') }

Add-Type -AssemblyName System.IO.Compression.FileSystem
function New-Archive([string] $Path, [hashtable] $Entries) {
    $zip = [IO.Compression.ZipFile]::Open($Path, 'Create')
    try {
        foreach ($name in $Entries.Keys) {
            $writer = [IO.StreamWriter]::new($zip.CreateEntry($name).Open())
            try { $writer.Write($Entries[$name]) } finally { $writer.Dispose() }
        }
    } finally { $zip.Dispose() }
}
$archive = Join-Path $EvidenceRoot 'valid.zip'
New-Archive $archive @{ 'directory/file.txt' = 'verified' }
$expanded = Join-Path $EvidenceRoot 'expanded'
Expand-DevArchive $archive $expanded
Check ((Get-Content -LiteralPath (Join-Path $expanded 'directory/file.txt') -Raw) -eq 'verified') 'Valid tool archive extracts intact'
foreach ($entry in @('../escaped.txt', '..\escaped.txt', 'C:/escaped.txt')) {
    $path = Join-Path $EvidenceRoot ([Guid]::NewGuid().ToString('N') + '.zip')
    New-Archive $path @{ $entry = 'blocked' }
    Reject "Archive traversal is rejected: $entry" '*escapes its destination*' { Expand-DevArchive $path (Join-Path $EvidenceRoot 'blocked-extraction') }
}
Check (!(Test-Path -LiteralPath (Join-Path $EvidenceRoot 'escaped.txt'))) 'Unsafe archive never writes outside its destination'

& {
    $sdkRoot = Join-Path $EvidenceRoot 'sdk'
    $fixtureArchive = $archive
    $null = New-Item -ItemType Directory -Path $sdkRoot
    [IO.File]::WriteAllText((Join-Path $sdkRoot 'global.json'), '{"sdk":{"version":"10.0.401","rollForward":"latestPatch","allowPrerelease":false}}')
    $sdkHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA512).Hash
    $response = [pscustomobject]@{ releases = @([pscustomobject]@{ sdk = New-Sdk '10.0.401' 'win-arm64' ('0' * 128) }) }
    function Invoke-RestMethod { param($Uri); return $response }
    function Save-DevDownload { param($Uri, $Path); Copy-Item -LiteralPath $fixtureArchive -Destination $Path }
    function Find-DevDotnet { return 'verified local SDK' }
    Reject 'SDK integrity failure prevents extraction' '*SHA-512 verification failed*' { Install-DevDotnet $sdkRoot $sdkRoot ARM64 }
    Check (@(Get-ChildItem -LiteralPath $sdkRoot -Directory).Count -eq 0) 'A corrupt SDK never creates an executable tool directory'
    $response.releases[0].sdk.files[0].hash = $sdkHash
    Check ((Install-DevDotnet $sdkRoot $sdkRoot ARM64) -eq 'verified local SDK') 'Hash-verified SDK is extracted and rediscovered'
    $files = @(Get-ChildItem -LiteralPath $sdkRoot -Recurse -File -Filter file.txt)
    Check ($files.Count -eq 1 -and (Get-Content -LiteralPath $files[0].FullName -Raw) -eq 'verified') 'SDK installation preserves the verified archive contents'
}

# Architecture is read from the PE header, independent of the host's emulation.
foreach ($machine in @(@(0xaa64, 'ARM64'), @(0x8664, 'x64'), @(0x014c, 'x86'))) {
    $path = Join-Path $EvidenceRoot ($machine[1] + '.exe')
    $writer = [IO.BinaryWriter]::new([IO.File]::Create($path))
    try {
        $writer.Write([uint16]0x5a4d)
        $writer.BaseStream.Position = 60
        $writer.Write([uint32]64)
        $writer.Write([uint32]0x4550)
        $writer.Write([uint16]$machine[0])
    } finally { $writer.Dispose() }
    Check ((Get-DevBinaryPlatform $path) -eq $machine[1]) "PE architecture detection: $($machine[1])"
}

# Compare the quoting helper against a real native argv parser, including an empty
# argument and a quoted executable path. The child only serializes its arguments.
$nativeSource = @'
using System;
using System.IO;
public class DevArgvFixture {
    public static void Main(string[] args) {
        using (var writer = new BinaryWriter(File.Create(args[0]))) {
            writer.Write(args.Length - 1);
            for (int i = 1; i < args.Length; i++) writer.Write(args[i]);
        }
    }
}
'@
$argvExe = Join-Path $EvidenceRoot 'argv fixture.exe'
# Framework csc produces a standalone Windows executable; no SDK/IDE required.
$source = Join-Path $EvidenceRoot 'argv.cs'
[IO.File]::WriteAllText($source, $nativeSource)
& (Join-Path $env:SystemRoot 'Microsoft.NET/Framework64/v4.0.30319/csc.exe') /nologo /target:exe "/out:$argvExe" $source
if ($LASTEXITCODE) { throw 'Cannot compile the inert argv fixture.' }
$argvOutput = Join-Path $EvidenceRoot 'argv.bin'
$arguments = @('', 'two words', 'plain', 'C:\path with spaces\', 'a"b', 'a\"b', 'quotes" and trailing\\')
$line = (@($argvOutput) + $arguments | ForEach-Object { ConvertTo-DevProcessArgument $_ }) -join ' '
$child = Start-Process -FilePath $argvExe -ArgumentList $line -WindowStyle Hidden -Wait -PassThru
try { Check ($child.ExitCode -eq 0) 'Quoted native argument fixture exits successfully' } finally { $child.Dispose() }
$reader = [IO.BinaryReader]::new([IO.File]::OpenRead($argvOutput))
try {
    Check ($reader.ReadInt32() -eq $arguments.Count) 'Quoted arguments preserve their count'
    foreach ($argument in $arguments) { Check ($reader.ReadString() -ceq $argument) "Native argv round-trip: [$argument]" }
} finally { $reader.Dispose() }

& {
    function Save-DevDownload { param($Uri, $Path); [IO.File]::WriteAllText($Path, 'inert setup') }
    function Get-AuthenticodeSignature { param($LiteralPath); return $signature }
    function Get-DevVSInstances { return $instances }
    function Invoke-DevElevatedProcess { param($Executable, $Arguments); $calls.Add(@{ Exe = $Executable; Arguments = $Arguments }) }
    $calls = [Collections.Generic.List[object]]::new()
    $instances = @()
    $signature = [pscustomobject]@{ Status = 'NotSigned'; SignerCertificate = $null }
    Reject 'Unsigned Build Tools setup never executes' '*valid Microsoft signature*' { Install-DevBuildTools $EvidenceRoot ARM64 }
    $signature = [pscustomobject]@{ Status = 'Valid'; SignerCertificate = [pscustomobject]@{ Subject = 'CN=Vendor, O=Other Company' } }
    Reject 'Non-Microsoft setup never executes' '*valid Microsoft signature*' { Install-DevBuildTools $EvidenceRoot ARM64 }
    Check ($calls.Count -eq 0) 'Rejected installers never reach the elevation boundary'
    $signature.SignerCertificate.Subject = 'CN=Microsoft Corporation, O=Microsoft Corporation, C=US'
    Install-DevBuildTools $EvidenceRoot ARM64
    $plan = $calls[0].Arguments
    Check ($plan -contains 'Microsoft.VisualStudio.Workload.UniversalBuildTools' -and
        $plan -contains 'Microsoft.VisualStudio.ComponentGroup.UWP.VC.BuildTools' -and
        $plan -contains 'Microsoft.VisualStudio.Component.UWP.VC.ARM64' -and
        $plan -contains 'Microsoft.VisualStudio.Component.Windows11SDK.26100' -and
        $plan -notcontains 'modify') 'New Build Tools setup requests managed/native UWP, ARM64 and pinned Windows SDK components'
    $instances = @([pscustomobject]@{ productId = 'Microsoft.VisualStudio.Product.Community'; installationPath = 'IDE must stay untouched' })
    Install-DevBuildTools $EvidenceRoot x64
    Check ($calls[1].Arguments -notcontains 'modify') 'Missing Build Tools never modifies an installed IDE'
    $instances += [pscustomobject]@{ productId = 'Microsoft.VisualStudio.Product.BuildTools'; installationPath = 'Existing Build Tools' }
    Install-DevBuildTools $EvidenceRoot x64
    Check ($calls[2].Arguments[0] -eq 'modify' -and $calls[2].Arguments[2] -eq 'Existing Build Tools') 'Existing Build Tools are augmented in place'
    $instances[1] | Add-Member NoteProperty isRebootRequired $true
    Reject 'Pending Build Tools restart prevents another setup' '*requires a Windows restart*' { Install-DevBuildTools $EvidenceRoot x64 }
    Check ($calls.Count -eq 3) 'Pending restart never reaches the setup execution boundary'
}

$publisher = 'CN=40E66D07-5A3A-4954-9CA3-A1EB15ED0804'
& {
    $mode = @{ Value = $null; Elevations = 0; EnableSucceeds = $true }
    function Get-ItemProperty { param($LiteralPath, $ErrorAction); return $mode.Value }
    function Invoke-DevElevatedProcess {
        param($Executable, $Arguments)
        $mode.Elevations++
        if ($mode.EnableSucceeds) { $mode.Value = [pscustomobject]@{ AllowDevelopmentWithoutDevLicense = 1 } }
    }
    Check (!(Test-DevRegistrationEnabled)) 'Missing Developer Mode registry value is detected'
    $mode.Value = [pscustomobject]@{ AllowDevelopmentWithoutDevLicense = 0 }
    Check (!(Test-DevRegistrationEnabled)) 'Disabled Developer Mode is detected'
    Enable-DevRegistration
    Check ((Test-DevRegistrationEnabled) -and $mode.Elevations -eq 1) 'Disabled Developer Mode requests elevation and rechecks the setting'
    Enable-DevRegistration
    Check ($mode.Elevations -eq 1) 'Enabled Developer Mode does not request elevation again'
    $mode.Value.AllowDevelopmentWithoutDevLicense = 0
    $mode.EnableSucceeds = $false
    Reject 'Blocked Developer Mode fails before registration' '*registration is still disabled*' { Enable-DevRegistration }
}
$appManifest = @"
<Package><Identity Name="Notepads-Dev" Publisher="$publisher" Version="2.0.0.0" ProcessorArchitecture="arm64" />
<Dependencies><PackageDependency Name="Framework.Test" Publisher="CN=Microsoft" MinVersion="1.0.0.0" /></Dependencies>
<Applications><Application Id="App" Executable="Notepads.exe" EntryPoint="Notepads.App" /></Applications></Package>
"@
Assert-DevManifest ([xml]$appManifest) ARM64
foreach ($mutation in @(
    $appManifest.Replace('Notepads-Dev', 'Notepads'), $appManifest.Replace($publisher, 'CN=Unexpected'),
    $appManifest.Replace('arm64', 'x64'), $appManifest.Replace('Notepads.exe', 'Unexpected.exe'))) {
    Reject 'Foreign identity/architecture/entry point is refused' '*Only the matching Notepads-Dev*' { Assert-DevManifest ([xml]$mutation) ARM64 }
}
& {
    $layout = Join-Path $EvidenceRoot 'layout'
    $packages = Join-Path $EvidenceRoot 'packages'
    $null = New-Item -ItemType Directory -Path $layout, $packages -Force
    [IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $appManifest)
    [IO.File]::WriteAllText((Join-Path $layout 'Notepads.exe'), 'inert executable')
    foreach ($framework in @(
        @{ File = 'wrong-publisher'; Publisher = 'CN=Other'; Arch = 'arm64'; Version = '9.0.0.0' },
        @{ File = 'wrong-arch'; Publisher = 'CN=Microsoft'; Arch = 'x64'; Version = '9.0.0.0' },
        @{ File = 'too-old'; Publisher = 'CN=Microsoft'; Arch = 'arm64'; Version = '0.9.0.0' },
        @{ File = 'matching'; Publisher = 'CN=Microsoft'; Arch = 'arm64'; Version = '1.0.0.0' })) {
        $xml = '<Package><Identity Name="Framework.Test" Publisher="{0}" ProcessorArchitecture="{1}" Version="{2}" /></Package>' -f $framework.Publisher, $framework.Arch, $framework.Version
        New-Archive (Join-Path $packages ($framework.File + '.appx')) @{ 'AppxManifest.xml' = $xml }
    }
    $calls = [Collections.Generic.List[object]]::new()
    $state = @{ Enabled = $false; Registered = $false; Installed = $false }
    function Enable-DevRegistration { $state.Enabled = $true }
    function Add-AppxPackage {
        param($Register, $Path, [string[]] $DependencyPath = @())
        if ($Register -and $DependencyPath.Count) { throw 'Dependency packages cannot accompany loose development registration.' }
        if ($Path) {
            $declared = @((Read-DevPackageManifest $Path).Package.Dependencies.PackageDependency | ForEach-Object Name)
            foreach ($dependency in $DependencyPath) {
                if ((Read-DevPackageManifest $dependency).Package.Identity.Name -notin $declared) {
                    throw 'Windows rejects an unrelated framework supplied through DependencyPath.'
                }
            }
        }
        if ($Register) { $state.Registered = $true }
        $calls.Add(@{ Manifest = $Register; Path = $Path; Dependencies = @($DependencyPath) })
    }
    function Get-AppxPackage {
        param($Name)
        if ($Name -eq 'Framework.Test' -and $state.Installed) {
            return [pscustomobject]@{ Publisher = 'CN=Microsoft'; Architecture = 'Arm64'; Version = '1.0.0.0' }
        }
        if ($Name -eq 'Notepads-Dev' -and $state.Registered) {
            return [pscustomobject]@{ Publisher = $publisher; Architecture = 'Arm64'; InstallLocation = $layout; PackageFamilyName = 'fixture' }
        }
    }
    $null = Register-DevLayout $layout $packages ARM64
    Check ($calls.Count -eq 2 -and $calls[0].Path -eq (Join-Path $packages 'matching.appx') -and
        !$calls[0].Manifest -and $calls[1].Manifest -eq (Join-Path $layout 'AppxManifest.xml') -and
        $calls[1].Dependencies.Count -eq 0) 'Only the matching framework is deployed before separate loose application registration'
    $state.Installed = $true
    $null = Register-DevLayout $layout $packages ARM64
    Check ($calls.Count -eq 3 -and !$calls[2].Path -and $calls[2].Dependencies.Count -eq 0) 'An installed compatible framework is reused'
    $state.Installed = $false
    $second = Join-Path $packages 'second.appx'
    New-Archive $second @{ 'AppxManifest.xml' = '<Package><Identity Name="Framework.Second" Publisher="CN=Microsoft" ProcessorArchitecture="arm64" Version="1.0.0.0" /></Package>' }
    [IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'),
        $appManifest.Replace('</Dependencies>', '<PackageDependency Name="Framework.Second" Publisher="CN=Microsoft" MinVersion="1.0.0.0" /></Dependencies>'))
    $null = Register-DevLayout $layout $packages ARM64
    Check ($calls.Count -eq 6 -and $calls[3].Path -eq (Join-Path $packages 'matching.appx') -and
        $calls[3].Dependencies.Count -eq 0 -and $calls[4].Path -eq $second -and $calls[4].Dependencies.Count -eq 0 -and
        $calls[5].Manifest -and $calls[5].Dependencies.Count -eq 0) 'Independent frameworks are installed without unrelated sibling packages'
    $dependent = '<Package><Identity Name="Framework.Test" Publisher="CN=Microsoft" ProcessorArchitecture="arm64" Version="1.0.0.0" /><Dependencies><PackageDependency Name="Framework.Second" Publisher="CN=Microsoft" MinVersion="1.0.0.0" /></Dependencies></Package>'
    Remove-Item -LiteralPath (Join-Path $packages 'matching.appx')
    New-Archive (Join-Path $packages 'matching.appx') @{ 'AppxManifest.xml' = $dependent }
    $null = Register-DevLayout $layout $packages ARM64
    Check ($calls.Count -eq 9 -and $calls[6].Dependencies.Count -eq 1 -and $calls[6].Dependencies[0] -eq $second -and
        $calls[7].Dependencies.Count -eq 0 -and $calls[8].Manifest) 'A declared framework dependency is supplied without creating a reverse dependency'
    [IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $appManifest)
    $empty = Join-Path $EvidenceRoot 'no-frameworks'
    $null = New-Item -ItemType Directory -Path $empty
    $state.Enabled = $false
    Reject 'Missing framework fails before system settings or registration change' '*Missing framework package*' { Register-DevLayout $layout $empty ARM64 }
    Check (!$state.Enabled -and $calls.Count -eq 9) 'Missing dependencies leave OS boundaries untouched'
}

# Execute the actual entry script against inert tool discovery. CheckOnly must
# never acquire a build lock, create artifacts, install tools or register an app.
& {
    $fixture = Join-Path $EvidenceRoot 'entry'
    $scripts = Join-Path $fixture 'scripts'
    $null = New-Item -ItemType Directory -Path $scripts -Force
    Copy-Item -LiteralPath (Join-Path $repo 'scripts/Run-Dev.ps1') -Destination $scripts
    [IO.File]::WriteAllText((Join-Path $fixture 'global.json'), '{"sdk":{"version":"10.0.401"}}')
    $stubs = @'
function Get-DevHostPlatform { 'ARM64' }
function Find-DevDotnet { if (!$env:NOTEPADS_CONTRACT_MISSING) { [pscustomobject]@{ Path = 'inert dotnet'; Version = '10.0.401' } } }
function Find-DevMSBuild { if (!$env:NOTEPADS_CONTRACT_MISSING) { [pscustomobject]@{ Path = 'inert MSBuild'; Version = '18.0' } } }
function Test-DevRegistrationEnabled { $false }
function Install-DevDotnet { throw 'UNEXPECTED installation' }
function Install-DevBuildTools { throw 'UNEXPECTED installation' }
'@
    [IO.File]::WriteAllText((Join-Path $scripts 'DevBuildTools.ps1'), $stubs)
    $oldPin = $env:MSBuildSDKsPath
    $oldMissing = $env:NOTEPADS_CONTRACT_MISSING
    try {
        $env:MSBuildSDKsPath = 'inherited SDK pin'
        $env:NOTEPADS_CONTRACT_MISSING = $null
        $result = & (Join-Path $scripts 'Run-Dev.ps1') -CheckOnly
        Check ($result -like 'READY:*') 'CheckOnly discovers a complete toolchain without invoking it'
        $env:NOTEPADS_CONTRACT_MISSING = '1'
        Reject 'CheckOnly does not install a missing toolchain' '*Toolchain is incomplete*' { & (Join-Path $scripts 'Run-Dev.ps1') -CheckOnly }
        Check ($env:MSBuildSDKsPath -eq 'inherited SDK pin') 'Entry script restores inherited SDK environment after success and failure'
        Check (!(Test-Path -LiteralPath (Join-Path $fixture '.tools')) -and !(Test-Path -LiteralPath (Join-Path $fixture 'artifacts'))) 'CheckOnly creates no tool or build directories'
        Reject 'NoInstall never invokes a missing tool installer' '*Toolchain is incomplete*' { & (Join-Path $scripts 'Run-Dev.ps1') -NoInstall -BuildOnly }
        $lockPath = Join-Path $fixture '.tools/notepads-dev/run.lock'
        $held = [IO.File]::Open($lockPath, 'Open', 'ReadWrite', 'None')
        try {
            Reject 'Concurrent invocation cannot take over the checkout' '*Another Run-Dev invocation*' { & (Join-Path $scripts 'Run-Dev.ps1') -NoInstall -BuildOnly }
        } finally { $held.Dispose() }
        Check (!(Test-Path -LiteralPath (Join-Path $fixture 'artifacts'))) 'Failed tool discovery never starts a build'
    } finally { $env:MSBuildSDKsPath = $oldPin; $env:NOTEPADS_CONTRACT_MISSING = $oldMissing }
}
$checks | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'checks.json') -Encoding utf8
Write-Output "PASS: Developer build contracts ($($checks.Count) checks); evidence: $EvidenceRoot"
