# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Exercise the real CI selector against a private inert Test.ps1. No package,
# process privilege or desktop setting is changed.
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
            $case.Groups = if ($lane -eq 'E2E') { 'UIAutomation,DragDrop,Editor,FutureFeature' } else { 'NativeFaults' }
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
