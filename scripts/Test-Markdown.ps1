# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

param(
    [ValidateSet('x86', 'x64', 'ARM64')][string] $Platform = 'x64',
    [string] $SourceLayout = '',
    [ValidateRange(10, 120)][int] $TimeoutSeconds = 45
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'EditorTestSession.ps1')
. (Join-Path $PSScriptRoot 'TestBuildIdentity.ps1')
if (!$SourceLayout) { $null = Assert-TestBuildIdentity App Release $Platform }
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing
foreach ($helper in @('PackagedAppDriver', 'OwnedWindowActivation', 'MarkdownClipboardLease')) {
    if (!($helper -as [type])) { Add-Type -Path (Join-Path $PSScriptRoot "$helper.cs") }
}
if (!('EditorAutomationInput' -as [type])) {
    $references = @([System.Drawing.Bitmap].Assembly.Location, [System.Drawing.Point].Assembly.Location)
    $references += @([System.Drawing.Bitmap].Assembly.GetReferencedAssemblies() |
        Where-Object Name -Like 'System.Private.Windows.*' | ForEach-Object { [Reflection.Assembly]::Load($_).Location })
    Add-Type -Path (Join-Path $PSScriptRoot 'EditorAutomationInput.cs') -ReferencedAssemblies $references
}
$session = $null
$clipboard = $null
$ownedClipboardText = $null
$copyAttempted = $false
$checks = 0
$report = [ordered]@{ Status = 'RUNNING'; Platform = $Platform; Cases = [Collections.Generic.List[string]]::new(); Error = $null; ClipboardRestored = $false }

function Check([bool] $Condition, [string] $Message) {
    if (!$Condition) { throw $Message }
    $script:checks++
}
function Decode([string] $Value) { [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($Value)) }
function State { Invoke-EditorTestCommand $session 'markdown-state' }
function Wait-State([scriptblock] $Predicate, [string] $Message) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $state = State
        if (& $Predicate $state) { return $state }
        Start-Sleep -Milliseconds 25
    } while ([DateTime]::UtcNow -lt $deadline)
    $state | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $session.Root 'markdown-state-failure.json') -Encoding utf8
    throw $Message
}
function Current($State) {
    $State.markdownPanelOpen -eq 'True' -and $State.markdownSourceCurrent -eq 'True' -and
        $State.markdownStatus -eq 'Ready' -and $State.markdownSurface -eq 'True' -and
        $State.markdownDocument -ceq $State.markdownDisplayedDocument -and
        $State.markdownAcceptedRevision -eq $State.markdownDisplayedRevision -and
        [string]::IsNullOrEmpty((Decode $State.markdownRenderError64))
}
function Find-Control([string] $Id) {
    Assert-EditorTestAlive $session
    $window = [PackagedAppDriver]::Window([uint32]$session.ProcessId)
    $root = [Windows.Automation.AutomationElement]::FromHandle($window)
    $element = $root.FindFirst([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.AndCondition]::new(
            [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty, [int]$session.ProcessId),
            [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty, $Id)))
    if (!$element -or $element.Current.IsOffscreen) { throw "Owned visible control is unavailable: $Id" }
    return $element
}
function Require-Target($Target, [switch] $AllowDescendantFocus) {
    Assert-EditorTestAlive $session
    $current = $Target.Current
    if ($current.ProcessId -ne $session.ProcessId -or !$current.IsEnabled -or $current.IsOffscreen) { throw 'The physical input target changed.' }
    if (!$AllowDescendantFocus -and !$current.HasKeyboardFocus) { throw 'Keyboard focus changed before physical input.' }
    [EditorAutomationInput]::RequireIdle()
    $bounds = $current.BoundingRectangle
    [EditorAutomationInput]::RequireForeground([uint32]$session.ProcessId,
        [int]($bounds.Left + $bounds.Width / 2), [int]($bounds.Top + $bounds.Height / 2))
}
function Send-Key($Target, [int] $Key, [bool] $Control = $false, [bool] $Alt = $false) {
    Require-Target $Target
    [PackagedAppDriver]::Key([uint32]$session.ProcessId, [uint16]$Key, $Control, $false, $Alt)
}
function Click-Control($Target) {
    Require-Target $Target -AllowDescendantFocus
    $bounds = $Target.Current.BoundingRectangle
    [EditorAutomationInput]::Click([uint32]$session.ProcessId,
        [int]($bounds.Left + $bounds.Width / 2), [int]($bounds.Top + $bounds.Height / 2))
}
function Capture([string] $Name) {
    # A Page need not expose its own UIA container peer. Measure the two real
    # visible controls already used for physical input instead of a page peer.
    $source = Find-Control 'MarkdownKeyboardSource'
    $preview = Find-Control 'MarkdownPreviewDocument'
    $sourceBounds = $source.Current.BoundingRectangle
    $previewBounds = $preview.Current.BoundingRectangle
    if ($sourceBounds.Width -le 0 -or $sourceBounds.Height -le 0 -or
        $previewBounds.Width -le 0 -or $previewBounds.Height -le 0) {
        throw 'The owned Markdown screenshot panes have no visible bounds.'
    }
    $left = [int][Math]::Ceiling([Math]::Min($sourceBounds.Left, $previewBounds.Left)) + 4
    $top = [int][Math]::Ceiling([Math]::Min($sourceBounds.Top, $previewBounds.Top)) + 4
    $right = [int][Math]::Floor([Math]::Max($sourceBounds.Right, $previewBounds.Right)) - 4
    $bottom = [int][Math]::Floor([Math]::Max($sourceBounds.Bottom, $previewBounds.Bottom)) - 4
    @{ Source = $sourceBounds; Preview = $previewBounds;
        Capture = @{ Left = $left; Top = $top; Width = $right - $left; Height = $bottom - $top } } |
        ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $session.Root "$Name-bounds.json") -Encoding utf8
    $path = Join-Path $session.Root "$Name.png"
    $null = [EditorAutomationInput]::Capture([uint32]$session.ProcessId,
        $left, $top, ($right - $left), ($bottom - $top), $path)
}

