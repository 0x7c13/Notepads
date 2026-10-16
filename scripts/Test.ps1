# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

param(
    [ValidateSet('Unit', 'Integration', 'All')] [string] $Suite = 'Unit',
    [string[]] $Group = @(),
    [string] $Filter = '',
    [switch] $List,
    [switch] $NoBuild,
    [ValidateSet('Debug', 'Release')] [string] $Configuration = 'Release',
    [ValidateSet('x64', 'ARM64')] [string] $Platform = 'ARM64',
    [int] $TimeoutSeconds = 180,
    [string] $PackagePath = ''
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'TestBuildIdentity.ps1')
$repo = Split-Path $PSScriptRoot -Parent
$integration = Join-Path $repo 'tests/IntegrationTests/IntegrationTests.csproj'
$groups = [ordered]@{
    Runner = @{ Profile = 'Source'; Default = $true }
    Headers = @{ Profile = 'Source'; Default = $true }
    Storage = @{ Profile = 'Headless'; Arguments = @('--storage'); Default = $true }
    MetricsInitialization = @{ Profile = 'Headless'; Arguments = @('--metrics-initialization-failure'); Default = $true }
    MetricsCounter = @{ Profile = 'Headless'; Arguments = @('--metrics-counter-failure'); Default = $true }
    MetricsDuration = @{ Profile = 'Headless'; Arguments = @('--metrics-duration-failure'); Default = $true }
    ArchitectureSelfTest = @{ Profile = 'Headless'; Arguments = @('--architecture-self-test'); Default = $true }
    Architecture = @{ Profile = 'Product'; Default = $true }
    Localization = @{ Profile = 'Source'; Default = $true }
    NativeDiff = @{ Profile = 'NativeDiff'; Default = $true }
    NativeBidi = @{ Profile = 'NativeBidi'; Default = $true }
    RegexText = @{ Profile = 'RegexText'; Default = $true }
    Editor = @{ Profile = 'App'; Arguments = @('--editor'); Default = $true }
    Bidi = @{ Profile = 'App'; Arguments = @('--bidi'); Default = $true }
    BidiPerformance = @{ Profile = 'App'; Arguments = @('--bidi-performance'); Default = $false; Performance = $true }
    ReadingOrder = @{ Profile = 'App'; Arguments = @('--reading-order'); Default = $true }
    Documents = @{ Profile = 'App'; Arguments = @('--documents'); Default = $true }
    Activation = @{ Profile = 'App'; Arguments = @('--activation'); Default = $true }
    OpenPicker = @{ Profile = 'App'; Arguments = @('--open-picker'); Default = $true }
    Outbound = @{ Profile = 'App'; Arguments = @('--outbound'); Default = $true }
    Printing = @{ Profile = 'App'; Arguments = @('--printing'); Default = $true }
    EditingUI = @{ Profile = 'App'; Arguments = @('--editing-ui'); Default = $true }
    Clipboard = @{ Profile = 'App'; Arguments = @('--clipboard'); Default = $true }
    ClipboardRejectedCut = @{ Profile = 'App'; Arguments = @('--clipboard-rejected-cut'); Default = $true }
    WorkspaceCommands = @{ Profile = 'App'; Arguments = @('--workspace-commands'); Default = $true }
    PrintState = @{ Profile = 'App'; Arguments = @('--print-state'); Default = $true }
    Preferences = @{ Profile = 'App'; Arguments = @('--preferences'); Default = $true }
    PreferencesStore = @{ Profile = 'App'; Arguments = @('--preferences-store'); Default = $true }
    PreferencesCorruptEncoding = @{ Profile = 'App'; Arguments = @('--preferences-corrupt-encoding'); Default = $true }
    PreferencesRestart = @{ Profile = 'App'; Arguments = @('--preferences-seed', '--preferences-restart'); Default = $true }
    Appearance = @{ Profile = 'App'; Arguments = @('--appearance'); Default = $true }
    ZoomLanguage = @{ Profile = 'App'; Arguments = @('--zoom-language'); Default = $true }
    ViewModes = @{ Profile = 'App'; Arguments = @('--view-modes'); Default = $true }
    Workspace = @{ Profile = 'App'; Arguments = @('--workspace-seed', '--workspace', '--workspace-window-close', '--workspace-restart'); Default = $true }
    WorkspaceClean = @{ Profile = 'App'; Arguments = @('--workspace-clean-seed', '--workspace-clean-close', '--workspace-clean-restart'); Default = $true }
    WorkspaceImport = @{ Profile = 'App'; Arguments = @('--workspace-import-seed', '--workspace-import', '--workspace-import-restart'); Default = $true }
    WorkspaceFailedImport = @{ Profile = 'App'; Arguments = @('--workspace-import-seed', '--workspace-import-failed'); Default = $true }
    FileOpening = @{ Profile = 'App'; Arguments = @('--workspace-opening'); Default = $true }
    Leases = @{ Profile = 'App'; Lease = $true; Default = $true }
    Search = @{ Profile = 'App'; Arguments = @('--search'); Default = $false }
    NativeStorage = @{ Profile = 'App'; Arguments = @('--native-storage'); Default = $false }
    Persistence = @{ Profile = 'App'; Arguments = @('--persistence'); Default = $true }
    NativeFaults = @{ Profile = 'App'; Configuration = 'Debug'; Arguments = @('--native-faults'); Default = $true }
    SessionResilience = @{ Profile = 'App'; Arguments = @('--session-resilience'); Default = $false }
    Syntax = @{ Profile = 'App'; Arguments = @('--syntax'); Default = $false }
    Diff = @{ Profile = 'App'; Arguments = @('--diff'); Default = $false }
    ScrollRestoration = @{ Profile = 'App'; Arguments = @('--scroll-restoration'); Default = $true }
    AutomationNavigation = @{ Profile = 'App'; Arguments = @('--automation-navigation'); Default = $false }
    UIAutomation = @{ Profile = 'Desktop'; Default = $false }
    DpiLayout = @{ Profile = 'Desktop'; Default = $false }
    DisplayDpi = @{ Profile = 'Desktop'; Default = $false }
    DragDrop = @{ Profile = 'Desktop'; Default = $false }
    RegexMemory = @{ Profile = 'App'; Arguments = @('--regex-memory'); Default = $false; Performance = $true }
    Performance = @{ Profile = 'App'; Arguments = @('--performance'); Default = $false; Performance = $true }
    Package = @{ Profile = 'Package'; Default = $false }
    PackagedApp = @{ Profile = 'PackageDesktop'; Default = $false }
}

