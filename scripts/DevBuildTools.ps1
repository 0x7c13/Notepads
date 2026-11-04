# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Shared by Run-Dev.ps1 and its inert boundary tests. Windows PowerShell 5.1 compatible.

function Get-DevHostPlatform {
    $architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    switch ($architecture) {
        'X64' { return 'x64' }
        'Arm64' { return 'ARM64' }
        default { throw 'Building Notepads requires a 64-bit Windows x64 or ARM64 host.' }
    }
}

function ConvertTo-DevProcessArgument([string] $Value) {
    # Start-Process joins its argument array; quote for Windows argv, including
    # backslashes immediately before a quote or the closing quote.
    return '"' + ([regex]::Replace($Value, '(\\*)"', '$1$1\"') -replace '(\\+)$', '$1$1') + '"'
}

function Invoke-DevElevatedProcess([string] $Executable, [string[]] $Arguments) {
    $commandLine = ($Arguments | ForEach-Object { ConvertTo-DevProcessArgument $_ }) -join ' '
    $process = Start-Process -FilePath $Executable -ArgumentList $commandLine -Verb RunAs -WindowStyle Hidden -Wait -PassThru
    try {
        if ($process.ExitCode -in @(1641, 3010)) { throw 'Setup requires a Windows restart. Restart, then run Run-Dev.ps1 again.' }
        if ($process.ExitCode -ne 0) { throw "Setup failed with exit code $($process.ExitCode)." }
    } finally { $process.Dispose() }
}

function Save-DevDownload([string] $Uri, [string] $Path) {
    $address = $null
    if (![uri]::TryCreate($Uri, [UriKind]::Absolute, [ref]$address) -or $address.Scheme -ne 'https') {
        throw 'Tool downloads require an absolute HTTPS address.'
    }
    $null = New-Item -ItemType Directory -Path (Split-Path $Path -Parent) -Force
    Write-Host "Downloading $Uri"
    # Windows PowerShell's per-chunk progress rendering substantially slows SDK ZIP downloads.
    $ProgressPreference = 'SilentlyContinue'
    Invoke-WebRequest -UseBasicParsing -Uri $Uri -OutFile $Path
}

function Expand-DevArchive([string] $Archive, [string] $Destination) {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $root = [IO.Path]::GetFullPath($Destination).TrimEnd('\') + '\'
    $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        foreach ($entry in $zip.Entries) {
            if ([IO.Path]::IsPathRooted($entry.FullName) -or $entry.FullName.Contains(':')) { throw 'Archive entry escapes its destination.' }
            $target = [IO.Path]::GetFullPath([IO.Path]::Combine($root, $entry.FullName))
            if (!$target.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) { throw 'Archive entry escapes its destination.' }
        }
    } finally { $zip.Dispose() }
    [IO.Compression.ZipFile]::ExtractToDirectory($Archive, $Destination)
}

function Get-DevSdkDownload($Metadata, [version] $Required, [string] $Platform) {
    $rid = 'win-' + $Platform.ToLowerInvariant()
    $sdks = @($Metadata.releases | ForEach-Object { $_.sdks; $_.sdk })
    $sdk = $sdks | Where-Object {
        $_.version -match '^\d+\.\d+\.\d+$' -and
        ([version]$_.version).Major -eq $Required.Major -and ([version]$_.version).Minor -eq $Required.Minor -and
        [math]::Floor(([version]$_.version).Build / 100) -eq [math]::Floor($Required.Build / 100) -and
        [version]$_.version -ge $Required
    } | Sort-Object { [version]$_.version } -Descending | Select-Object -First 1
    $file = @($sdk.files | Where-Object { $_.rid -eq $rid -and $_.url -match '\.zip$' })
    if ($file.Count -ne 1 -or $file[0].hash -notmatch '^[0-9a-fA-F]{128}$') {
        throw "No verified $rid SDK download in the required $Required feature band."
    }
    return [pscustomobject]@{ Version = $sdk.version; Url = $file[0].url; Hash = $file[0].hash }
}

function Get-DevBinaryPlatform([string] $Path) {
    $stream = [IO.File]::OpenRead($Path)
    $reader = [IO.BinaryReader]::new($stream)
    try {
        if ($reader.ReadUInt16() -ne 0x5a4d) { throw 'Not a Windows executable.' }
        $stream.Position = 60
        $stream.Position = $reader.ReadUInt32()
        if ($reader.ReadUInt32() -ne 0x4550) { throw 'Missing PE signature.' }
        switch ($reader.ReadUInt16()) {
            0xaa64 { return 'ARM64' }
            0x8664 { return 'x64' }
            0x014c { return 'x86' }
            default { throw 'Unsupported executable architecture.' }
        }
    } finally { $reader.Dispose() }
}