try {
    $session = Start-EditorTestSession -Platform $Platform -SourceLayout $SourceLayout -TimeoutSeconds $TimeoutSeconds
    $report['Evidence'] = $session.Root
    $null = Invoke-EditorTestCommand $session 'markdown-start'
    $window = [PackagedAppDriver]::Window([uint32]$session.ProcessId)
    $null = [OwnedWindowActivation]::Acquire([uint32]$session.ProcessId, $window)
    $source = Find-Control 'MarkdownKeyboardSource'
    Click-Control $source
    Send-Key $source 0x23 $true # Ctrl+End
    $initial = State
    $original = Decode $initial.markdownSource64
    Check ($initial.markdownPanelOpen -eq 'False') 'The fixture must leave preview opening to real Alt+P.'
    Send-Key $source 0x50 $false $true # Alt+P
    $opened = Wait-State { param($state) (Current $state) -and $state.markdownImageLoaded -eq 'True' } 'Physical Alt+P did not open a current native Markdown preview.'
    Check ($opened.markdownSourceFocused -eq 'True') 'Opening preview must preserve source keyboard focus.'
    $report.Cases.Add('AltP opens the real native preview and loads the relative image')

    $full = [long]$opened.markdownFull
    $delta = [long]$opened.markdownDelta
    $typed = ' typed Ω 😀 0123456789'
    Require-Target $source
    [PackagedAppDriver]::Type([uint32]$session.ProcessId, $typed)
    $typedState = Wait-State { param($state) (Current $state) -and (Decode $state.markdownSource64) -ceq ($original + $typed) } 'Continuous physical typing did not produce the exact current source/preview.'
    Check ([long]$typedState.markdownDelta -gt $delta -and [long]$typedState.markdownFull -eq $full) 'Physical typing must advance ApplyEdits without complete source submission.'
    Check ($typedState.markdownSourceFocused -eq 'True' -and (Decode $typedState.markdownPreview64).Contains($typed)) 'Typing completion must preserve focus and display the inserted Unicode text.'
    $report.Cases.Add('Continuous physical Unicode input advances the real incremental source')

    $clipboard = [MarkdownClipboardLease]::new($window)
    $report['PreservedClipboardFormats'] = $clipboard.PreservedFormats
    $pasted = ' pasted Ω 😀 clipboard payload'
    $clipboard.SetText($pasted)
    $ownedClipboardText = $pasted
    Send-Key $source 0x56 $true # Ctrl+V reads the real OS clipboard through the declared test boundary.
    $pastedState = Wait-State { param($state) (Current $state) -and (Decode $state.markdownSource64) -ceq ($original + $typed + $pasted) } 'Physical Ctrl+V did not paste the real clipboard into the current native source.'
    Check ($clipboard.MatchesText($pasted)) 'After native paste, the real clipboard must still contain the exact owned payload before accepting any new sequence.'
    Check ([long]$pastedState.markdownDelta -gt [long]$typedState.markdownDelta -and [long]$pastedState.markdownFull -eq $full) 'Real clipboard paste must use a source delta.'
    Send-Key $source 0x5a $true # Ctrl+Z
    $undone = Wait-State { param($state) (Current $state) -and (Decode $state.markdownSource64) -ceq ($original + $typed) } 'Physical Undo did not reverse the complete paste.'
    Send-Key $source 0x59 $true # Ctrl+Y
    $redone = Wait-State { param($state) (Current $state) -and (Decode $state.markdownSource64) -ceq ($original + $typed + $pasted) } 'Physical Redo did not restore the pasted Unicode payload.'
    Check ([long]$redone.markdownDelta -gt [long]$undone.markdownDelta -and $redone.markdownSourceFocused -eq 'True') 'Undo/Redo completion must remain incremental and keep source focus.'
    $report.Cases.Add('Physical OS paste, Undo and Redo converge to the exact latest source')

    Send-Key $source 0x50 $false $true
    $null = Wait-State { param($state) $state.markdownPanelOpen -eq 'False' } 'Physical Alt+P did not close preview.'
    Send-Key $source 0x50 $false $true
    $reopened = Wait-State { param($state) (Current $state) -and $state.markdownImageLoaded -eq 'True' } 'Physical Alt+P did not reopen a new current preview.'
    Check ((Decode $reopened.markdownSource64) -ceq ($original + $typed + $pasted)) 'Alt+P close/reopen must preserve the edited source.'
    $report.Cases.Add('Physical AltP closes and reopens without losing native edits')

    # Return to the beginning with a real navigation key, then use the rendered
    # text provider only to measure the mouse targets. It does not select text.
    Send-Key $source 0x24 $true # Ctrl+Home
    $null = Wait-State { param($state) (Current $state) -and $state.markdownSourceCaret -eq '0' } 'Ctrl+Home did not return to the current document start.'
    Start-Sleep -Milliseconds 180
    $preview = Find-Control 'MarkdownPreviewDocument'
    $pattern = $preview.GetCurrentPattern([Windows.Automation.TextPattern]::Pattern)
    $from = $pattern.DocumentRange.FindText('Alpha', $false, $false)
    $through = $pattern.DocumentRange.FindText('Selection ends', $false, $false)
    Check ($from -and $through) 'The real preview text provider must expose both measured drag endpoints.'
    $first = @($from.GetBoundingRectangles())[0]
    $last = @($through.GetBoundingRectangles())[0]
    $x = [int]($first.Left + 2)
    $startY = [int]($first.Top + $first.Height / 2)
    $endY = [int]($last.Top + $last.Height / 2)
    $canary = 'Notepads-Markdown-clipboard-canary-' + [Guid]::NewGuid().ToString('N')
    Check ($clipboard.MatchesText($pasted)) 'Before the copy canary, independently verify that no external content replaced the owned paste payload.'
    $clipboard.SetTextAfterVerifiedText($canary, $pasted)
    $ownedClipboardText = $canary
    [EditorAutomationInput]::DragVertical([uint32]$session.ProcessId, $x, $startY, ($endY - $startY))
    $selected = Wait-State { param($state) $state.markdownPreviewFocused -eq 'True' -and [long]$state.markdownSelectionLength -gt 0 } 'The physical preview drag did not make a native selection.'
    $plain = Decode $selected.markdownPreview64
    $chosen = $plain.Substring([int]$selected.markdownSelectionStart, [int]$selected.markdownSelectionLength)
    Check ($chosen.Contains([string][char]0xfffc)) 'The measured physical drag must cross the rendered image object.'
    $expectedCopy = $chosen.Replace([string][char]0xfffc, 'keyboard image alt')
    $copyAttempted = $true
    Send-Key $preview 0x43 $true # Ctrl+C in the actual preview.
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while (!$clipboard.MatchesText($expectedCopy) -and [DateTime]::UtcNow -lt $deadline) {
        Assert-EditorTestAlive $session
        Start-Sleep -Milliseconds 25
    }
    Check ($clipboard.MatchesText($expectedCopy) -and $expectedCopy -cne $canary) 'Physical preview Ctrl+C must replace its unique canary with exact selected text and image alt.'
    $ownedClipboardText = $expectedCopy
    Check ((State).markdownPreviewFocused -eq 'True') 'Preview copying must keep keyboard focus in the preview.'
    $report.Cases.Add('Physical mouse selection and CtrlC export exact text/image alt through a real clipboard canary')

    foreach ($theme in @('Light', 'Dark')) {
        $before = State
        $null = Invoke-EditorTestCommand $session "markdown-theme|$theme"
        $null = Wait-State { param($state) (Current $state) -and $state.markdownTheme -eq $theme -and [long]$state.markdownThemeGeneration -gt [long]$before.markdownThemeGeneration } "The real $theme theme did not reach the current preview."
        Capture "markdown-$($theme.ToLowerInvariant())"
    }
    $report.Cases.Add('Light/Dark screenshots contain only the exact owned test page')
    $clipboard.RestoreAfterVerifiedText($ownedClipboardText)
    $report.ClipboardRestored = $clipboard.Restored
    Check $report.ClipboardRestored 'The preserved clipboard formats must be restored with exact bytes.'
    $clipboard.Dispose()
    $clipboard = $null
    $null = Invoke-EditorTestCommand $session 'markdown-stop'
    $report.Status = 'PASS'
    Write-Output "PASS Markdown physical input: $($report.Cases.Count) cases, $checks assertions. Evidence: $($session.Root)"
}
catch { $report.Status = 'FAIL'; $report.Error = $_.ToString(); throw }
finally {
    $cleanupErrors = [Collections.Generic.List[string]]::new()
    if ($clipboard) {
        try {
            if ($copyAttempted) {
                $process = $session.ProcessIdentity.Process
                $clipboard.RestoreAfterTestCopy($ownedClipboardText, [uint32]$process.Id, !$process.HasExited)
            }
            elseif ($null -ne $ownedClipboardText) { $clipboard.RestoreAfterVerifiedText($ownedClipboardText) }
        }
        catch { $cleanupErrors.Add('Clipboard recovery: ' + $_.ToString()) }
        try { $clipboard.Dispose() }
        catch { $cleanupErrors.Add('Clipboard cleanup: ' + $_.ToString()) }
        $report.ClipboardRestored = $clipboard.Restored
    }
    if ($session) {
        try { Stop-EditorTestSession $session }
        catch { $cleanupErrors.Add('Host cleanup: ' + $_.ToString()) }
        $report['Checks'] = $checks
        $report['CleanupErrors'] = @($cleanupErrors)
        if ($cleanupErrors.Count) { $report.Status = 'FAIL' }
        $report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $session.Root 'markdown-keyboard-results.json') -Encoding utf8
    }
    if ($cleanupErrors.Count) { throw ($cleanupErrors -join "`n") }
}
