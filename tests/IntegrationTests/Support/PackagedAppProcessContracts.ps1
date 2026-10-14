# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Private console children and inert race fixtures exercise the same retained
# handles and exit validation as the installed-app tests. No app/UI is launched.
param([Parameter(Mandatory)][string] $EvidenceRoot)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
. (Join-Path $repo 'scripts/PackagedAppProcess.ps1')
Add-Type -Path (Join-Path $repo 'scripts/IsolatedTestProcess.cs')
$null = New-Item -ItemType Directory -Path $EvidenceRoot -Force
$checks = [Collections.Generic.List[string]]::new()

function Verify-Exit([string] $Name, $Process, [string] $ExpectedError, [bool] $Requested) {
    $path = Join-Path $EvidenceRoot "$Name.jsonl"
    $completed = [Collections.Generic.HashSet[object]]::new()
    $failure = $null
    try { Stop-PackagedAppProcesses @($Process) $completed $path $Name }
    catch { $failure = $_ }
    if (($ExpectedError -and (!$failure -or $failure.ToString() -notlike $ExpectedError)) -or
        (!$ExpectedError -and $failure)) { throw "Incorrect exit outcome for ${Name}: $failure" }
    $records = @(Get-Content -LiteralPath $path | ConvertFrom-Json)
    $status = if ($ExpectedError) { 'FAIL' } else { 'PASS' }
    if ($records.Count -ne 1 -or $records[0].Status -ne $status -or
        $records[0].TerminationRequested -ne $Requested -or $records[0].ProcessId -ne $Process.Id -or
        $completed.Contains($Process) -ne (!$ExpectedError)) { throw "Incorrect exit evidence for $Name." }
    if ($ExpectedError -notlike '*did not terminate*' -and $records[0].ExitCode -ne $Process.ExitCode) {
        throw "Exit evidence omitted or changed the actual code for $Name."
    }
    if (!$ExpectedError) {
        # Later closes still enumerate retained handles from earlier launches.
        Stop-PackagedAppProcesses @($Process) $completed $path 'repeated close'
        if (@(Get-Content -LiteralPath $path).Count -ne 1) { throw 'An already validated exit was processed twice.' }
    }
    $checks.Add($Name)
}

foreach ($case in @(
    @{ Name = 'normal-exit'; Code = 0; Error = ''; Terminate = $false },
    @{ Name = 'native-crash'; Code = -1073741819; Error = '*0xC0000005*'; Terminate = $false },
    @{ Name = 'application-failure'; Code = 7; Error = '*0x00000007*'; Terminate = $false },
    @{ Name = 'unrequested-minus-one'; Code = -1; Error = '*0xFFFFFFFF*'; Terminate = $false },
    @{ Name = 'intentional-termination'; Code = 0; Error = ''; Terminate = $true }
)) {
    $start = [Diagnostics.ProcessStartInfo]::new((Join-Path $PSHOME 'pwsh.exe'))
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardInput = $true
    foreach ($argument in @('-NoProfile', '-NonInteractive', '-Command',
        "[Console]::ReadLine() | Out-Null; [Environment]::Exit($($case.Code))")) { $start.ArgumentList.Add($argument) }
    $child = [Diagnostics.Process]::Start($start)
    $retained = $null
    try {
        $retained = [IsolatedTestProcess]::new($child.Id)
        if (!$case.Terminate) {
            $child.StandardInput.WriteLine('exit')
            $child.StandardInput.Flush()
            if (!$retained.WaitForExit(10000)) { throw 'Private exit fixture timed out.' }
        }
        Verify-Exit $case.Name $retained $case.Error $case.Terminate
    }
    finally {
        if (!$child.HasExited) { $child.Kill(); $child.WaitForExit() }
        if ($retained) { $retained.Dispose() }
        $child.Dispose()
    }
}

foreach ($case in @(
    @{ Name = 'crash-during-termination'; Code = -1073741819; Exits = $true; Error = '*0xC0000005*' },
    @{ Name = 'normal-exit-during-termination'; Code = 0; Exits = $true; Error = '' },
    @{ Name = 'termination-timeout'; Code = 259; Exits = $false; Error = '*did not terminate*' }
)) {
    # Deterministically finish between the live check and termination return.
    $fixture = [pscustomobject]@{
        Id = 123; Path = 'inert-race-fixture'; StartTime = [DateTime]::UtcNow
        HasExited = $false; ExitCode = $case.Code; Exits = $case.Exits
    }
    $fixture | Add-Member -MemberType ScriptMethod -Name Kill -Value { $this.HasExited = $this.Exits }
    $fixture | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { param($milliseconds); return $this.HasExited }
    Verify-Exit $case.Name $fixture $case.Error $true
}

$checks | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'checks.json')
Write-Output "PASS: packaged process exit contracts ($($checks.Count) checks)."
