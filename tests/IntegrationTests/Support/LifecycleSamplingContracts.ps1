# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Exercise the actual performance-admission policy with private time and PDH
# boundaries. No performance threshold is applied to this machine by these tests.
param([Parameter(Mandatory)] [string] $EvidenceRoot)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$null = New-Item -ItemType Directory -Path $EvidenceRoot -Force
$checks = [Collections.Generic.List[string]]::new()
$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $repo 'scripts/Test-AppLifecycle.ps1'), [ref]$null, [ref]$null)
$definition = $ast.Find({ param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Wait-IdleCpu'
}, $false)
$body = $definition.Extent.Text.Replace('[Diagnostics.Stopwatch]::StartNew()', '(New-SampleClock)')
$body = $body.Replace('[ProcessorUtilitySampler]::new()', '(New-UtilitySampler)')
if ($body -ceq $definition.Extent.Text) { throw 'The CPU admission clock boundary was not found.' }
. ([scriptblock]::Create($body))

function New-SampleClock {
    $probe.Clock.Elapsed = [TimeSpan]::Zero
    return $probe.Clock
}
function Start-Sleep {
    param([int] $Seconds)
    if ($Seconds -ne 1 -or ++$probe.Sleeps -gt 125) { throw 'CPU sampling did not wait between samples or respect its deadline.' }
    $probe.Clock.Elapsed += [TimeSpan]::FromSeconds($Seconds)
}
function New-UtilitySampler {
    if ($probe.OpenError) { throw $probe.OpenError }
    $sampler = [pscustomobject]@{}
    $sampler | Add-Member ScriptMethod Read {
        $probe.Queries++
        if ($probe.Error) { throw $probe.Error }
        return $probe.Values[[Math]::Min($probe.Queries - 1, $probe.Values.Count - 1)]
    }
    $sampler | Add-Member ScriptMethod Dispose { $probe.Disposals++ }
    return $sampler
}
function New-Probe {
    return @{ Queries = 0; Sleeps = 0; Disposals = 0; Clock = [pscustomobject]@{ Elapsed = [TimeSpan]::Zero }; Values = @(200, 20, 90, 22, 21, 20) }
}
function Expect-Rejection([string] $Name, [string] $Message) {
    $failure = $null
    try { Wait-IdleCpu | Out-Null } catch { $failure = $_ }
    if (!$failure -or $failure.ToString() -notlike $Message) { throw "$Name did not reject as expected ($Message): $failure" }
    $checks.Add($Name)
}

$probe = New-Probe
$history = Wait-IdleCpu
if ($probe.Queries -ne 6 -or $probe.Sleeps -ne 6 -or $probe.Disposals -ne 1 -or ($history -join ',') -ne '200,20,90,22,21,20') {
    throw 'CPU admission did not reset its quiet streak after a busy sample.'
}
$checks.Add('Three consecutive idle samples are required; utility above 100 is valid busy data')

foreach ($case in @(
    @{ Name = 'Provider failure identifies PDH sampling'; Error = 'Fixture provider unavailable' },
    @{ Name = 'Negative utility is rejected'; Values = @(-1) },
    @{ Name = 'NaN utility is rejected'; Values = @([double]::NaN) },
    @{ Name = 'Infinite utility is rejected'; Values = @([double]::PositiveInfinity) }
)) {
    $probe = New-Probe
    if ($case.Error) { $probe.Error = $case.Error } else { $probe.Values = $case.Values }
    Expect-Rejection $case.Name '*Cannot sample total processor utility through PDH*'
    if ($probe.Queries -ne 1 -or $probe.Sleeps -ne 1 -or $probe.Disposals -ne 1) { throw 'Provider failure was ignored or its query leaked.' }
}

$probe = New-Probe
$probe.OpenError = 'Fixture query open failed'
Expect-Rejection 'Query initialization failure is explicit' '*Cannot sample total processor utility through PDH*'
if ($probe.Queries -ne 0 -or $probe.Sleeps -ne 0 -or $probe.Disposals -ne 0) { throw 'Failed initialization sampled an unavailable query.' }

$probe = New-Probe
$probe.Values = @(90)
Expect-Rejection 'A continuously busy machine reaches the admission deadline' '*CPU did not settle*'
if ($probe.Queries -ne 121 -or $probe.Sleeps -ne 121 -or $probe.Disposals -ne 1) { throw 'CPU admission ignored its deadline or leaked its query.' }

$checks | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'checks.json')
Write-Output "PASS: lifecycle CPU sampling contracts ($($checks.Count) checks)."