if ($List) {
    if ($Suite -ne 'Integration') { Write-Output 'Unit: discover/filter with dotnet test tests/UnitTests/UnitTests.csproj --list-tests or -Filter <VSTest expression>.' }
    if ($Suite -ne 'Unit') {
        foreach ($name in $groups.Keys) {
            [pscustomobject]@{ Group = $name; Profile = $groups[$name].Profile; Default = [bool]$groups[$name].Default;
                Configuration = $(if ($groups[$name].Configuration) { $groups[$name].Configuration } else { $Configuration });
                Performance = [bool]$groups[$name].Performance; Phases = ($groups[$name].Arguments -join ', ') }
        }
    }
    return
}
if ($Suite -eq 'Unit' -and $Group.Count) { throw 'Unit tests use -Filter, not integration -Group names.' }
if ($Suite -eq 'Integration' -and $Filter) { throw 'Integration tests use exact -Group names; -Filter is a Unit test expression.' }
if ($Group.Count -and @($Group | Where-Object { !$groups.Contains($_) }).Count) {
    throw ('Unknown integration group(s): ' + (@($Group | Where-Object { !$groups.Contains($_) }) -join ', '))
}
$selected = if ($Group.Count) { @($Group | Select-Object -Unique) } else { @($groups.Keys | Where-Object { $groups[$_].Default }) }
if ($Suite -ne 'Unit' -and !$selected.Count) { throw 'No integration groups selected.' }
if (('Package' -in $selected -or 'PackagedApp' -in $selected) -and !$PackagePath) { throw '-Group Package/PackagedApp requires -PackagePath pointing to the actual packaged product.' }
$reportRoot = Join-Path $repo ('artifacts/test-runs/' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $reportRoot -Force | Out-Null
$reports = [Collections.Generic.List[object]]::new()
$built = @{}

function Get-TestMSBuild {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $installation = & $vswhere -latest -products '*' -version '[18.0,19.0)' -property installationPath
    if (!$installation) { throw 'Visual Studio 2026 with UWP/native build tools is required.' }
    $hostDirectory = if ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString() -eq 'Arm64') { 'arm64/' } else { '' }
    $build = Join-Path $installation "MSBuild/Current/Bin/${hostDirectory}MSBuild.exe"
    if (!(Test-Path -LiteralPath $build)) { throw "MSBuild is missing: $build" }
    return $build
}

