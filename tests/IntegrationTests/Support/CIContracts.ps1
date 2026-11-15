# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Exercise CI SDK bootstrap and the real selector against private fixtures.
# No SDK, package, process privilege or desktop setting is changed.
param([Parameter(Mandatory)][string] $EvidenceRoot)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$ciFixtureRoot = [IO.Path]::GetFullPath($EvidenceRoot)
$null = New-Item -ItemType Directory -Path $ciFixtureRoot -Force
$launcher = Join-Path $ciFixtureRoot 'Test-CIUI.ps1'
Copy-Item -LiteralPath (Join-Path $repo 'scripts/Test-CIUI.ps1') -Destination $launcher
@'
param([string] $Suite, [switch] $List, [string[]] $Group, [string] $Configuration, [string] $Platform)
$case = Get-Content (Join-Path $PSScriptRoot 'case.json') -Raw | ConvertFrom-Json
if ($List) {
    if ($Suite -ne 'Integration' -or $Configuration -ne 'Release') { throw 'Discovery must resolve default groups as Release.' }
    if ($case.Mode -eq 'list-failure') { throw 'fixture discovery failure' }
    if ($case.Mode -eq 'empty') { return }
    if ($case.Mode -eq 'unsupported') {
        [pscustomobject]@{ Group = 'Unexpected'; Profile = 'App'; Default = $true; Configuration = 'Production' }
    }
    @(
        @{ Group = 'Editor'; Profile = 'App'; Default = $true; Configuration = 'Release' },
        @{ Group = 'FutureFeature'; Profile = 'App'; Default = $true; Configuration = 'Release' },
        @{ Group = 'NativeFaults'; Profile = 'App'; Default = $true; Configuration = 'Debug' },
        @{ Group = 'Performance'; Profile = 'App'; Default = $false; Configuration = 'Release' },
        @{ Group = 'OptionalFaults'; Profile = 'App'; Default = $false; Configuration = 'Debug' },
        @{ Group = 'Runner'; Profile = 'Source'; Default = $true; Configuration = 'Release' }
    ) | ForEach-Object { [pscustomobject]$_ } | Where-Object {
        $case.Mode -ne 'missing-lane' -or $_.Configuration -ne $case.Configuration
    }
    return
}
if ($Suite -ne 'Integration' -or $Configuration -ne $case.Configuration -or $Platform -ne 'x64' -or
    ($Group -join ',') -ne $case.Groups) { throw 'Incorrect hosted UI group selection.' }
