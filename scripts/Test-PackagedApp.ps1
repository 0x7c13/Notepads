# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Installs the actual Production bundle on a disposable hosted VM. Never use a
# developer's registered app/data as a fixture. No identity or payload rewriting.
#requires -Version 7.4
param([Parameter(Mandatory)][string] $PackagePath)
$ErrorActionPreference = 'Stop'
if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'Packaged app tests require a disposable GitHub-hosted runner.'
}
if (![Environment]::UserInteractive -or (Get-Process -Id $PID).SessionId -eq 0 -or
    [Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') {
    throw 'Packaged app tests require an interactive x64 Windows desktop.'
}
$repo = Split-Path $PSScriptRoot -Parent
$sourcePackage = (Resolve-Path -LiteralPath $PackagePath).Path
if ([IO.Path]::GetExtension($sourcePackage) -notin '.msixupload', '.appxupload') {
    throw 'Supply the Production upload package from Build-CI.ps1.'
}
$root = Join-Path $repo ('artifacts/packaged-app/' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $root
$report = [ordered]@{
    Status = 'RUNNING'; Source = $sourcePackage
    SourceSha256 = (Get-FileHash -LiteralPath $sourcePackage -Algorithm SHA256).Hash
    Package = $null; Cases = [Collections.Generic.List[object]]::new()
    Error = $null; CleanupErrors = [Collections.Generic.List[string]]::new()
}
$certificate = $null
$trusted = $null
$installed = $null
$driver = $null
$registeredByTest = $false
Start-Transcript -LiteralPath (Join-Path $root 'driver.log') | Out-Null

function Read-ZipXml($Archive, [string] $Name) {
    $entry = $Archive.GetEntry($Name)
    if (!$entry) { throw "Missing archive XML: $Name" }
    $reader = [IO.StreamReader]::new($entry.Open())
    try { return [xml]$reader.ReadToEnd() } finally { $reader.Dispose() }
}

function Get-EntryHash($Entry) {
    $stream = $Entry.Open()
    try { return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)) }
    finally { $stream.Dispose() }
}