function Ensure-TestBuild([string] $Profile, [string] $BuildConfiguration = $Configuration) {
    $cacheKey = "$Profile/$BuildConfiguration"
    if ($built.ContainsKey($cacheKey)) { return }
    $buildPlatform = if ($Profile -in @('Unit', 'Headless')) { 'AnyCPU' } elseif ($Profile -in @('NativeDiff', 'NativeBidi', 'RegexText')) { 'x64' } else { $Platform }
    if ($NoBuild) { $null = Assert-TestBuildIdentity $Profile $BuildConfiguration $buildPlatform }
    else {
        $before = Get-TestSourceHashes $Profile
        $log = Join-Path $reportRoot "build-$Profile-$BuildConfiguration.log"
        Write-Host "BUILD $Profile ($BuildConfiguration, $buildPlatform)"
        Write-Host "  Log: $log"
        if ($Profile -eq 'Unit') {
            & dotnet build (Join-Path $repo 'tests/UnitTests/UnitTests.csproj') -c $BuildConfiguration --nologo *> $log
        }
        elseif ($Profile -eq 'Headless') {
            & dotnet build $integration -c $BuildConfiguration /p:IntegrationHost=Headless --nologo *> $log
        }
        elseif ($Profile -eq 'DragPeer') {
            & dotnet build (Join-Path $repo 'tests/IntegrationTests/Support/DragDrop.Peer/DragDrop.Peer.csproj') -c $BuildConfiguration -r ('win-' + $Platform.ToLowerInvariant()) --nologo *> $log
        }
        else {
            $msbuild = Get-TestMSBuild
            $solutionDirectory = [IO.Path]::GetFullPath((Join-Path $repo 'src/'))
            $buildArguments = @('/restore', "/p:Configuration=$BuildConfiguration", "/p:Platform=$buildPlatform", "/p:SolutionDir=$solutionDirectory", '/v:minimal', '/nologo')
            $project = if ($Profile -eq 'Product') { Join-Path $repo 'src/Notepads/Notepads.csproj' }
                elseif ($Profile -in @('NativeDiff', 'NativeBidi', 'RegexText')) { Join-Path $repo "tests/IntegrationTests/Support/$Profile.Tests/$Profile.Tests.vcxproj" }
                else { $integration }
            if ($Profile -eq 'App') {
                $buildArguments += @('/p:IntegrationHost=App', '/p:AppxBundle=Never', '/p:AppxPackageSigningEnabled=false', $(if ($BuildConfiguration -eq 'Debug') { '/t:Build' } else { '/t:Publish' }))
            }
            else { $buildArguments += '/t:Build' }
            $priorPath = $env:PATH
            try {
                $env:PATH = (Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer') + ';' + $priorPath
                & $msbuild $project @buildArguments *> $log
            }
            finally { $env:PATH = $priorPath }
        }
        if ($LASTEXITCODE -ne 0) { throw "Build failed ($LASTEXITCODE): $log" }
        $null = Write-TestBuildIdentity $Profile $BuildConfiguration $buildPlatform $before
    }
    $built[$cacheKey] = $true
}

function Invoke-ReportedTest([string] $Name, [scriptblock] $Action) {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $entry = [ordered]@{ Name = $Name; Status = 'RUNNING'; DurationMilliseconds = 0; Error = $null }
    $reports.Add($entry)
    try {
        Write-Host "RUN $Name"
        & $Action
        $entry.Status = 'PASS'
    }
    catch { $entry.Status = 'FAIL'; $entry.Error = $_.ToString(); throw }
    finally {
        $entry.DurationMilliseconds = $watch.ElapsedMilliseconds
        $reports | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $reportRoot 'results.json') -Encoding utf8
        Write-Output "$($entry.Status) $Name ($($entry.DurationMilliseconds) ms)"
    }
}

