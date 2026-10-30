# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Uses physical key chords against an isolated, owned foreground window.
# The test host temporarily loads Polish Programmers and restores/unloads it.
param([ValidateSet('x64', 'ARM64')] [string] $Platform = 'ARM64', [string] $SourceLayout = '')
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'EditorTestSession.ps1')
. (Join-Path $PSScriptRoot 'TestBuildIdentity.ps1')
if (!$SourceLayout) { $null = Assert-TestBuildIdentity App Release $Platform }
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -Path (Join-Path $PSScriptRoot 'PackagedAppDriver.cs')
Add-Type -Path (Join-Path $PSScriptRoot 'OwnedWindowActivation.cs')
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class InternationalKeyboardInput {
    [DllImport("user32.dll", SetLastError=true)] static extern uint SendInput(uint count, Input[] input, int size);
    [StructLayout(LayoutKind.Explicit, Size=40)] struct Input {
        [FieldOffset(0)] public uint Type;
        [FieldOffset(8)] public ushort Key;
        [FieldOffset(12)] public uint Flags;
    }
    static Input Key(ushort key, bool up) => new Input { Type=1, Key=key, Flags=(up?2u:0u) | (key==0xA5?1u:0u) };
    public static void Chord(ushort[] keys) {
        var input=new List<Input>(); var release=new List<Input>();
        foreach(var key in keys) input.Add(Key(key,false));
        for(int i=keys.Length-1;i>=0;i--) release.Add(Key(keys[i],true));
        input.AddRange(release);
        if(SendInput((uint)input.Count,input.ToArray(),40)!=(uint)input.Count) {
            SendInput((uint)release.Count,release.ToArray(),40);
            throw new InvalidOperationException("The complete key chord was not accepted.");
        }
    }
}
'@
$session = $null
$restored = $false
$checks = 0
$observations = [Collections.Generic.List[object]]::new()
function Text($state) { [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($state.text64)) }
function Check([bool]$condition, [string]$label) {
    if (!$condition) { throw "FAIL: $label" }
    $script:checks++
}
function State { Invoke-EditorTestCommand $session 'ime-workspace-state' }
function Chord([ushort[]]$keys) {
    Assert-EditorTestAlive $session
    [OwnedWindowActivation]::RequireIdle()
    if (![PackagedAppDriver]::IsForeground([uint32]$session.ProcessId)) { throw 'The owned test window lost foreground.' }
    if (!$element.Current.HasKeyboardFocus) { throw 'The owned editor lost keyboard focus.' }
    [InternationalKeyboardInput]::Chord($keys)
}
function Expect([string]$expected) {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    do {
        $state = State
        if ((Text $state) -ceq $expected) { break }
        Start-Sleep -Milliseconds 30
    } while ($watch.Elapsed.TotalSeconds -lt 5)
    $observations.Add(@{ expected=$expected; state=$state })
    Check ((Text $state) -ceq $expected) "Physical layout input: $expected (actual: $(Text $state))"
    Check ([int]$state.tabCount -eq 2) 'Character input never invokes the log-file shortcut or opens a tab.'
    $pattern = [Windows.Automation.TextPattern]$element.GetCurrentPattern([Windows.Automation.TextPattern]::Pattern)
    Check ($pattern.DocumentRange.GetText(-1) -ceq $expected) 'Independent UIA text agrees with native input.'
}
try {
    $session = Start-EditorTestSession -Platform $Platform -SourceLayout $SourceLayout
    Write-Host "Evidence: $($session.Root)"
    $null = Invoke-EditorTestCommand $session 'ime-workspace-start'
    $name = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('keyboard.txt'))
    $null = Invoke-EditorTestCommand $session "ime-workspace-new|$name"
    $condition = [Windows.Automation.AndCondition]::new(
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty,[int]$session.ProcessId),
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'WorkspaceImeEditor'))
    $element = [Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Descendants,$condition)
    if (!$element) { throw 'The workspace editor was not found.' }
    [OwnedWindowActivation]::RequireIdle()
    $element.SetFocus()
    [OwnedWindowActivation]::Acquire([uint32]$session.ProcessId,[PackagedAppDriver]::Window([uint32]$session.ProcessId)) | Write-Host
    $null = Invoke-EditorTestCommand $session 'keyboard-layout-polish'
    # Windows represents AltGr as Ctrl+RightAlt. Supply both modifier events
    # explicitly when injecting the chord through SendInput.
    Chord @(0xA2,0xA5,0x4C); Expect 'ł'
    Chord @(0xA2,0xA5,0x10,0x4C); Expect 'łŁ'
    Chord @(0xA2,0xA4,0x4C); Expect 'łŁł'
    Chord @(0xA2,0xA4,0x10,0x4C); Expect 'łŁłŁ'
    Chord @(0xA2,0x41)
    Chord @(0x08); Expect ''
    Chord @(0xA2,0x5A); Expect 'łŁłŁ'
    Chord @(0xA2,0x10,0x7B)
    $watch = [Diagnostics.Stopwatch]::StartNew()
    do { $state = State; if ([int]$state.tabCount -eq 3) { break }; Start-Sleep -Milliseconds 50 } while ($watch.Elapsed.TotalSeconds -lt 5)
    Check ([int]$state.tabCount -eq 3) 'Ctrl+Shift+F12 opens the diagnostic log through the actual workspace handler.'
    Write-Host "PASS: $checks international keyboard checks."
}
finally {
    if ($session) {
        try {
            $result = Invoke-EditorTestCommand $session 'keyboard-layout-restore'
            $restored = $result.keyboardLayoutRestored -eq 'True'
            if (!$restored) { throw 'Input layout restoration was not acknowledged.' }
        }
        finally {
            try {
                @{ checks=$checks; restored=$restored; observations=$observations } | ConvertTo-Json -Depth 6 |
                    Set-Content -LiteralPath (Join-Path $session.Root 'international-keyboard.json')
            }
            finally { Stop-EditorTestSession $session }
        }
    }
}
