# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Call only with retained, identity-verified IsolatedTestProcess handles. A PID
# alone cannot identify a process after exit/reuse. Completed holds handle objects.
function Stop-PackagedAppProcesses {
    param([object[]] $Processes, [Collections.Generic.HashSet[object]] $Completed,
        [string] $EvidencePath, [string] $Reason)
    foreach ($process in $Processes) {
        if ($Completed.Contains($process)) { continue }
        $record = [ordered]@{
            ProcessId = $process.Id; Path = $process.Path; StartTime = $process.StartTime
            Reason = $Reason; TerminationRequested = $false; ExitCode = $null
            ExitCodeHex = $null; Status = 'FAIL'; Error = $null
        }
        try {
            $record.TerminationRequested = !$process.HasExited
            # IsolatedTestProcess.Kill uses TerminateProcess(handle, -1). Check
            # the final code even if the process exits between this check and Kill.
            if ($record.TerminationRequested) { $process.Kill() }
            if (!$process.WaitForExit(10000)) { throw "Owned process $($process.Id) did not terminate." }
            $record.ExitCode = $process.ExitCode
            $record.ExitCodeHex = '0x{0:X8}' -f $record.ExitCode
            if ($record.ExitCode -ne 0 -and !($record.TerminationRequested -and $record.ExitCode -eq -1)) {
                throw "Owned process $($process.Id) exited unexpectedly with $($record.ExitCodeHex)."
            }
            $null = $Completed.Add($process)
            $record.Status = 'PASS'
        }
        catch { $record.Error = $_.ToString(); throw }
        finally {
            $record | ConvertTo-Json -Compress | Add-Content -LiteralPath $EvidencePath
        }
    }
}