try {
    if ($Suite -ne 'Integration') {
        Invoke-ReportedTest 'Unit' {
            Ensure-TestBuild Unit
            $arguments = @('test', (Join-Path $repo 'tests/UnitTests/UnitTests.csproj'), '-c', $Configuration, '--no-build', '--no-restore',
                '--logger', 'trx;LogFileName=unit.trx', '--results-directory', $reportRoot)
            if ($Filter) { $arguments += @('--filter', $Filter) }
            $arguments += @('--', 'RunConfiguration.TreatNoTestsAsError=true')
            & dotnet @arguments
            if ($LASTEXITCODE -ne 0) { throw "Unit tests failed or selected no cases ($LASTEXITCODE)." }
            [xml] $trx = Get-Content -LiteralPath (Join-Path $reportRoot 'unit.trx')
            if ([int]$trx.TestRun.ResultSummary.Counters.executed -lt 1) { throw 'The unit selection executed no cases.' }
        }
    }
    if ($Suite -ne 'Unit') {
        foreach ($name in $selected) {
            Invoke-ReportedTest $name {
                $spec = $groups[$name]
                if ($spec.Profile -eq 'Headless') {
                    Ensure-TestBuild Headless
                    & dotnet (Join-Path $repo "tests/IntegrationTests/bin/Headless/$Configuration/net10.0/NotepadsIntegrationTests.Headless.dll") @($spec.Arguments)
                    if ($LASTEXITCODE -ne 0) { throw "Headless integration failed: $name" }
                }
                elseif ($name -eq 'Architecture') {
                    Ensure-TestBuild Headless
                    Ensure-TestBuild Product
                    $assembly = Join-Path $repo "src/Notepads/obj/$Platform/$Configuration/net10.0-windows10.0.26100.0/win-$($Platform.ToLowerInvariant())/Notepads.dll"
                    & dotnet (Join-Path $repo "tests/IntegrationTests/bin/Headless/$Configuration/net10.0/NotepadsIntegrationTests.Headless.dll") --architecture $assembly
                    if ($LASTEXITCODE -ne 0) { throw 'The actual compiled product violates architecture boundaries.' }
                }
                elseif ($spec.Profile -eq 'App') {
                    $appConfiguration = if ($spec.Configuration) { $spec.Configuration } else { $Configuration }
                    Ensure-TestBuild App $appConfiguration
                    $parameters = @{ Platform = $Platform; Configuration = $appConfiguration; TimeoutSeconds = $TimeoutSeconds }
                    if ($spec.Lease) { $parameters.LeaseProbe = $true } else { $parameters.TestArguments = @($spec.Arguments) }
                    & (Join-Path $PSScriptRoot 'Test-IsolatedUwp.ps1') @parameters
                }
                elseif ($spec.Profile -in @('NativeDiff', 'NativeBidi', 'RegexText')) {
                    Ensure-TestBuild $spec.Profile
                    $nativeArguments = if ($name -eq 'NativeBidi') {
                        @('--report', (Join-Path $reportRoot 'native-bidi.json'))
                    } else { @() }
                    & (Join-Path $repo "tests/IntegrationTests/Support/$name.Tests/bin/x64/$Configuration/$name.Tests.exe") @nativeArguments
                    if ($LASTEXITCODE -ne 0) { throw "Native x64 helper failed: $name" }
                }
                elseif ($name -eq 'Runner') {
                    & (Join-Path $repo 'tests/IntegrationTests/Support/RunnerContracts.ps1') -EvidenceRoot (Join-Path $reportRoot 'runner')
                }
                elseif ($name -eq 'Headers') {
                    & (Join-Path $PSScriptRoot 'Test-Headers.ps1')
                    & (Join-Path $repo 'tests/IntegrationTests/Support/HeaderContracts.ps1') -EvidenceRoot (Join-Path $reportRoot 'headers')
                }
                elseif ($name -eq 'Localization') { & (Join-Path $PSScriptRoot 'Test-Localization.ps1') }
                elseif ($name -eq 'Package') { & (Join-Path $PSScriptRoot 'Test-AppPackage.ps1') -Path $PackagePath }
                elseif ($name -eq 'PackagedApp') {
                    if ($Platform -ne 'x64') { throw 'PackagedApp currently validates x64 on a hosted desktop.' }
                    & (Join-Path $PSScriptRoot 'Test-PackagedApp.ps1') -PackagePath $PackagePath
                }
                elseif ($spec.Profile -eq 'Desktop') {
                    if ($Configuration -ne 'Release') { throw 'Desktop probes require the Release AOT app.' }
                    Ensure-TestBuild App
                    if ($name -eq 'DragDrop') {
                        Ensure-TestBuild DragPeer
                        & (Join-Path $PSScriptRoot 'Test-EditorDragDrop.ps1') -Platform $Platform
                    }
                    elseif ($name -in @('DpiLayout', 'DisplayDpi')) {
                        & (Join-Path $PSScriptRoot 'Test-EditorDpi.ps1') -Platform $Platform -CurrentScaleOnly:($name -eq 'DpiLayout')
                    }
                    else { & (Join-Path $PSScriptRoot 'Test-EditorAutomation.ps1') -Platform $Platform }
                }
                else { throw "No runner exists for selected group: $name" }
            }
        }
    }
}
finally {
    if ($Suite -ne 'Unit') {
        foreach ($name in $selected) {
            if ($name -notin @($reports | ForEach-Object { $_.Name })) {
                $reports.Add([ordered]@{ Name = $name; Status = 'NOT_RUN'; DurationMilliseconds = 0; Error = 'Earlier selected group failed.' })
            }
        }
        $reports | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $reportRoot 'results.json') -Encoding utf8
    }
    Write-Output "Test report: $reportRoot"
}