function Find-DevDotnet([string] $Repo, [string] $ToolsRoot) {
    $hostPlatform = Get-DevHostPlatform
    $command = Get-Command dotnet.exe -ErrorAction SilentlyContinue
    $candidates = @()
    if (Test-Path -LiteralPath $ToolsRoot) {
        $candidates += @(Get-ChildItem -LiteralPath $ToolsRoot -Directory -Filter 'dotnet-*' |
            ForEach-Object { Join-Path $_.FullName 'dotnet.exe' })
    }
    if ($command) { $candidates += $command.Source }
    $candidates += Join-Path $env:ProgramFiles 'dotnet/dotnet.exe'
    foreach ($candidate in $candidates | Select-Object -Unique) {
        if (!(Test-Path -LiteralPath $candidate -PathType Leaf)) { continue }
        Push-Location -LiteralPath $Repo
        try {
            # MSBuild and the AOT compiler must use the native host SDK, even when
            # an emulated dotnet.exe happens to be first on PATH.
            if ((Get-DevBinaryPlatform $candidate) -ne $hostPlatform) { continue }
            $version = & $candidate --version 2>&1
            if ($LASTEXITCODE -eq 0 -and ($version -join '').Trim() -match '^\d+\.\d+\.\d+$') {
                return [pscustomobject]@{ Path = $candidate; Version = ($version -join '').Trim() }
            }
        } catch { Write-Verbose "SDK at $candidate is not usable: $_" }
        finally { Pop-Location }
    }
    return $null
}

function Install-DevDotnet([string] $Repo, [string] $ToolsRoot, [string] $Platform) {
    $required = (Get-Content -LiteralPath (Join-Path $Repo 'global.json') -Raw | ConvertFrom-Json).sdk
    if ($required.rollForward -ne 'latestPatch' -or $required.allowPrerelease) {
        throw 'The automatic SDK installer expects the repository stable/latestPatch global.json policy.'
    }
    $version = [version]$required.version
    $metadata = Invoke-RestMethod -Uri "https://builds.dotnet.microsoft.com/dotnet/release-metadata/$($version.Major).$($version.Minor)/releases.json"
    $download = Get-DevSdkDownload $metadata $version $Platform
    $archive = Join-Path $ToolsRoot "dotnet-$($download.Version)-$Platform.zip"
    Save-DevDownload $download.Url $archive
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA512).Hash -ine $download.Hash) { throw '.NET SDK SHA-512 verification failed.' }
    $destination = Join-Path $ToolsRoot ("dotnet-$($download.Version)-$Platform-" + [Guid]::NewGuid().ToString('N'))
    Expand-DevArchive $archive $destination
    return Find-DevDotnet $Repo $ToolsRoot
}

function Get-DevBuildComponents([string] $Platform) {
    @('Microsoft.VisualStudio.Workload.UniversalBuildTools',
        'Microsoft.VisualStudio.ComponentGroup.UWP.VC.BuildTools',
        'Microsoft.VisualStudio.Component.VC.Tools.x86.x64',
        'Microsoft.VisualStudio.Component.Windows11SDK.26100')
    if ($Platform -eq 'ARM64') {
        'Microsoft.VisualStudio.Component.VC.Tools.ARM64'
        'Microsoft.VisualStudio.Component.UWP.VC.ARM64'
    }
}

function Get-DevVSInstances {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (!(Test-Path -LiteralPath $vswhere -PathType Leaf)) { return @() }
    $json = & $vswhere -all -products '*' -version '[18.0,19.0)' -format json
    if ($LASTEXITCODE -ne 0) { throw 'Visual Studio Installer discovery failed.' }
    return @($json -join "`n" | ConvertFrom-Json)
}

