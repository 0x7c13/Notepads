# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Requires installed Microsoft Pinyin + English input profiles and an idle desktop.
# Default: physical IME regression and comparison with the system RichEdit control.
param(
    [ValidateSet('x64', 'ARM64')] [string] $Platform = 'ARM64',
    [string] $SourceLayout = '',
    [switch] $ObserveOnly,
    [switch] $SkipReference,
    [switch] $Workspace
)
$Regression = !$ObserveOnly
$WithReference = !$SkipReference
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
. (Join-Path $repo 'scripts/EditorTestSession.ps1')
. (Join-Path $repo 'scripts/TestBuildIdentity.ps1')
if (!$SourceLayout) { $null = Assert-TestBuildIdentity App Release $Platform }
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -Path (Join-Path $repo 'scripts/PackagedAppDriver.cs')
Add-Type -Path (Join-Path $repo 'scripts/OwnedWindowActivation.cs')
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class ImeProfileProbe {
    delegate bool EnumProc(IntPtr h, IntPtr d);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr d);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr h, EnumProc cb, IntPtr d);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr h, uint flags);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern short GetAsyncKeyState(int key);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder name, int count);
    [DllImport("user32.dll")] static extern int GetKeyboardLayoutList(int n, [Out] IntPtr[] list);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
    [DllImport("user32.dll", SetLastError=true)] static extern uint SendInput(uint count, Input[] input, int size);
    [StructLayout(LayoutKind.Explicit, Size=40)] struct Input {
        [FieldOffset(0)] public uint Type;
        [FieldOffset(8)] public ushort Key;
        [FieldOffset(12)] public uint Flags;
    }
    static IntPtr RequireOwnedIdle(uint expected) {
        for(int key=1;key<255;key++) if((GetAsyncKeyState(key)&0x8000)!=0) throw new Exception("Input is not idle.");
        IntPtr core=IntPtr.Zero;
        EnumProc inspect=(h,d)=>{ GetWindowThreadProcessId(h,out uint owner); var name=new StringBuilder(256);
            GetClassName(h,name,256); if(owner==expected && name.ToString()=="Windows.UI.Core.CoreWindow") core=h; return true; };
        EnumWindows((h,d)=>{inspect(h,d); EnumChildWindows(h,inspect,d); return true;},IntPtr.Zero);
        if(core==IntPtr.Zero) throw new Exception("Owned CoreWindow not found.");
        if(GetAncestor(core,2)!=GetForegroundWindow()) throw new Exception("Owned test window lost foreground.");
        return core;
    }
    public static void WinSpace(uint expected) {
        if(IntPtr.Size!=8) throw new PlatformNotSupportedException("The IME driver requires a 64-bit process.");
        RequireOwnedIdle(expected);
        var chord=new[] {
            new Input { Type=1, Key=0x5b, Flags=1 }, new Input { Type=1, Key=0x20 },
            new Input { Type=1, Key=0x20, Flags=2 }, new Input { Type=1, Key=0x5b, Flags=3 }
        };
        // Queue the complete chord together. If Windows accepts only a prefix,
        // release both possible held keys without adding any further key-downs.
        if(SendInput(4,chord,40)!=4) {
            var releases=new[] { chord[2], chord[3] };
            SendInput(2,releases,40);
            throw new Exception("Win+Space input was not completely accepted.");
        }
    }
    public static void Language(uint expected, int language) {
        var core=RequireOwnedIdle(expected);
        var layouts=new IntPtr[GetKeyboardLayoutList(0,null)]; GetKeyboardLayoutList(layouts.Length,layouts);
        foreach(var layout in layouts) if((layout.ToInt64()&0xffff)==language) {
            if(!PostMessage(core,0x50,IntPtr.Zero,layout)) throw new Exception("Input profile request failed."); return;
        }
        throw new Exception("Required input profile is not installed.");
    }
}
'@
$session = $null
$observations = [Collections.Generic.List[object]]::new()
$checks = 0
$completedAll = $false
function Check([bool]$condition, [string]$label) {
    if(!$condition) { throw "FAIL: $label" }
    $script:checks++
}
function Text($s) { [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($s.text64)) }
function CheckJournal([string]$label) {
    if(!$Regression) { return }
    $before=Invoke-EditorTestCommand $session 'state'
    $replayed=Invoke-EditorTestCommand $session 'ime-journal-check'
    Check ($replayed.journalText64 -ceq $before.text64) "$label (durable recovery replay)"
    Check ($replayed.undoDepth -eq $before.undoDepth -and $replayed.isComposing -eq $before.isComposing) "$label (checkpoint preserves composition and undo scope)"
}
function ExpectText([string]$text, [string]$label) {
    $s=WaitState { param($value) (Text $value) -ceq $text -and $value.isComposing -eq 'False' -and $value.undoDepth -eq '0' }
    $pattern=[Windows.Automation.TextPattern]$element.GetCurrentPattern([Windows.Automation.TextPattern]::Pattern)
    Check ($pattern.DocumentRange.GetText(-1) -ceq $text) "$label (independent UIA text)"
    $null=State $label
    return $s
}
function State([string]$label) {
    $s = Invoke-EditorTestCommand $session 'state'
    $row = @{ label=$label; text=(Text $s); composing=$s.isComposing; canUndo=$s.canUndo; caret=$s.caret; undoDepth=$s.undoDepth }
    $observations.Add($row)
    $row | ConvertTo-Json -Compress | Write-Host
    return $s
}
function Key([int]$code, [bool]$ctrl=$false) {
    OwnedIdle
    if (!$element.Current.HasKeyboardFocus) { throw 'Editor lost keyboard focus.' }
    [PackagedAppDriver]::Key([uint32]$session.ProcessId,[uint16]$code,$ctrl,$false)
}
function OwnedIdle { [OwnedWindowActivation]::RequireIdle(); Assert-EditorTestAlive $session }
function WaitState([scriptblock]$condition) {
    $watch=[Diagnostics.Stopwatch]::StartNew()
    do { $s=Invoke-EditorTestCommand $session 'state'; if (& $condition $s) { return $s }; Start-Sleep -Milliseconds 30 } while($watch.Elapsed.TotalSeconds -lt 6)
    throw "State did not settle: $($s | ConvertTo-Json -Compress)"
}
function Language([int]$language) {
    [ImeProfileProbe]::Language([uint32]$session.ProcessId,$language)
    $tag=if($language -eq 0x804) { 'zh-*' } else { 'en-*' }
    WaitInputLanguage $tag
}
function WaitInputLanguage([string]$tag) {
    $watch=[Diagnostics.Stopwatch]::StartNew()
    do {
        $s=Invoke-EditorTestCommand $session 'ime-state'
        if($s.inputLanguage -like $tag) { return }
        Start-Sleep -Milliseconds 30
    } while($watch.Elapsed.TotalSeconds -lt 5)
    throw "Input profile did not activate: $tag"
}
function Reset([string]$text='') {
    $encoded=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($text))
    $null=Invoke-EditorTestCommand $session "reset|$encoded|False|520|340|18"
    $element.SetFocus()
    Language 0x804
}
function TypePinyin {
    foreach($c in 'ceshiwenzi'.ToUpperInvariant().ToCharArray()) { Key ([int]$c) }
    $null=WaitState { param($s) $s.isComposing -eq 'True' -and (Text $s).Contains("ce'shi'wen'zi") }
}
function Compose {
    Key 67
    $first=WaitState { param($s) [int]$s.length -gt 0 }
    if($first.isComposing -ne 'True') {
        if([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($first.text64)) -cne 'c') { throw 'Unexpected IME mode probe.' }
        Reset
        Key 16
        Key 67
        $null=WaitState { param($s) $s.isComposing -eq 'True' }
    }
    foreach($c in 'eshiwenzi'.ToUpperInvariant().ToCharArray()) { Key ([int]$c) }
    $null=WaitState { param($s) $s.isComposing -eq 'True' -and [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($s.text64)) -like '*zi' }
}
function WorkspaceState([string]$label) {
    $s=Invoke-EditorTestCommand $session 'ime-workspace-state'
    $row=@{label=$label; text=(Text $s); composing=$s.isComposing; undoDepth=$s.undoDepth; canUndo=$s.canUndo; title=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($s.title64))}
    $observations.Add($row); $row | ConvertTo-Json -Compress | Write-Host
    return $s
}
function WaitWorkspace([scriptblock]$condition) {
    $watch=[Diagnostics.Stopwatch]::StartNew()
    do {
        $s=Invoke-EditorTestCommand $session 'ime-workspace-state'
        if(& $condition $s) { return $s }
        Start-Sleep -Milliseconds 30
    } while($watch.Elapsed.TotalSeconds -lt 6)
    throw "Workspace state did not settle: $($s | ConvertTo-Json -Compress)"
}
function TestWorkspace {
    $null=Invoke-EditorTestCommand $session 'ime-workspace-start'
    foreach($name in @('Untitled.txt','named draft')) {
        foreach($ending in @('Space','Escape','Shift','Language','SpaceUndo')) {
            $encoded=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($name))
            $null=Invoke-EditorTestCommand $session "ime-workspace-new|$encoded"
            $condition=[Windows.Automation.AndCondition]::new(
                [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty,[int]$session.ProcessId),
                [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'WorkspaceImeEditor'))
            $element=[Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Descendants,$condition)
            if(!$element) { throw 'Workspace editor not found.' }
            $element.SetFocus()
            Language 0x804
            Key 67
            $null=WaitWorkspace { param($s) [int]$s.length -gt 0 }
            $s=WorkspaceState "$name-$ending-first-key"
            if($s.isComposing -ne 'True') {
                if((Text $s) -cne 'c') { throw 'Unexpected workspace IME probe.' }
                Key 8; Key 16; Key 67
                $null=WaitWorkspace { param($s) $s.isComposing -eq 'True' }
                $s=WorkspaceState "$name-$ending-chinese-mode"
                if($s.isComposing -ne 'True') { throw 'Microsoft Pinyin composition did not start.' }
            }
            foreach($c in 'eshiwenzi'.ToUpperInvariant().ToCharArray()) {
                # Deliberate human typing cadence, not a correctness deadline.
                Start-Sleep -Milliseconds 120
                Key ([int]$c)
                $null=WorkspaceState "$name-$ending-key"
            }
            # Leave the preedit visible across normal UI/backup callbacks.
            Start-Sleep -Milliseconds 1500
            $null=WorkspaceState "$name-$ending-preedit"
            switch($ending) {
                Space { Key 32 }; Escape { Key 27 }; Shift { Key 16 }; Language { Language 0x409 }
                SpaceUndo { Key 32; Key 90 $true }
            }
            $null=WaitWorkspace { param($s) $s.isComposing -eq 'False' }
            $s=WorkspaceState "$name-$ending-completed"
            $committed=Text $s
            if($ending -eq 'SpaceUndo') {
                if($Regression) { Check ($committed -ceq '') 'Immediate Space then Ctrl+Z removes the whole composition' }
                continue
            }
            if($Regression) {
                Check ($s.isComposing -eq 'False' -and $s.undoDepth -eq '0') "$name/$ending completes composition"
                if($ending -eq 'Space') { Check ($committed -match '[\u4e00-\u9fff]') 'Workspace committed Chinese' }
                if($ending -eq 'Shift') { Check ($committed -ceq 'ceshiwenzi') 'Workspace Shift-to-English preserves the Latin commit' }
                if($ending -in @('Escape','Language')) { Check ($committed -ceq '') 'Workspace canceled preedit' }
                if($ending -eq 'Language') {
                    Check ($s.canRedo -eq 'False') 'A language switch cannot redo discarded preedit'
                    continue
                }
            }
            Key 90 $true
            if($Regression) { $null=WaitWorkspace { param($s) (Text $s) -ceq '' } }
            $s=WorkspaceState "$name-$ending-undo"
            if($Regression) {
                Check ((Text $s) -ceq '') "$name/$ending undo removes the composition"
                $pattern=[Windows.Automation.TextPattern]$element.GetCurrentPattern([Windows.Automation.TextPattern]::Pattern)
                Check ($pattern.DocumentRange.GetText(-1) -ceq '') 'Workspace independent UIA undo text'
            }
            Key 89 $true
            if($Regression) { $null=WaitWorkspace { param($s) (Text $s) -ceq $committed } }
            $s=WorkspaceState "$name-$ending-redo"
            if($Regression) { Check ((Text $s) -ceq $committed) "$name/$ending redo restores committed text" }
        }
    }
}
try {
    $session=Start-EditorTestSession -Platform $Platform -SourceLayout $SourceLayout
    Write-Host "Evidence: $($session.Root)"
    $condition=[Windows.Automation.AndCondition]::new(
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty,[int]$session.ProcessId),
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'InteractiveEditor'))
    $watch=[Diagnostics.Stopwatch]::StartNew()
    do {
        $element=[Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Descendants,$condition)
        if($element) { break }; Assert-EditorTestAlive $session; Start-Sleep -Milliseconds 100
    } while($watch.Elapsed.TotalSeconds -lt 20)
    if(!$element) { throw 'Editor not found.' }
    [OwnedWindowActivation]::RequireIdle()
    $element.SetFocus()
    $nativeElement=$element
    [OwnedWindowActivation]::Acquire([uint32]$session.ProcessId,[PackagedAppDriver]::Window([uint32]$session.ProcessId)) | Write-Host
    if($Workspace) { TestWorkspace }
    else {
    foreach($ending in @('Space','Escape','Enter','Language','Shift')) {
        Reset
        if($Regression) { $null=Invoke-EditorTestCommand $session 'ime-journal-start' }
        Compose
        CheckJournal "$ending-preedit"
        $null=State "$ending-preedit"
        switch($ending) {
            Space { Key 32 }
            Escape { Key 27 }
            Enter { Key 13 }
            Shift { Key 16 }
            Language { Language 0x409 }
        }
        $completed=WaitState { param($s) $s.isComposing -eq 'False' }
        $finalText=Text $completed
        CheckJournal "$ending-completed"
        if($Regression) {
            Check ($completed.undoDepth -eq '0') "$ending retires composition undo scope"
            if($ending -eq 'Space') { Check ($finalText -match '[\u4e00-\u9fff]') 'Space committed actual Chinese candidates' }
            if($ending -eq 'Escape') { Check ($finalText -ceq '') 'Escape cancels preedit' }
            if($ending -in @('Shift','Enter')) { Check ($finalText -ceq 'ceshiwenzi') "$ending preserves explicitly committed Latin text" }
            if($ending -eq 'Language') {
                Check ($finalText -ceq '') 'Switching input languages discards unconfirmed preedit'
                Check ($completed.canUndo -eq 'False' -and $completed.canRedo -eq 'False') 'Cancelled composition is absent from both undo and redo'
            }
        }
        $null=State "$ending-completed"
        Key 90 $true
        if($Regression) {
            $null=WaitState { param($s) (Text $s) -ceq '' -and $s.canUndo -eq 'False' }
            $null=ExpectText '' "$ending-single-undo"
        }
        $null=State "$ending-undo"
        CheckJournal "$ending-undo"
        Key 89 $true
        if($Regression) {
            $expectedUndo=if($ending -eq 'Language') { 'False' } else { 'True' }
            $null=WaitState { param($s) (Text $s) -ceq $finalText -and $s.canUndo -eq $expectedUndo }
            $null=ExpectText $finalText "$ending-single-redo"
        }
        $null=State "$ending-redo"
        CheckJournal "$ending-redo"
        if($Regression) { $null=Invoke-EditorTestCommand $session 'ime-journal-stop' }
    }
    if($Regression) {
        Reset; Compose
        OwnedIdle
        if(!$element.Current.HasKeyboardFocus) { throw 'Editor lost focus before Win+Space.' }
        [ImeProfileProbe]::WinSpace([uint32]$session.ProcessId)
        WaitInputLanguage 'en-*'
        $s=ExpectText '' 'physical-WinSpace-cancels-pending-Pinyin'
        Check ($s.canUndo -eq 'False' -and $s.canRedo -eq 'False') 'Win+Space discards pending composition history'

        Reset; Compose
        Key 32
        OwnedIdle
        if(!$element.Current.HasKeyboardFocus) { throw 'Editor lost focus after candidate commit.' }
        [ImeProfileProbe]::WinSpace([uint32]$session.ProcessId)
        WaitInputLanguage 'en-*'
        $s=WaitState { param($s) $s.isComposing -eq 'False' }
        Check ((Text $s) -match '^[\u4e00-\u9fff]+$') 'Space immediately followed by Win+Space preserves confirmed Chinese'
        $null=ExpectText (Text $s) 'immediate-commit-then-WinSpace'

        # Establish Chinese mode with a real composition, then seed a clean
        # fixture. Seeding never stands in for the keyboard/IME path under test.
        Reset; Compose; Key 27
        $null=WaitState { param($s) $s.isComposing -eq 'False' }
        $original='前😀old后'
        Reset $original
        $null=Invoke-EditorTestCommand $session 'select|7|10'
        TypePinyin; Key 32
        $s=WaitState { param($s) $s.isComposing -eq 'False' }
        $replacement=Text $s
        Check ($replacement.StartsWith('前😀') -and $replacement.EndsWith('后') -and !$replacement.Contains('old')) 'IME replaces only the selected UTF-8 byte range'
        Key 90 $true; $null=ExpectText $original 'selection-undo'
        Key 89 $true; $null=ExpectText $replacement 'selection-redo'

        Reset $original
        $null=Invoke-EditorTestCommand $session 'select|7|10'
        $null=Invoke-EditorTestCommand $session 'ime-journal-start'
        TypePinyin
        CheckJournal 'selected-preedit'
        Language 0x409
        $s=ExpectText $original 'language-switch-restores-replaced-selection'
        Check ($s.anchor -eq '7' -and $s.caret -eq '10') 'Cancellation restores the original Unicode selection'
        Check ($s.canUndo -eq 'False' -and $s.canRedo -eq 'False') 'Cancellation discards only the pending composition history'
        CheckJournal 'selected-language-cancel'
        Key 88; $null=ExpectText '前😀x后' 'typing-after-language-cancel'
        Key 90 $true; $null=ExpectText $original 'typing-after-language-cancel-undo'
        CheckJournal 'selected-language-cancel-then-undo'
        $null=Invoke-EditorTestCommand $session 'ime-journal-stop'

        Reset
        Language 0x409
        Key 65; $null=ExpectText 'a' 'prior-English'
        Language 0x804
        TypePinyin; Key 32
        $s=WaitState { param($s) $s.isComposing -eq 'False' }
        $chinese=Text $s
        Check ($chinese -match '^a[\u4e00-\u9fff]+$') 'Chinese composition preserves preceding ordinary typing'
        Language 0x409
        Key 88; $null=ExpectText ($chinese+'x') 'following-English'
        foreach($expected in @($chinese,'a','')) { Key 90 $true; $null=ExpectText $expected 'separate-undo' }
        foreach($expected in @('a',$chinese,($chinese+'x'))) { Key 89 $true; $null=ExpectText $expected 'separate-redo' }

        Reset; Compose; Key 32
        $s=WaitState { param($s) $s.isComposing -eq 'False' }
        $committed=Text $s
        TypePinyin
        Language 0x409
        $null=ExpectText $committed 'language-switch-preserves-previously-committed-Chinese'
        Key 90 $true; $null=ExpectText '' 'cancelled-preedit-does-not-consume-an-undo-step'
        Key 89 $true; $null=ExpectText $committed 'redo-restores-only-confirmed-Chinese'

        Reset; Compose
        $null=Invoke-EditorTestCommand $session 'ime-reference'
        $null=WaitState { param($s) $s.isComposing -eq 'False' -and $s.undoDepth -eq '0' }
        $s=State 'focus-loss'
        $blurred=Text $s
        $element.SetFocus()
        Key 90 $true; $null=ExpectText '' 'focus-loss-undo'
        Key 89 $true; $null=ExpectText $blurred 'focus-loss-redo'

        Reset; Compose
        $encoded=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('replaced 😀'))
        $null=Invoke-EditorTestCommand $session "set|$encoded"
        $s=ExpectText 'replaced 😀' 'replace-during-composition'
        Check ($s.canUndo -eq 'False') 'History reset during composition leaves a clean undo boundary'
    }
    if($WithReference) {
        $nativeElement=$element
        foreach($ending in @('Space','Escape','Enter','Language','Shift')) {
            $element=$nativeElement
            Reset
            Compose
            Key 27
            $null=WaitState { param($s) $s.isComposing -eq 'False' }
            $null=Invoke-EditorTestCommand $session 'ime-reference'
            $refCondition=[Windows.Automation.AndCondition]::new(
                [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty,[int]$session.ProcessId),
                [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'ImeReference'))
            $element=[Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Descendants,$refCondition)
            if(!$element) { throw 'RichEdit reference was not found.' }
            $element.SetFocus()
            foreach($c in 'ceshiwenzi'.ToUpperInvariant().ToCharArray()) { Key ([int]$c) }
            $s=Invoke-EditorTestCommand $session 'ime-reference-state'
            $value=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($s.referenceText64))
            if($value.TrimEnd("`r","`n") -ceq 'ceshiwenzi') {
                Key 65 $true; Key 8; Key 16
                foreach($c in 'ceshiwenzi'.ToUpperInvariant().ToCharArray()) { Key ([int]$c) }
            }
            foreach($stage in @('preedit','completed','undo','redo')) {
                if($stage -eq 'completed') {
                    switch($ending) {
                        Space { Key 32 }; Escape { Key 27 }; Enter { Key 13 }; Shift { Key 16 }
                        Language { Language 0x409 }
                    }
                }
                elseif($stage -eq 'undo') { Key 90 $true }
                elseif($stage -eq 'redo') { Key 89 $true }
                $s=Invoke-EditorTestCommand $session 'ime-reference-state'
                $row=@{label="RichEdit-$ending-$stage"; text=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($s.referenceText64))}
                $observations.Add($row); $row | ConvertTo-Json -Compress | Write-Host
                # A language switch deliberately cancels pending text in Notepads;
                # RichEdit's literal commit is recorded as a reference, not an oracle.
                if($Regression -and $ending -ne 'Language') {
                    $native=@($observations | Where-Object label -CEQ "$ending-$stage")[0]
                    Check ($row.text.TrimEnd("`r","`n") -ceq $native.text) "RichEdit agrees with native editor: $ending/$stage"
                }
            }
        }
    }
    if($Regression) {
        $element=$nativeElement
        Reset; Compose
        $s=Invoke-EditorTestCommand $session 'dispose'
        Check ($s.isComposing -eq 'False' -and $s.undoDepth -eq '0') 'Disposing an active composition retires its native undo scope'
    }
    }
    $completedAll = $true
    if($Regression) { Write-Host "PASS: $checks IME assertions through physical Microsoft Pinyin input." }
}
catch { Write-Host $_.ToString(); throw }
finally {
    if($session) {
        try {
            @{ mode=if($ObserveOnly){'observed'}else{'regression'}; workspace=[bool]$Workspace; checks=$checks; completed=$completedAll } | ConvertTo-Json | Set-Content (Join-Path $session.Root 'ime-result.json')
            $observations | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $session.Root 'ime-probe.json')
            Get-FileHash (Join-Path $session.Layout 'WinUIEditor.dll') | ConvertTo-Json | Set-Content (Join-Path $session.Root 'ime-native-hash.json')
        }
        finally { Stop-EditorTestSession $session }
    }
}
