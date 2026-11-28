# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Exercise the actual native target's shell/argument boundary without a native build or VS installation.
# The child PATH is empty: the documented Windows PowerShell entry must not depend on PowerShell 7.
param([string] $EvidenceRoot = '', [string] $NativeProjectPath = '')
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
if (!$NativeProjectPath) { $NativeProjectPath = Join-Path $repo 'src/Notepads.Markdown/windows/Notepads.Markdown/Notepads.Markdown.vcxproj' }
if (!$EvidenceRoot) { $EvidenceRoot = Join-Path $repo ('artifacts/markdown-build-contracts/' + [Guid]::NewGuid().ToString('N')) }
$root = [IO.Path]::GetFullPath((Join-Path $EvidenceRoot 'shell fixture with spaces'))
[IO.Directory]::CreateDirectory((Join-Path $root 'module/scripts')) | Out-Null
$stub = @'
param([string] $Target, [string] $TargetDirectory, [string] $StagedLibrary,
    [string] $NativeOutput, [string] $TrackedHeader, [string] $VsInstallRoot, [string] $Diagnostics)
$ErrorActionPreference = 'Stop'
$env:PSModulePath = $env:NOTEPADS_MARKDOWN_TEST_MODULE_PATH
__UTILITY_IMPORT__
if ($PSVersionTable.PSVersion.Major -ne 5 -or $PSVersionTable.PSVersion.Minor -lt 1) {
    throw 'The source build must support the bundled Windows PowerShell 5.1.'
}
$hashInput = $NativeOutput + '.input'
[IO.File]::WriteAllBytes($hashInput, [Text.Encoding]::ASCII.GetBytes('abc'))
if ((Get-FileHash -LiteralPath $hashInput -Algorithm SHA256).Hash -cne
    'BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD') {
    throw 'The native build requires this engine''s Utility module for hashing and JSON receipts.'
}
[pscustomobject]@{ Target = $Target; TargetDirectory = $TargetDirectory; StagedLibrary = $StagedLibrary;
    NativeOutput = $NativeOutput; TrackedHeader = $TrackedHeader; VsInstallRoot = $VsInstallRoot; Diagnostics = $Diagnostics;
    ShellVersion = $PSVersionTable.PSVersion.ToString() } |
    ConvertTo-Json | Set-Content -LiteralPath ($NativeOutput + '.json') -Encoding utf8