function Find-DevMSBuild([string] $Repo, [string] $Platform, [string] $HostPlatform) {
    foreach ($instance in Get-DevVSInstances | Where-Object { $_.isComplete -and !$_.isRebootRequired } |
        Sort-Object { $_.productId -ne 'Microsoft.VisualStudio.Product.BuildTools' }) {
        $root = $instance.installationPath
        $hostFolder = if ($HostPlatform -eq 'ARM64') { 'arm64/' } else { '' }
        $msbuild = Join-Path $root "MSBuild/Current/Bin/${hostFolder}MSBuild.exe"
        if (!(Test-Path -LiteralPath $msbuild -PathType Leaf)) { continue }
        $nativePlatform = if ($Platform -eq 'x86') { 'Win32' } else { $Platform }
        try {
            $output = & $msbuild (Join-Path $Repo 'src/Notepads.Native/WinUIEditor/WinUIEditor.vcxproj') /nologo /v:q /p:Configuration=Release "/p:Platform=$nativePlatform" /getProperty:PlatformToolset,VCToolsInstallDir,VCTargetsPath,WindowsSdkDir,WindowsTargetPlatformVersion 2>&1
            if ($LASTEXITCODE -ne 0) { throw ($output -join "`n") }
            $properties = ($output -join "`n" | ConvertFrom-Json).Properties
            $installationRoot = [IO.Path]::GetFullPath($root).TrimEnd('\') + '\'
            foreach ($directory in @($properties.VCToolsInstallDir, $properties.VCTargetsPath)) {
                if (!$directory -or ![IO.Path]::GetFullPath($directory).StartsWith($installationRoot, [StringComparison]::OrdinalIgnoreCase)) {
                    throw 'Native targets and compiler must belong to the selected installation.'
                }
            }
            if ($properties.PlatformToolset -ne 'v145') { throw 'The v145 toolset is required.' }
            $compilerHost = $HostPlatform.ToLowerInvariant()
            $target = $Platform.ToLowerInvariant()
            $sdk = $properties.WindowsTargetPlatformVersion
            $required = @(
                (Join-Path $root 'MSBuild/Microsoft/WindowsXaml/v18.0/Microsoft.Windows.UI.Xaml.CSharp.targets'),
                (Join-Path $root 'MSBuild/Microsoft/VisualStudio/v18.0/AppxPackage/Microsoft.AppXPackage.Targets'),
                (Join-Path $properties.VCToolsInstallDir "bin/Host$compilerHost/$target/cl.exe"),
                (Join-Path $properties.VCToolsInstallDir "lib/$target/uwp/vccorlib.lib"),
                (Join-Path $properties.WindowsSdkDir "Include/$sdk/um/Windows.h"),
                (Join-Path $properties.WindowsSdkDir "Lib/$sdk/um/$target/kernel32.lib"))
            foreach ($file in $required) { if (!(Test-Path -LiteralPath $file -PathType Leaf)) { throw "Missing $file" } }
            return [pscustomobject]@{ Path = $msbuild; Installation = $root; Version = $instance.installationVersion }
        } catch { Write-Host "Incomplete build tools at ${root}: $_" }
    }
    return $null
}

function Install-DevBuildTools([string] $ToolsRoot, [string] $Platform) {
    $bootstrapper = Join-Path $ToolsRoot 'vs_BuildTools.exe'
    Save-DevDownload 'https://aka.ms/vs/18/stable/vs_BuildTools.exe' $bootstrapper
    $signature = Get-AuthenticodeSignature -LiteralPath $bootstrapper
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch '(^|,\s*)O=Microsoft Corporation(,|$)') {
        throw 'Build Tools bootstrapper does not have a valid Microsoft signature.'
    }
    $existing = @(Get-DevVSInstances | Where-Object productId -eq 'Microsoft.VisualStudio.Product.BuildTools')
    if ($existing.Count -and $existing[0].isRebootRequired) { throw 'Build Tools requires a Windows restart. Restart, then run Run-Dev.ps1 again.' }
    $arguments = @('--quiet', '--wait', '--norestart')
    if ($existing.Count) { $arguments = @('modify', '--installPath', $existing[0].installationPath) + $arguments }
    foreach ($component in Get-DevBuildComponents $Platform) { $arguments += @('--add', $component) }
    Write-Host 'Installing Build Tools 2026. Windows may request administrator approval; no Visual Studio IDE is installed.'
    Write-Host 'Microsoft terms: https://visualstudio.microsoft.com/license-terms/vs2026-ga-diagnostic-buildtools/'
    Invoke-DevElevatedProcess $bootstrapper $arguments
}

function Read-DevPackageManifest([string] $Package) {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($Package)
    try {
        $entry = $zip.GetEntry('AppxManifest.xml')
        if (!$entry) { throw "Missing package manifest: $Package" }
        $reader = [IO.StreamReader]::new($entry.Open())
        try { return [xml]$reader.ReadToEnd() } finally { $reader.Dispose() }
    } finally { $zip.Dispose() }
}

function Assert-DevManifest([xml] $Manifest, [string] $Platform,
    [ValidateSet('Notepads-Dev', '19282JackieLiu.Notepads-Beta')][string] $Name = 'Notepads-Dev') {
    $identity = $Manifest.Package.Identity
    $apps = @($Manifest.Package.Applications.Application)
    if ($identity.Name -ne $Name -or $identity.Publisher -ne 'CN=40E66D07-5A3A-4954-9CA3-A1EB15ED0804' -or
        $identity.ProcessorArchitecture -ine $Platform -or $apps.Count -ne 1 -or $apps[0].Id -ne 'App' -or
        $apps[0].Executable -ne 'Notepads.exe' -or $apps[0].EntryPoint -ne 'Notepads.App') {
        throw "Only the matching $Name application may be deployed."
    }
}

