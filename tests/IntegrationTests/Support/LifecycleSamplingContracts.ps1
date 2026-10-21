# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Exercise the actual performance-admission policy with private time and CIM
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
function Get-CimInstance {
    param([string] $Query, [int] $OperationTimeoutSec, [string] $ErrorAction)
    if ($Query -notmatch '\bWin32_PerfFormattedData_Counters_ProcessorInformation\b' -or
        $Query -notmatch '\bPercentProcessorUtility\b' -or $Query -notmatch "WHERE\s+Name\s*=\s*'_Total'" -or
        $OperationTimeoutSec -ne 5 -or $ErrorAction -ne 'Stop') { throw 'CPU sampling changed metric, total scope or provider error handling.' }
    $probe.Queries++
    if ($probe.Error) { throw $probe.Error }
    if ($probe.ContainsKey('FixedSamples')) { return $probe.FixedSamples }
    $value = $probe.Values[[Math]::Min($probe.Queries - 1, $probe.Values.Count - 1)]
    return [pscustomobject]@{ Name = '_Total'; PercentProcessorUtility = $value }
}
function New-Probe {
    return @{ Queries = 0; Sleeps = 0; Clock = [pscustomobject]@{ Elapsed = [TimeSpan]::Zero }; Values = @(200, 20, 90, 22, 21, 20) }
}
function Expect-Rejection([string] $Name, [string] $Message) {
    $failure = $null
    try { Wait-IdleCpu | Out-Null } catch { $failure = $_ }
    if (!$failure -or $failure.ToString() -notlike $Message) { throw "$Name did not reject as expected ($Message): $failure" }
    $checks.Add($Name)
}

$probe = New-Probe
$history = Wait-IdleCpu
if ($probe.Queries -ne 6 -or $probe.Sleeps -ne 6 -or ($history -join ',') -ne '200,20,90,22,21,20') {
    throw 'CPU admission did not reset its quiet streak after a busy sample.'
}
$checks.Add('Three consecutive idle samples are required; utility above 100 is valid busy data')

foreach ($case in @(
    @{ Name = 'Provider failure identifies CIM sampling'; Error = 'Fixture provider unavailable' },
    @{ Name = 'Empty result is not an idle sample'; Samples = @() },
    @{ Name = 'Duplicate totals are rejected'; Samples = @(@{ Name = '_Total'; PercentProcessorUtility = 10 }, @{ Name = '_Total'; PercentProcessorUtility = 20 }) },
    @{ Name = 'A per-core value cannot replace the total'; Samples = @(@{ Name = '0,0'; PercentProcessorUtility = 10 }) },
    @{ Name = 'Missing utility is rejected'; Samples = @(@{ Name = '_Total' }) },
    @{ Name = 'Negative utility is rejected'; Samples = @(@{ Name = '_Total'; PercentProcessorUtility = -1 }) },
    @{ Name = 'NaN utility is rejected'; Samples = @(@{ Name = '_Total'; PercentProcessorUtility = [double]::NaN }) },
    @{ Name = 'Infinite utility is rejected'; Samples = @(@{ Name = '_Total'; PercentProcessorUtility = [double]::PositiveInfinity }) }
)) {
    $probe = New-Probe
    if ($case.Error) { $probe.Error = $case.Error } else { $probe.FixedSamples = $case.Samples }
    Expect-Rejection $case.Name '*Cannot sample total processor utility through CIM*'
    if ($probe.Queries -ne 1 -or $probe.Sleeps -ne 1) { throw 'Invalid provider data was treated as ordinary CPU load or retried.' }
}

$probe = New-Probe
$probe.Values = @(90)
Expect-Rejection 'A continuously busy machine reaches the admission deadline' '*CPU did not settle*'
if ($probe.Queries -ne 121 -or $probe.Sleeps -ne 121) { throw 'CPU admission ignored its monotonic deadline.' }

$checks | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'checks.json')
Write-Output "PASS: lifecycle CPU sampling contracts ($($checks.Count) checks)."