'@
$buildScriptPath = Join-Path $repo 'src/Notepads.Markdown/scripts/Build-Rust.ps1'
$parseTokens = $null
$parseErrors = $null
$buildAst = [Management.Automation.Language.Parser]::ParseFile($buildScriptPath, [ref]$parseTokens, [ref]$parseErrors)
$utilityImports = @($buildAst.FindAll({ param($node)
    $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'Import-Module' -and
        $node.Extent.Text.Contains('Microsoft.PowerShell.Utility.psd1')
}, $false))
if ($parseErrors.Count -or $utilityImports.Count -ne 1) { throw 'Expected one engine-local Utility module initialization in the native build script.' }
$stub = $stub.Replace('__UTILITY_IMPORT__', $utilityImports[0].Extent.Text)
[IO.File]::WriteAllText((Join-Path $root 'module/scripts/Build-Rust.ps1'), $stub, [Text.UTF8Encoding]::new($true))
[xml]$project = Get-Content -LiteralPath $NativeProjectPath -Raw
$targets = @($project.Project.Target | Where-Object Name -eq 'BuildNotepadsMarkdownRust')
if ($targets.Count -ne 1 -or @($targets[0].Exec).Count -ne 1) { throw 'Expected one native Rust build command.' }
$checks = [Collections.Generic.List[string]]::new()
. (Join-Path $repo 'src/Notepads.Markdown/scripts/RustToolchain.ps1')
foreach ($case in @(
    @{ Triple = 'aarch64-pc-windows-msvc'; Architecture = 'arm64' },
    @{ Triple = 'x86_64-pc-windows-msvc'; Architecture = 'x64' },
    @{ Triple = 'i686-pc-windows-msvc'; Architecture = 'x64' }
)) {
    $architecture = Get-MarkdownRustDevShellArchitecture @('cargo probe', ('host: ' + $case.Triple), 'release: probe')
    if ($architecture -cne $case.Architecture) { throw "Wrong desktop tools for Cargo host $($case.Triple)." }
}
$checks.Add('The Rust host selects native desktop tools independently of the Markdown library target')
foreach ($diagnostics in @('false', 'true')) {
    $properties = [ordered]@{
        SystemRoot = $env:SystemRoot
        NotepadsMarkdownRoot = (Join-Path $root 'module') + '\'
        NotepadsMarkdownRustTarget = 'x86_64-pc-windows-msvc'
        NotepadsMarkdownRustTargetDir = (Join-Path $root 'Cargo output') + '\'
        NotepadsMarkdownRustLibrary = Join-Path $root 'staged library.lib'
        OutDir = $root + '\'
        TargetName = 'native-' + $diagnostics
        GeneratedFilesDir = (Join-Path $root 'Generated Files') + '\'
        VsInstallRoot = Join-Path $root 'Selected Visual Studio'
        NotepadsMarkdownEnableDiagnostics = $diagnostics
    }
    $command = [string]$targets[0].Exec.Command
    foreach ($name in $properties.Keys) { $command = $command.Replace('$(' + $name + ')', $properties[$name]) }
    if ($command.Contains('$(')) { throw 'The native Rust command contains an unbound property.' }
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $env:ComSpec
    $start.Arguments = '/d /s /c "' + $command + '"'
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.EnvironmentVariables['PATH'] = Join-Path $root 'empty-path'
    $start.EnvironmentVariables['NOTEPADS_MARKDOWN_TEST_MODULE_PATH'] = Join-Path $root 'foreign PS7 modules'
    $process = [Diagnostics.Process]::Start($start)
    try {
        $output = $process.StandardOutput.ReadToEndAsync()
        $errorOutput = $process.StandardError.ReadToEndAsync()
        if (!$process.WaitForExit(30000)) { $process.Kill(); $process.WaitForExit(); throw 'Native Rust shell fixture timed out.' }
        $log = $output.GetAwaiter().GetResult() + $errorOutput.GetAwaiter().GetResult()
        [IO.File]::WriteAllText((Join-Path $root ('shell-' + $diagnostics + '.log')), $log)
        if ($process.ExitCode -ne 0) { throw "Native Rust command failed without PowerShell 7: $log" }
    } finally { $process.Dispose() }
    $receipt = Get-Content -LiteralPath (Join-Path $root ('native-' + $diagnostics + '.dll.json')) -Raw | ConvertFrom-Json
    if ($receipt.Target -cne $properties.NotepadsMarkdownRustTarget -or
        $receipt.TargetDirectory -cne ($properties.NotepadsMarkdownRustTargetDir + '.') -or
        $receipt.StagedLibrary -cne $properties.NotepadsMarkdownRustLibrary -or
        $receipt.NativeOutput -cne ($properties.OutDir + $properties.TargetName + '.dll') -or
        $receipt.TrackedHeader -cne ($properties.GeneratedFilesDir + 'winrt\Notepads.Markdown.h') -or
        $receipt.VsInstallRoot -cne ($properties.VsInstallRoot + '\.') -or
        $receipt.Diagnostics -cne $diagnostics) { throw 'The native Rust shell boundary changed its quoted arguments.' }
    $checks.Add('Windows PowerShell 5.1 launches the actual native command, initializes Utility despite foreign module paths, and preserves exact spaced arguments; diagnostics=' + $diagnostics)
}
$brokerProbe = @'
param([string] $Root, [string] $BuildTools)
$ErrorActionPreference = 'Stop'
. $BuildTools
$source = @"
using System;
using System.IO;
public static class BrokerBuildFixture {
    public static int Main(string[] args) {
        File.WriteAllLines(Environment.GetEnvironmentVariable("NOTEPADS_MSBUILD_FIXTURE_ARGS"), args);
        foreach (string argument in args) {
            if (!argument.StartsWith("/p:OutDir=", StringComparison.Ordinal)) continue;
            string directory = argument.Substring(10);
            Directory.CreateDirectory(directory);
            File.WriteAllText(Path.Combine(directory, "Notepads.DocumentAssets.exe"), "inert fixture");
        }
        Console.Error.WriteLine("inert MSBuild diagnostic");
        return Environment.GetEnvironmentVariable("NOTEPADS_MSBUILD_FIXTURE_FAIL") == "1" ? 17 : 0;
    }
}
"@
$fakeBuild = Join-Path $Root 'fixture-msbuild.exe'
if (!(Test-Path -LiteralPath $fakeBuild)) { Add-Type -TypeDefinition $source -OutputAssembly $fakeBuild -OutputType ConsoleApplication }
$env:NOTEPADS_MSBUILD_FIXTURE_ARGS = Join-Path $Root 'broker-build-arguments.txt'
$checkout = Join-Path $Root 'checkout with spaces'
$output = Join-Path $Root 'broker output with spaces'
$executable = Build-DevDocumentAssetBroker $fakeBuild $checkout ARM64 $output
$arguments = [IO.File]::ReadAllLines($env:NOTEPADS_MSBUILD_FIXTURE_ARGS)
$outputArguments = @($arguments | Where-Object { $_.StartsWith('/p:OutDir=', [StringComparison]::Ordinal) })
if ($arguments.Length -ne 7 -or $arguments[0] -cne (Join-Path $checkout 'src/Notepads.DocumentAssets/Notepads.DocumentAssets.vcxproj') -or
    '/p:Platform=ARM64' -cnotin $arguments -or $outputArguments.Count -ne 1 -or
    [IO.Path]::GetFullPath($outputArguments[0].Substring(10)).TrimEnd('\') -cne [IO.Path]::GetFullPath($output) -or
    $executable -cne (Join-Path $output 'Notepads.DocumentAssets.exe')) { throw 'PS5 changed a spaced broker build argument.' }
if (!(Select-String -LiteralPath (Join-Path $output 'desktop-bridge.log') -SimpleMatch 'inert MSBuild diagnostic')) {
    throw 'The successful broker build lost its stderr diagnostic.'
}
$env:NOTEPADS_MSBUILD_FIXTURE_FAIL = '1'
try {
    $null = Build-DevDocumentAssetBroker $fakeBuild $checkout ARM64 $output
    throw 'A nonzero native exit passed the broker build.'
}
catch {
    if ($_.Exception.Message -notlike 'Desktop bridge build failed (17).*') { throw }
}
$global:LASTEXITCODE = 0
try {
    $null = Invoke-DevNativeLogged (Join-Path $Root 'missing-msbuild.exe') @() (Join-Path $Root 'missing-msbuild.log')
    throw 'A stale native exit code passed a launch failure.'
}
catch {
    if ($_.FullyQualifiedErrorId -ne 'CommandNotFoundException' -and
        $_.Exception.Message -notlike 'No native exit code from *') { throw }
}
if ($global:LASTEXITCODE -ne 0) { throw 'The native helper changed the caller exit code.' }
try {
    $null = Invoke-DevNativeLogged (Join-Path $env:SystemRoot 'System32/cmd.exe') @('/d', '/c', 'exit 0') $output
    throw 'Logging to a directory passed the native invocation.'
}
catch {
    if ($_.InvocationInfo.MyCommand.Name -ne 'Out-File') { throw }
}
'@
$brokerProbePath = Join-Path $root 'broker-build-probe.ps1'
[IO.File]::WriteAllText($brokerProbePath, $brokerProbe, [Text.UTF8Encoding]::new($true))
& (Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe') -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File $brokerProbePath -Root $root -BuildTools (Join-Path $repo 'scripts/DevBuildTools.ps1') *> (Join-Path $root 'broker-build-probe.log')
if ($LASTEXITCODE -ne 0) { throw "PS5 broker build boundary failed; see $root/broker-build-probe.log" }
$checks.Add('Windows PowerShell 5.1 preserves spaced broker build argv, logs stderr and rejects nonzero exits, launch failures and log failures')
& {
    # Evaluate the real props at their SDK import point without restoring packages
    # or referencing the native project. Defaults and explicit contexts must agree.
    $managedProject = Join-Path $root 'ManagedProbe.csproj'
    $directoryProps = Join-Path $repo 'src/Notepads.Markdown/Directory.Build.props'
    $managedFixture = @'
<Project>
  <PropertyGroup>
    <DirectoryBuildPropsPath>__DIRECTORY_PROPS__</DirectoryBuildPropsPath>
    <TargetFramework>net10.0</TargetFramework>
    <EnableDefaultItems>false</EnableDefaultItems>
  </PropertyGroup>
  <Import Project="Sdk.props" Sdk="Microsoft.NET.Sdk" />
  <Import Project="Sdk.targets" Sdk="Microsoft.NET.Sdk" />
</Project>
'@
    [IO.File]::WriteAllText($managedProject, $managedFixture.Replace('__DIRECTORY_PROPS__', [Security.SecurityElement]::Escape($directoryProps)))
    $buildRoot = Join-Path $root 'managed output'
    $propertyNames = 'Configuration,Platform,NotepadsMarkdownVariant,NotepadsMarkdownNativePlatform,BaseIntermediateOutputPath,MSBuildProjectExtensionsPath,IntermediateOutputPath,BaseOutputPath,OutputPath'
    foreach ($case in @(
        @{ Name = 'defaults'; Configuration = 'Debug'; Platform = 'AnyCPU'; Variant = 'production'; Properties = @() },
        @{ Name = 'implicit-debug'; Configuration = 'Debug'; Platform = 'x64'; Variant = 'production'; Properties = @('/p:Platform=x64') },
        @{ Name = 'explicit-debug'; Configuration = 'Debug'; Platform = 'x64'; Variant = 'production'; Properties = @('/p:Configuration=Debug', '/p:Platform=x64') },
        @{ Name = 'project-defaults'; Configuration = 'Release'; Platform = 'ARM64'; Variant = 'production'; Properties = @('/p:DefaultProjectConfiguration=Release', '/p:DefaultProjectPlatform=ARM64') },
        @{ Name = 'diagnostics'; Configuration = 'Release'; Platform = 'ARM64'; Variant = 'diagnostics'; Properties = @('/p:Configuration=Release', '/p:Platform=ARM64', '/p:NotepadsMarkdownEnableDiagnostics=true') },
        @{ Name = 'production-x86'; Configuration = 'Production'; Platform = 'x86'; Variant = 'production'; Properties = @('/p:Configuration=Production', '/p:Platform=x86') }
    )) {
        $propertyLog = Join-Path $root ('managed-properties-' + $case.Name + '.json')
        & dotnet msbuild $managedProject /nologo "/getProperty:$propertyNames" "/p:NotepadsMarkdownBuildRoot=$buildRoot\" @($case.Properties) *> $propertyLog
        if ($LASTEXITCODE -ne 0) { throw "Managed Markdown property evaluation failed: $propertyLog" }
        $evaluated = (Get-Content -LiteralPath $propertyLog -Raw | ConvertFrom-Json).Properties
        $projectRoot = Join-Path $buildRoot ('managed/' + $case.Variant + '/ManagedProbe')
        $obj = [IO.Path]::GetFullPath((Join-Path $projectRoot 'obj')) + '\'
        $bin = [IO.Path]::GetFullPath((Join-Path $projectRoot 'bin')) + '\'
        $context = $case.Platform + '\' + $case.Configuration + '\'
        $sdkContext = if ($case.Platform -eq 'AnyCPU') { $case.Configuration + '\' } else { $context }
        $nativePlatform = switch ($case.Platform) { 'x86' { 'Win32' } 'AnyCPU' { '' } default { $case.Platform } }
        if ($evaluated.Configuration -cne $case.Configuration -or $evaluated.Platform -cne $case.Platform -or
            $evaluated.NotepadsMarkdownVariant -cne $case.Variant -or
            $evaluated.NotepadsMarkdownNativePlatform -cne $nativePlatform -or
            $evaluated.BaseIntermediateOutputPath -ine $obj -or $evaluated.BaseOutputPath -ine $bin -or
            $evaluated.MSBuildProjectExtensionsPath -ine ($obj + $context) -or
            $evaluated.IntermediateOutputPath -ine ($obj + $sdkContext + 'net10.0\') -or
            $evaluated.OutputPath -ine ($bin + $sdkContext + 'net10.0\')) {
            throw "Managed Markdown paths lost context isolation or duplicated SDK suffixes: $propertyLog"
        }
        $checks.Add('Managed Markdown property evaluation preserves variant/configuration/platform isolation: ' + $case.Name)
    }
    $nativeProbe = Join-Path $root 'NativeProbe.vcxproj'
    [IO.File]::WriteAllText($nativeProbe, '<Project><Import Project="' + [Security.SecurityElement]::Escape($directoryProps) + '" /></Project>')
    $nativeLog = Join-Path $root 'native-properties.json'
    & dotnet msbuild $nativeProbe /nologo /getProperty:BaseIntermediateOutputPath,MSBuildProjectExtensionsPath,BaseOutputPath *> $nativeLog
    if ($LASTEXITCODE -ne 0) { throw "Native Markdown property evaluation failed: $nativeLog" }
    $nativeProperties = (Get-Content -LiteralPath $nativeLog -Raw | ConvertFrom-Json).Properties
    if ($nativeProperties.BaseIntermediateOutputPath -or $nativeProperties.MSBuildProjectExtensionsPath -or $nativeProperties.BaseOutputPath) {
        throw 'Managed Markdown output paths leaked into a native project.'
    }
    $checks.Add('Managed Markdown output paths do not override native project defaults')
}
$consumer = Join-Path $repo 'src/Notepads.Markdown/build/Notepads.Markdown.Consumer.targets'
$guardProject = Join-Path $root 'consumer-guard.proj'
[IO.File]::WriteAllText($guardProject, ('<Project><Import Project="' + [Security.SecurityElement]::Escape($consumer) + '" /></Project>'))
foreach ($case in @(
    @{ Configuration = 'Production'; Diagnostics = 'false'; Reject = $false },
    @{ Configuration = 'Release'; Diagnostics = 'true'; Reject = $false },
    @{ Configuration = 'Production'; Diagnostics = 'true'; Reject = $true }
)) {
    $guardLog = Join-Path $root ('consumer-' + $case.Configuration + '-' + $case.Diagnostics + '.log')
    & dotnet msbuild $guardProject /nologo /t:ValidateNotepadsMarkdownConsumerConfiguration `
        "/p:Configuration=$($case.Configuration)" "/p:NotepadsMarkdownEnableDiagnostics=$($case.Diagnostics)" *> $guardLog
    if (($LASTEXITCODE -ne 0) -ne $case.Reject -or
        ($case.Reject -and [IO.File]::ReadAllText($guardLog) -notlike '*Production packages cannot enable Notepads.Markdown diagnostics*')) {
        throw "The Markdown consumer configuration guard changed: $guardLog"
    }
    $checks.Add('Consumer diagnostics guard: ' + $case.Configuration + '/' + $case.Diagnostics)
}
# Run the real Rust test entry and the real native staging tail with an inert
# Cargo boundary. No Rust compiler, package installation or user clipboard is invoked.
& {
    $policyRoot = Join-Path $root 'rust entry policy'
    [IO.Directory]::CreateDirectory((Join-Path $policyRoot 'scripts')) | Out-Null
    [IO.File]::WriteAllText((Join-Path $policyRoot 'rust-toolchain.toml'), "[toolchain]" + [Environment]::NewLine + 'channel = "9.8.7"')
    $testEntry = Join-Path $policyRoot 'scripts/Test-Rust.ps1'
    Copy-Item -LiteralPath (Join-Path $repo 'src/Notepads.Markdown/scripts/Test-Rust.ps1') -Destination $testEntry
    $rustHelper = Join-Path $repo 'src/Notepads.Markdown/scripts/RustToolchain.ps1'
    Copy-Item -LiteralPath $rustHelper -Destination (Join-Path $policyRoot 'scripts/RustToolchain.ps1')
    . $rustHelper
    $buildText = [IO.File]::ReadAllText($buildScriptPath)
    $policyStatements = @($buildAst.FindAll({ param($node)
        $node -is [Management.Automation.Language.AssignmentStatementAst] -and
            $node.Left -is [Management.Automation.Language.VariableExpressionAst] -and
            $node.Left.VariablePath.UserPath -in @('rustPolicy', 'channel')
    }, $false))
    $plainPath = @($buildAst.FindAll({ param($node)
        $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Assert-PlainPath'
    }, $false))
    $tailStart = $buildText.IndexOf('$previousTarget = $env:CARGO_TARGET_DIR', [StringComparison]::Ordinal)
    if ($policyStatements.Count -ne 2 -or $plainPath.Count -ne 1 -or $tailStart -lt 0) {
        throw 'The native Rust policy/staging fixture no longer identifies the real entry statements.'
    }
    $nativeEntry = [scriptblock]::Create(($policyStatements[0].Extent.Text, $policyStatements[1].Extent.Text, $plainPath[0].Extent.Text, $buildText.Substring($tailStart)) -join [Environment]::NewLine)
    $invocations = [Collections.Generic.List[object]]::new()
    $fixtureCargo = {
        $invocations.Add([pscustomobject]@{ Channel = $env:RUSTUP_TOOLCHAIN; TargetDirectory = $env:CARGO_TARGET_DIR; Arguments = @($args) })
        $global:LASTEXITCODE = if ($fixtureMode -eq 'failure') { 1 } else { 0 }
        if ($fixtureMode -eq 'success' -and $args[0] -eq 'rustc') {
            $source = Join-Path $env:CARGO_TARGET_DIR "$Target/release/markdown_ffi.lib"
            [IO.Directory]::CreateDirectory((Split-Path -Parent $source)) | Out-Null
            [IO.File]::WriteAllBytes($source, [Text.Encoding]::ASCII.GetBytes('inert Rust static library'))
        }
    }
    function Get-Command {
        param([string] $Name, [string] $ErrorAction)
        if ($Name -ne 'cargo.exe') { throw 'Unexpected command discovery in Rust test fixture.' }
        [pscustomobject]@{ Source = $fixtureCargo }
    }
    $priorToolchain = $env:RUSTUP_TOOLCHAIN
    $priorTarget = $env:CARGO_TARGET_DIR
    try {
        foreach ($entry in @('test', 'native')) {
            foreach ($fixtureMode in @('success', 'failure')) {
                $env:RUSTUP_TOOLCHAIN = 'ambient-contract-override'
                $env:CARGO_TARGET_DIR = 'ambient-contract-target'
                $before = $invocations.Count
                $failure = $null
                $TargetDirectory = Join-Path $policyRoot ($entry + '-' + $fixtureMode)
                try {
                    if ($entry -eq 'test') { & $testEntry -Layer Svg -TargetDirectory $TargetDirectory | Out-Null }
                    else {
                        $moduleRoot = $policyRoot
                        $Target = 'x86_64-pc-windows-msvc'
                        $Diagnostics = 'false'
                        $StagedLibrary = Join-Path $TargetDirectory 'staged/markdown_ffi.lib'
                        $NativeOutput = Join-Path $TargetDirectory 'Notepads.Markdown.dll'
                        $cargoPath = $fixtureCargo
                        & $nativeEntry | Out-Null
                    }
                } catch { $failure = $_ }
                if (($fixtureMode -eq 'failure') -ne [bool]$failure -or $invocations.Count -ne $before + 1 -or
                    $invocations[$before].Channel -cne '9.8.7' -or
                    [IO.Path]::GetFullPath($invocations[$before].TargetDirectory) -cne [IO.Path]::GetFullPath($TargetDirectory) -or
                    $env:RUSTUP_TOOLCHAIN -cne 'ambient-contract-override' -or $env:CARGO_TARGET_DIR -cne 'ambient-contract-target') {
                    throw "Rust entry did not pin/restore its checked-in channel and Cargo directory: $entry/$fixtureMode/$failure"
                }
                $checks.Add('Actual Rust entry pins the policy channel and restores inherited overrides: ' + $entry + '/' + $fixtureMode)
            }
        }
    } finally { $env:RUSTUP_TOOLCHAIN = $priorToolchain; $env:CARGO_TARGET_DIR = $priorTarget }
}
& {
    $clipboardSource = [IO.File]::ReadAllText((Join-Path $repo 'scripts/MarkdownClipboardLease.cs'))
    $declaration = [regex]::Match($clipboardSource, '(?m)^\s*public void RestoreAfterTestCopy\([^\r\n]+\)')
    if (!$declaration.Success) { throw 'Clipboard recovery API declaration is missing.' }
    # Compile only the actual declaration into a C# 5 facade. PS5's Add-Type
    # compiler cannot consume the physical driver's newer C# implementation.
    if (!('MarkdownClipboardRecoveryApiFixture' -as [type])) {
        $api = 'public sealed class MarkdownClipboardRecoveryApiFixture { ' + $declaration.Value.Trim() + ' {} }'
        Add-Type -TypeDefinition $api
    }
    $parameters = [MarkdownClipboardRecoveryApiFixture].GetMethod('RestoreAfterTestCopy').GetParameters()
    if ($parameters.Count -ne 3 -or $parameters[1].ParameterType -ne [uint32] -or $parameters[2].ParameterType -ne [bool]) {
        throw 'Clipboard recovery must accept the retained isolated process ID and liveness without a Process type conversion.'
    }
    $clipboardTokens = $null
    $clipboardErrors = $null
    $clipboardAst = [Management.Automation.Language.Parser]::ParseFile(
        (Join-Path $repo 'scripts/Test-Markdown.ps1'), [ref]$clipboardTokens, [ref]$clipboardErrors)
    $copyCleanup = @($clipboardAst.FindAll({ param($node)
        $node -is [Management.Automation.Language.IfStatementAst] -and
            $node.Extent.Text.StartsWith('if ($copyAttempted)', [StringComparison]::Ordinal)
    }, $false))
    if ($clipboardErrors.Count -or $copyCleanup.Count -ne 1) { throw 'Expected one physical Markdown copy cleanup branch.' }
    $clipboard = [pscustomobject]@{ Receipt = $null }
    $clipboard | Add-Member -MemberType ScriptMethod -Name RestoreAfterTestCopy -Value {
        param([string] $Text, [uint32] $ProcessId, [bool] $Alive)
        $this.Receipt = [pscustomobject]@{ Text = $Text; ProcessId = $ProcessId; Alive = $Alive }
    }
    $copyAttempted = $true
    $ownedClipboardText = 'inert verified test text'
    $session = @{ ProcessIdentity = @{ Process = [pscustomobject]@{ Id = [uint32]42; HasExited = $false } } }
    & ([scriptblock]::Create($copyCleanup[0].Extent.Text))
    if ($clipboard.Receipt.Text -cne $ownedClipboardText -or $clipboard.Receipt.ProcessId -ne 42 -or !$clipboard.Receipt.Alive) {
        throw 'The physical Markdown failure cleanup changed its retained process arguments.'
    }
    $checks.Add('Actual physical Markdown failure cleanup binds the clipboard API without touching the clipboard')
}

$checks | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'checks.json') -Encoding utf8
Write-Output "PASS: Markdown build shell contracts ($($checks.Count) checks); private evidence: $root"