try {
    & (Join-Path $PSScriptRoot 'Test-AppPackage.ps1') -Path $sourcePackage -ExpectedArchitectures x64
    $archive = [IO.Compression.ZipFile]::OpenRead($sourcePackage)
    try {
        $bundles = @($archive.Entries | Where-Object Name -Match '\.(msixbundle|appxbundle)$')
        if ($bundles.Count -ne 1) { throw 'Expected exactly one bundle in the upload package.' }
        $bundle = Join-Path $root 'installed.msixbundle'
        [IO.Compression.ZipFileExtensions]::ExtractToFile($bundles[0], $bundle)
    }
    finally { $archive.Dispose() }

    $archive = [IO.Compression.ZipFile]::OpenRead($bundle)
    try {
        $bundleManifest = Read-ZipXml $archive 'AppxMetadata/AppxBundleManifest.xml'
        $payloads = @($bundleManifest.Bundle.Packages.Package | Where-Object { $_.Type -eq 'application' -and $_.Architecture -eq 'x64' })
        if ($payloads.Count -ne 1) { throw 'Expected one x64 application payload.' }
        $payloadName = $payloads[0].FileName
        $payloadHash = Get-EntryHash $archive.GetEntry($payloadName)
        $payload = [IO.Compression.ZipArchive]::new($archive.GetEntry($payloadName).Open(), [IO.Compression.ZipArchiveMode]::Read)
        try {
            $manifest = Read-ZipXml $payload 'AppxManifest.xml'
            $exeHash = Get-EntryHash $payload.GetEntry('Notepads.exe')
        }
        finally { $payload.Dispose() }
    }
    finally { $archive.Dispose() }
    [xml]$sourceManifest = Get-Content -LiteralPath (Join-Path $repo 'src/Notepads/Package.appxmanifest')
    $identity = $manifest.Package.Identity
    if ($identity.Name -ne $sourceManifest.Package.Identity.Name -or
        $identity.Publisher -ne $sourceManifest.Package.Identity.Publisher -or
        @($manifest.Package.Applications.Application).Count -ne 1) {
        throw 'The test requires the unchanged Production identity and application manifest.'
    }
    if (@(Get-AppxPackage -AllUsers -Name $identity.Name).Count -or @(Get-Process Notepads -ErrorAction SilentlyContinue).Count) {
        throw 'A pre-existing Notepads package/process must never be used or removed by this test.'
    }

    # Trust a short-lived public certificate only on this disposable VM. The
    # private key stays in its certificate store and is never uploaded as evidence.
    $certificate = New-SelfSignedCertificate -Type CodeSigningCert -Subject $identity.Publisher `
        -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddDays(2)
    $publicCertificate = Join-Path $root 'test-signing.cer'
    $null = Export-Certificate -Cert $certificate -FilePath $publicCertificate
    $trusted = Import-Certificate -FilePath $publicCertificate -CertStoreLocation Cert:\LocalMachine\TrustedPeople
    $sdkBin = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin/10.0.26100.0/x64'
    & (Join-Path $sdkBin 'signtool.exe') sign /fd SHA256 /sha1 $certificate.Thumbprint /s My $bundle
    if ($LASTEXITCODE -ne 0) { throw "Bundle signing failed: $LASTEXITCODE" }
    & (Join-Path $sdkBin 'signtool.exe') verify /pa $bundle
    if ($LASTEXITCODE -ne 0) { throw "Bundle signature verification failed: $LASTEXITCODE" }
    $archive = [IO.Compression.ZipFile]::OpenRead($bundle)
    try {
        if ((Get-EntryHash $archive.GetEntry($payloadName)) -ne $payloadHash) { throw 'Signing changed the application payload.' }
    }
    finally { $archive.Dispose() }
    Add-AppxPackage -Path $bundle
    $registeredByTest = $true
    $registrations = @(Get-AppxPackage -Name $identity.Name)
    if ($registrations.Count -ne 1) { throw 'Expected one installed Production package.' }
    $installed = $registrations[0]
    if ($installed.IsDevelopmentMode -or $installed.Publisher -ne $identity.Publisher -or
        (Get-FileHash -LiteralPath (Join-Path $installed.InstallLocation 'Notepads.exe')).Hash -ne $exeHash) {
        throw 'The installed executable is not the original packaged Production executable.'
    }
    $report.Package = $installed.PackageFullName
    $report.ExecutableSha256 = $exeHash
    $localState = Join-Path $env:LOCALAPPDATA "Packages/$($installed.PackageFamilyName)/LocalState"
    $null = New-Item -ItemType Directory -Path $localState -Force
    Add-Type -Path (Join-Path $PSScriptRoot 'PackagedAppDriver.cs')
    Add-Type -Path (Join-Path $PSScriptRoot 'IsolatedTestProcess.cs')
    Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
    $appId = $installed.PackageFamilyName + '!' + $manifest.Package.Applications.Application.Id
    $driver = [PackagedAppDriver]::new($appId, $installed.PackageFullName)
    & (Join-Path $repo 'tests/IntegrationTests/PackagedApp/Scenarios.ps1') `
        -Driver $driver -Installed $installed -LocalState $localState -EvidenceRoot $root -Cases $report.Cases
    $report.Status = 'PASS'
}
catch {
    $report.Status = 'FAIL'
    $report.Error = $_.ToString() + "`n" + $_.ScriptStackTrace
    try {
        Add-Type -AssemblyName System.Windows.Forms, System.Drawing
        $bounds = [Windows.Forms.SystemInformation]::VirtualScreen
        $bitmap = [Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
        try {
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            try { $graphics.CopyFromScreen($bounds.Location, [Drawing.Point]::Empty, $bounds.Size) }
            finally { $graphics.Dispose() }
            $bitmap.Save((Join-Path $root 'failure.png'))
        }
        finally { $bitmap.Dispose() }
    }
    catch { Write-Warning "Screenshot unavailable: $_" }
    throw
}
finally {
    if ($driver) {
        try { $driver.Terminate() } catch { $report.CleanupErrors.Add("Terminate: $_") }
        $driver.Dispose()
    }
    if ($registeredByTest) {
        try {
            # The identity was absent before our install. Resolve it even when
            # validation failed before the installed object could be assigned.
            foreach ($package in @(Get-AppxPackage -Name $identity.Name)) { Remove-AppxPackage -Package $package.PackageFullName }
            if (@(Get-AppxPackage -Name $identity.Name).Count) { throw 'Test package remains registered.' }
        }
        catch { $report.CleanupErrors.Add("Uninstall: $_") }
    }
    foreach ($cert in @($trusted, $certificate)) {
        if ($cert) {
            try { Remove-Item -LiteralPath $cert.PSPath -Force }
            catch { $report.CleanupErrors.Add("Certificate: $_") }
        }
    }
    if ($report.CleanupErrors.Count) { $report.Status = 'FAIL' }
    $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $root 'results.json')
    Stop-Transcript | Out-Null
    Write-Host "Packaged application report: $root"
    if ($report.CleanupErrors.Count) { throw ($report.CleanupErrors -join "`n") }
}
