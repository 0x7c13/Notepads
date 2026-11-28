# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

param([Parameter(Mandatory)][string] $EvidenceRoot)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$null = New-Item -ItemType Directory -Path $EvidenceRoot -Force
$tokens = $parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile(
    (Join-Path $repo 'scripts/Test-InternationalKeyboard.ps1'), [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw 'The international keyboard script must parse.' }
$try = @($ast.EndBlock.Statements | Where-Object { $_ -is [Management.Automation.Language.TryStatementAst] })
if ($try.Count -ne 1 -or !$try[0].Finally) { throw 'The script must have one outer cleanup boundary.' }
$body = $try[0].Finally.Extent.Text
$cleanup = [scriptblock]::Create($body.Substring(1, $body.Length - 2))
$checks = [Collections.Generic.List[object]]::new()

# Execute only the real finally body with inert command boundaries. Never load
# a layout, launch an application or inject input in these failure tests.
foreach ($failure in @('', 'restore', 'receipt', 'stop')) {
    & {
        param($failure)
        $session = @{ Root = $EvidenceRoot }
        $restored = $false
        $observations = @()
        $script:events = [Collections.Generic.List[string]]::new()
        function Invoke-EditorTestCommand {
            $script:events.Add('restore')
            if ($failure -eq 'restore') { throw 'injected restore failure' }
            return @{ keyboardLayoutRestored = 'True' }
        }
        function Set-Content {
            param([string]$LiteralPath, [Parameter(ValueFromPipeline)]$Value)
            process {
                $script:events.Add('receipt')
                if ($failure -eq 'receipt') { throw 'injected receipt failure' }
            }
        }
        function Stop-EditorTestSession {
            $script:events.Add('stop')
            if ($failure -eq 'stop') { throw 'injected stop failure' }
        }
        $errorText = ''
        try { . $cleanup } catch { $errorText = $_.Exception.Message }
        if (($script:events -join ',') -ne 'restore,receipt,stop') {
            throw "Session cleanup was skipped after '$failure': $($script:events -join ',')."
        }
        $expected = if ($failure) { "injected $failure failure" } else { '' }
        if ($errorText -ne $expected) { throw "The cleanup error was lost or replaced: $errorText" }
        $checks.Add(@{ Failure = $failure; Events = @($script:events); Error = $errorText })
    } $failure
}
$checks | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'verified-cases.json')
Write-Output "PASS: international keyboard cleanup contracts ($($checks.Count) cases; inert OS input)."