function Test-DevRegistrationEnabled {
    $path = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock'
    $settings = Get-ItemProperty -LiteralPath $path -ErrorAction SilentlyContinue
    return $settings -and $settings.AllowDevelopmentWithoutDevLicense -eq 1
}

function Enable-DevRegistration {
    if (Test-DevRegistrationEnabled) { return }
    Write-Host 'Windows Developer Mode is disabled. Requesting administrator approval to enable development package registration.'
    $command = @'
$ErrorActionPreference = 'Stop'
try {
    $path = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock'
    $null = New-Item -Path $path -Force
    $null = New-ItemProperty -Path $path -Name AllowDevelopmentWithoutDevLicense -Value 1 -PropertyType DWord -Force
} catch { Write-Error $_; exit 1 }
'@
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
    Invoke-DevElevatedProcess (Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe') @('-NoProfile', '-NonInteractive', '-EncodedCommand', $encoded)
    if (!(Test-DevRegistrationEnabled)) {
        throw 'Windows development package registration is still disabled.'
    }
}

function Find-DevFrameworkPackage($Dependency, [string] $PackageRoot, [string] $Platform) {
    foreach ($file in Get-ChildItem -LiteralPath $PackageRoot -Recurse -File -Filter '*.appx') {
        $identity = (Read-DevPackageManifest $file.FullName).Package.Identity
        if ($identity.Name -eq $Dependency.Name -and $identity.Publisher -eq $Dependency.Publisher -and
            $identity.ProcessorArchitecture -in @($Platform, 'neutral') -and
            [version]$identity.Version -ge [version]$Dependency.MinVersion) { return $file.FullName }
    }
    throw "Missing framework package $($Dependency.Name). See $PackageRoot."
}

function Register-DevLayout([string] $Layout, [string] $PackageRoot, [string] $Platform,
    [ValidateSet('Notepads-Dev', '19282JackieLiu.Notepads-Beta')][string] $Name = 'Notepads-Dev') {
    $manifestPath = Join-Path $Layout 'AppxManifest.xml'
    [xml]$manifest = Get-Content -LiteralPath $manifestPath -Raw
    Assert-DevManifest $manifest $Platform $Name
    if (!(Test-Path -LiteralPath (Join-Path $Layout 'Notepads.exe') -PathType Leaf)) { throw 'The development layout is missing Notepads.exe.' }
    $dependencyPaths = @()
    foreach ($dependency in $manifest.Package.Dependencies.PackageDependency) {
        $installed = @(Get-AppxPackage -Name $dependency.Name | Where-Object {
            $_.Publisher -eq $dependency.Publisher -and $_.Architecture -in @($Platform, 'Neutral') -and
            [version]$_.Version -ge [version]$dependency.MinVersion
        })
        if ($installed.Count) { continue }
        $dependencyPaths += Find-DevFrameworkPackage $dependency $PackageRoot $Platform
    }
    Enable-DevRegistration
    # Never uninstall a package, force-close an app, or reset its application data.
    # Packaged, signed frameworks use normal deployment. DependencyPath on a
    # Register call requires DisableDevelopmentMode, which is incompatible with
    # our loose development layout. AppX rejects unrelated sibling frameworks:
    # only supply packages that this framework actually declares as dependencies.
    foreach ($path in $dependencyPaths) {
        $required = @((Read-DevPackageManifest $path).Package.Dependencies.PackageDependency | ForEach-Object Name)
        $siblings = @($dependencyPaths | Where-Object {
            $_ -ne $path -and (Read-DevPackageManifest $_).Package.Identity.Name -in $required
        })
        if ($siblings.Count) { Add-AppxPackage -Path $path -DependencyPath $siblings }
        else { Add-AppxPackage -Path $path }
    }
    Add-AppxPackage -Register $manifestPath
    $registered = Get-AppxPackage -Name $Name | Where-Object {
        $_.Publisher -eq $manifest.Package.Identity.Publisher -and $_.Architecture -ieq $Platform -and
        [IO.Path]::GetFullPath($_.InstallLocation).TrimEnd('\') -ieq [IO.Path]::GetFullPath($Layout).TrimEnd('\')
    }
    if (@($registered).Count -ne 1) { throw 'The development registration does not match the built layout.' }
    return $registered
}