Set-Content -LiteralPath (Join-Path $PSScriptRoot 'invoked.txt') -Value ($Group -join ',')
if ($case.Mode -eq 'test-failure') { throw 'fixture UI test failure' }
'@ | Set-Content -LiteralPath (Join-Path $ciFixtureRoot 'Test.ps1')
$checks = [Collections.Generic.List[string]]::new()
# Execute the composite action's actual PowerShell step against private SDK
# policies. Bootstrap must install the declared version, not resolve latestPatch
# through aka.ms; build-time SDK roll-forward remains untouched.
$sdkAction = Get-Content -LiteralPath (Join-Path $repo '.github/actions/setup-dotnet/action.yml') -Raw
$sdkStep = [regex]::Match($sdkAction, '(?m)^      run: \|\r?\n(?<body>(?:        [^\r\n]*\r?\n)+)')
if (!$sdkStep.Success -or $sdkAction -match '(?m)^\s+global-json-file:' -or
    $sdkAction -notmatch 'dotnet-version: \$\{\{ steps\.sdk\.outputs\.version \}\}') {
    throw 'CI bootstrap must pass the explicit SDK version to setup-dotnet.'
}
$sdkCommand = [scriptblock]::Create(($sdkStep.Groups['body'].Value -replace '(?m)^        ', ''))
$sdkRoot = Join-Path $ciFixtureRoot 'sdk'
$null = New-Item -ItemType Directory -Path $sdkRoot -Force
$priorOutput = $env:GITHUB_OUTPUT
Push-Location -LiteralPath $sdkRoot
try {
    $env:GITHUB_OUTPUT = Join-Path $sdkRoot 'output.txt'
    foreach ($version in @('10.0.404', '10.0.405', '11.0.100', '10.0.4xx', '10.0.404-preview.1', '')) {
        $valid = $version -in @('10.0.404', '10.0.405')
        @{ sdk = @{ version = $version; rollForward = 'latestPatch'; allowPrerelease = $false } } |
            ConvertTo-Json | Set-Content -LiteralPath 'global.json'
        $before = (Get-FileHash -LiteralPath 'global.json').Hash
        Set-Content -LiteralPath $env:GITHUB_OUTPUT -Value '' -NoNewline
        $failure = $null
        try { & $sdkCommand } catch { $failure = $_ }
        $output = [IO.File]::ReadAllText($env:GITHUB_OUTPUT).Trim()
        if (($valid -and ($failure -or $output -cne "version=$version")) -or
            (!$valid -and (!$failure -or $output)) -or
            (Get-FileHash -LiteralPath 'global.json').Hash -cne $before) {
            throw "SDK bootstrap changed policy or resolved an incorrect installation version: $version / $failure"
        }
        $checks.Add("Explicit SDK bootstrap: '$version'")
    }
}
finally { Pop-Location; $env:GITHUB_OUTPUT = $priorOutput }
& {
    $rustRoot = Join-Path $ciFixtureRoot 'rust-policy'
    [IO.Directory]::CreateDirectory($rustRoot) | Out-Null
    $rustCalls = [Collections.Generic.List[string]]::new()
    $rustMode = 'complete'
    $fixtureChannel = '9.8.7'
    function rustup.exe {
        $rustCalls.Add(($args -join ' '))
        $global:LASTEXITCODE = 0
        switch ($args[0]) {
            'toolchain' { return }
            'run' {
                if ($rustMode -eq 'missing-compiler') { $global:LASTEXITCODE = 1; return }
                return "release: $fixtureChannel"
            }
            'target' {
                if ($rustMode -eq 'missing-target') { return 'x86_64-pc-windows-msvc' }
                return @('x86_64-pc-windows-msvc', 'i686-pc-windows-msvc', 'aarch64-pc-windows-msvc')
            }
            'component' {
                if ($rustMode -eq 'missing-component') { return 'rustfmt-x86_64-pc-windows-msvc' }
                return @('rustfmt-x86_64-pc-windows-msvc', 'clippy-x86_64-pc-windows-msvc')
            }
            default { throw 'Unexpected rustup fixture operation.' }
        }
    }
    $initializeRust = Join-Path $repo 'scripts/Initialize-RustToolchain.ps1'
    foreach ($fixtureChannel in @('9.8.7', '8.7.6')) {
        $policyPath = Join-Path $rustRoot 'rust-toolchain.toml'
        [IO.File]::WriteAllText($policyPath, @"
[toolchain]
channel = "$fixtureChannel"
profile = "minimal"
components = ["rustfmt", "clippy"]
targets = ["x86_64-pc-windows-msvc", "i686-pc-windows-msvc", "aarch64-pc-windows-msvc"]
"@)
        $policyHash = (Get-FileHash -LiteralPath $policyPath).Hash
        foreach ($rustMode in @('complete', 'missing-compiler', 'missing-target', 'missing-component')) {
            $rustCalls.Clear()
            $failure = $null
            try { & $initializeRust -ModuleRoot $rustRoot | Out-Null } catch { $failure = $_ }
            if (($rustMode -eq 'complete') -eq [bool]$failure -or
                @($rustCalls | Where-Object { $_ -like 'toolchain install*' }).Count) {
                throw "Rust read-only verification failed its fixture: $fixtureChannel/$rustMode/$failure"
            }
            $checks.Add("Read-only pinned Rust verification: $fixtureChannel/$rustMode")
        }
        $rustMode = 'complete'
        $rustCalls.Clear()
        & $initializeRust -ModuleRoot $rustRoot -InstallMissing | Out-Null
        $expectedInstall = "toolchain install $fixtureChannel --profile minimal --component rustfmt,clippy --target x86_64-pc-windows-msvc,i686-pc-windows-msvc,aarch64-pc-windows-msvc"
        if ($rustCalls[0] -cne $expectedInstall -or (Get-FileHash -LiteralPath $policyPath).Hash -cne $policyHash) {
            throw 'Rust provisioning changed or duplicated the checked-in toolchain policy.'
        }
        $checks.Add('Explicit provisioning follows the exact fixture Rust policy: ' + $fixtureChannel)
    }
}
$ciWorkflow = Get-Content -LiteralPath (Join-Path $repo '.github/workflows/ci.yml') -Raw
foreach ($job in @('native', 'package', 'ui')) {
    $jobBody = [regex]::Match($ciWorkflow, '(?ms)^  ' + $job + ':\r?\n(?<body>.*?)(?=^  [a-zA-Z0-9_-]+:|\z)').Groups['body'].Value
    if ($jobBody -notmatch 'run: ./scripts/Initialize-RustToolchain.ps1 -InstallMissing' -or
        ($job -eq 'native' -and $jobBody -notmatch 'run: ./src/Notepads.Markdown/scripts/Test-Rust.ps1 -Layer All')) {
        throw "CI $job omits the pinned Rust provisioning or native layer-test contract."
    }
    $checks.Add('CI Rust wiring: ' + $job)
}
$priorEnvironment = $env:RUNNER_ENVIRONMENT
try {
    $env:RUNNER_ENVIRONMENT = 'self-hosted'
    foreach ($path in @($launcher, (Join-Path $repo 'scripts/Initialize-CIDesktop.ps1'))) {
        $failure = $null
        try { & $path } catch { $failure = $_ }
        if (!$failure -or $failure.ToString() -notlike '*requires a disposable GitHub-hosted runner*') {
            throw 'A non-hosted invocation was not rejected before machine changes.'
        }
        $checks.Add('Non-hosted invocation rejected: ' + (Split-Path $path -Leaf))
    }
    $failure = $null
    try { & $launcher -Lane Faults } catch { $failure = $_ }
    if (!$failure -or $failure.ToString() -notlike '*requires a disposable GitHub-hosted runner*') {
        throw 'The fault lane accepted a non-hosted invocation.'
    }
    $checks.Add('Non-hosted fault lane rejected')
    $failure = $null
    try { & (Join-Path $repo 'scripts/Test-PackagedApp.ps1') -PackagePath 'does-not-exist.msixupload' }
    catch { $failure = $_ }
    if (!$failure -or $failure.ToString() -notlike '*require a disposable GitHub-hosted runner*') {
        throw 'Installed package test must reject a non-hosted machine before touching package files or state.'
    }
    $checks.Add('Production package installation rejected before accessing local state')
    $env:RUNNER_ENVIRONMENT = 'github-hosted'
    foreach ($lane in @('E2E', 'Faults')) {
        foreach ($case in @(
            @{ Mode = 'success'; Error = ''; Invoked = $true },
            @{ Mode = 'empty'; Error = '*No default App test groups*'; Invoked = $false },
            @{ Mode = 'missing-lane'; Error = '*No default App test groups*'; Invoked = $false },
            @{ Mode = 'unsupported'; Error = '*no supported CI configuration*'; Invoked = $false },
            @{ Mode = 'list-failure'; Error = '*fixture discovery failure*'; Invoked = $false },
            @{ Mode = 'test-failure'; Error = '*fixture UI test failure*'; Invoked = $true }
        )) {
            $case.Configuration = if ($lane -eq 'E2E') { 'Release' } else { 'Debug' }
            $case.Groups = if ($lane -eq 'E2E') {
                @('UIAutomation', 'DragDrop', 'TabDragging', 'AppLifecycle', 'WindowMoveBroker',
                    'MarkdownCoreInterop', 'MarkdownCoreImages', 'Editor', 'FutureFeature') -join ','
            } else { 'NativeFaults' }
            $case | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $ciFixtureRoot 'case.json')
            $marker = Join-Path $ciFixtureRoot 'invoked.txt'
            if (Test-Path -LiteralPath $marker) { Remove-Item -LiteralPath $marker }
            $failure = $null
            try { & $launcher -Lane $lane } catch { $failure = $_ }
            if (($case.Error -and (!$failure -or $failure.ToString() -notlike $case.Error)) -or (!$case.Error -and $failure) -or
                (Test-Path -LiteralPath $marker) -ne $case.Invoked) { throw "Incorrect CI selector outcome for $lane/$($case.Mode): $failure" }
            $checks.Add("$lane/$($case.Mode)")
        }
    }
}
finally { $env:RUNNER_ENVIRONMENT = $priorEnvironment }
$checks | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $ciFixtureRoot 'checks.json')
Write-Output "PASS: hosted UI selector contracts ($($checks.Count) checks)."
