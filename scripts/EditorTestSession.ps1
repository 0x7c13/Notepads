# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

. (Join-Path $PSScriptRoot 'IsolatedTestPackage.ps1')

# Shared lifecycle for isolated, interactive editor tests. A pre-existing test
# package is deliberately preserved; production packages are never considered.
function Read-EditorTestFile {
    param([string] $Path, [switch] $AsLines, [switch] $CompleteLinesOnly)
    # Protocol files are published by atomic rename, and timelines are appended
    # concurrently. Readers must permit both writers and deletion/rename handles.
    $sharing = [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete
    $stream = [IO.FileStream]::new($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, $sharing)
    $reader = $null
    try {
        $reader = [IO.StreamReader]::new($stream, [Text.UTF8Encoding]::new($false, $true), $true)
        $text = $reader.ReadToEnd()
        if ($AsLines) {
            if ($CompleteLinesOnly) {
                # An append can be observed halfway through its last TSV record.
                $lastNewline = $text.LastIndexOf("`n")
                if ($lastNewline -lt 0) { return ,@() }
                $text = $text.Substring(0, $lastNewline + 1)
            }
            return ,($text -split "`r`n|`n|`r")
        }
        return $text
    }
    finally {
        if ($reader) { $reader.Dispose() } else { $stream.Dispose() }
    }
}

function Start-EditorTestSession {
    param([ValidateSet('x64', 'ARM64')] [string] $Platform = 'ARM64', [int] $TimeoutSeconds = 40)
    $session = Start-IsolatedTestPackage -Platform $Platform -Configuration Release
    try {
        $session.Sequence = 0
        $session.Channel = Join-Path $session.RunRoot "InteractiveTests\$($session.RunId)"
        $identity = Start-IsolatedTestProcess $session "--interactive-test $($session.RunId)"
        $session.ProcessIdentity = $identity
        $session.ProcessId = $identity.Id
        $session.ProcessStartTicks = $identity.StartTicks
        @{ RunId = $session.RunId; ProcessId = $identity.Id; ProcessStartTicks = $identity.StartTicks; Layout = $session.Layout; Channel = $session.Channel } |
            ConvertTo-Json | Set-Content -LiteralPath (Join-Path $session.Root 'session.json') -Encoding utf8
        $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
        while (!(Test-Path -LiteralPath (Join-Path $session.Channel 'ready.txt'))) {
            Assert-EditorTestAlive $session
            if ([DateTime]::UtcNow -ge $deadline) { throw "Interactive editor startup timed out: $($session.Root)" }
            Start-Sleep -Milliseconds 100
        }
        return $session
    }
    catch {
        $failure = $_
        try { Stop-EditorTestSession $session }
        catch { throw "Interactive startup failed: $failure`nCleanup failed: $_" }
        throw $failure
    }
}
function Assert-EditorTestAlive {
    param([hashtable] $Session)
    $process = Get-IsolatedTestProcess $Session.ProcessIdentity
    if (!$process) { throw "Isolated editor process exited. Evidence: $($Session.Root)" }
    Assert-EditorTestNoFatalError $Session
}
function Assert-EditorTestNoFatalError {
    param([hashtable] $Session)
    $fatal = Join-Path $Session.Channel 'fatal.txt'
    if (Test-Path -LiteralPath $fatal) { throw (Read-EditorTestFile $fatal) }
}

function Invoke-EditorTestCommand {
    param([hashtable] $Session, [string] $Command, [int] $TimeoutSeconds = 15)
    Assert-EditorTestAlive $Session
    $Session.Sequence++
    $id = '{0:D4}' -f $Session.Sequence
    $commandPath = Join-Path $Session.Channel "command-$id.txt"
    [IO.File]::WriteAllText($commandPath + '.tmp', $Command, [Text.UTF8Encoding]::new($false))
    [IO.File]::Move($commandPath + '.tmp', $commandPath)
    $response = Join-Path $Session.Channel "response-$id.txt"
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while (!(Test-Path -LiteralPath $response)) {
        Assert-EditorTestNoFatalError $Session
        if (!(Get-IsolatedTestProcess $Session.ProcessIdentity)) {
            # Exit may publish its atomic acknowledgement between the response
            # check and process check. Only this command's final ACK permits
            # consuming the response; shutdown still validates the exit code.
            if ($Command -ceq 'exit' -and (Test-Path -LiteralPath $response)) { break }
            throw "Isolated editor process exited. Evidence: $($Session.Root)"
        }
        if ([DateTime]::UtcNow -gt $deadline) { throw "Interactive command timed out: $Command" }
        Start-Sleep -Milliseconds 25
    }
    Assert-EditorTestNoFatalError $Session
    $lines = Read-EditorTestFile $response -AsLines
    if ($lines[0] -ne 'OK') { throw ($lines -join "`n") }
    $state = @{}
    foreach ($line in $lines | Select-Object -Skip 1) {
        $pair = $line.Split(@('='), 2)
        if ($pair.Length -eq 2) { $state[$pair[0]] = $pair[1] }
    }
    return $state
}

function Stop-EditorTestSession {
    param([hashtable] $Session)
    if (!$Session -or $Session.Completed) { return }
    $errors = [Collections.Generic.List[string]]::new()
    try {
        if ($Session.ProcessIdentity) {
            try {
                if (!(Get-IsolatedTestProcess $Session.ProcessIdentity)) { throw 'Interactive test exited before shutdown was acknowledged.' }
                $null = Invoke-EditorTestCommand $Session 'exit'
                if (!$Session.ProcessIdentity.Process.WaitForExit(10000)) { throw 'Interactive test acknowledged exit but did not cleanly terminate.' }
                Assert-IsolatedTestExit $Session.ProcessIdentity
                Close-IsolatedTestProcess $Session $Session.ProcessIdentity
            }
            catch { $errors.Add($_.ToString()) }
        }
        if ($Session.Channel -and (Test-Path -LiteralPath $Session.Channel)) {
            try {
                $evidence = Join-Path $Session.Root 'commands'
                New-Item -ItemType Directory -Path $evidence -Force | Out-Null
                Get-ChildItem -LiteralPath $Session.Channel -File | Copy-Item -Destination $evidence -Force
            }
            catch { $errors.Add($_.ToString()) }
        }
    }
    finally {
        try { Complete-IsolatedTestPackage $Session }
        catch { $errors.Add($_.ToString()) }
    }
    if ($errors.Count) { throw ($errors -join "`n") }
}
