# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

param([string] $EvidenceRoot = '')
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$checker = Join-Path $repo 'scripts/Test-Localization.ps1'
if (!$EvidenceRoot) { $EvidenceRoot = Join-Path $repo ('artifacts/localization-contracts/' + [Guid]::NewGuid().ToString('N')) }
$fixtureRoot = Join-Path $EvidenceRoot ('Strings-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $fixtureRoot -Force
foreach ($locale in @('en-US', 'zh-CN', 'fr-FR', 'de-DE')) {
    Copy-Item -LiteralPath (Join-Path $repo "src/Notepads/Strings/$locale") -Destination $fixtureRoot -Recurse
}
$checks = [Collections.Generic.List[string]]::new()
& $checker -StringsRoot $fixtureRoot | Out-Null
$checks.Add('Accept complete translations, Chinese reference and reviewed shared words')

function Test-Rejection([string] $Name, [string] $File, [string] $Message, [scriptblock] $Change) {
    $path = Join-Path $fixtureRoot $File
    $before = [IO.File]::ReadAllText($path)
    try {
        $after = & $Change $before
        if ($after -ceq $before) { throw "Fixture did not change for $Name" }
        [IO.File]::WriteAllText($path, $after, [Text.UTF8Encoding]::new($false))
        $failure = $null
        try { & $checker -StringsRoot $fixtureRoot | Out-Null } catch { $failure = $_ }
        if (!$failure -or $failure.Exception.Message -notlike $Message) {
            throw "$Name did not fail with the expected guard ($Message): $failure"
        }
        $checks.Add($Name)
    }
    finally { [IO.File]::WriteAllText($path, $before, [Text.UTF8Encoding]::new($false)) }
}

function Replace-Value([string] $Text, [string] $Key, [string] $Value) {
    $pattern = '(<data name="' + [regex]::Escape($Key) + '"[^>]*><value>).*?(</value>)'
    if ([regex]::Matches($Text, $pattern).Count -ne 1) { throw "Fixture key missing or duplicated: $Key" }
    $escaped = [Security.SecurityElement]::Escape($Value)
    return [regex]::Replace($Text, $pattern, [Text.RegularExpressions.MatchEvaluator] {
        param($m)
        $m.Groups[1].Value + $escaped + $m.Groups[2].Value
    })
}

Test-Rejection 'English fallback in Chinese' 'zh-CN/Resources.resw' '*Unreviewed English copy*' {
    param($text)
    Replace-Value $text 'FileRenameDialog_PrimaryButtonText' 'Save'
}
Test-Rejection 'Whitespace cannot disguise an English copy' 'zh-CN/Resources.resw' '*Unreviewed English copy*' {
    param($text)
    Replace-Value $text 'FileRenameDialog_PrimaryButtonText' ' Save '
}
Test-Rejection 'Shared-word exception is locale-specific' 'de-DE/Resources.resw' '*Unreviewed English copy*' {
    param($text)
    Replace-Value $text 'MainMenu_AutomationName' 'Menu'
}
Test-Rejection 'Lost format placeholder' 'zh-CN/Resources.resw' '*Format placeholders differ*' {
    param($text)
    Replace-Value $text 'TextEditor_LineColumnIndicator_ShortText' '行 {0}'
}
Test-Rejection 'Malformed composite format' 'zh-CN/Resources.resw' '*Invalid composite format*' {
    param($text)
    Replace-Value $text 'TextEditor_LineColumnIndicator_ShortText' '行 {0}, 列 {1} {'
}
Test-Rejection 'Empty translation' 'zh-CN/Resources.resw' '*Empty translation*' {
    param($text)
    Replace-Value $text 'FileRenameDialog_PrimaryButtonText' ' '
}
Test-Rejection 'Duplicated key' 'zh-CN/Resources.resw' '*Duplicate resource*' {
    param($text)
    $line = [regex]::Match($text, '(?m)^  <data name="FileRenameDialog_PrimaryButtonText"[^\r\n]*').Value
    $text.Replace($line, "$line`n$line")
}
Test-Rejection 'Missing key' 'zh-CN/Resources.resw' '*Resource key count differs*' {
    param($text)
    [regex]::Replace($text, '(?m)^  <data name="FileRenameDialog_PrimaryButtonText"[^\r\n]*\r?\n', '')
}
Test-Rejection 'Inverted toggle caption' 'zh-CN/Settings.resw' '*Toggle labels differ*' {
    param($text)
    Replace-Value $text 'EditorWhitespace.OnContent' '隐藏空格和制表符'
}
# A new stateful switch is allowed without weakening fixed-caption contracts.
$stateLabels = @{
    'en-US' = @('Running', 'Stopped')
    'zh-CN' = @('运行中', '已停止')
    'fr-FR' = @('En marche', "À l’arrêt")
    'de-DE' = @('Läuft', 'Angehalten')
}
$originalSettings = @{}
try {
    foreach ($locale in $stateLabels.Keys) {
        $path = Join-Path $fixtureRoot "$locale/Settings.resw"
        $text = [IO.File]::ReadAllText($path)
        $originalSettings[$path] = $text
        $entries = '  <data name="ContractStatefulSwitch.OnContent" xml:space="preserve"><value>' +
            [Security.SecurityElement]::Escape($stateLabels[$locale][0]) + "</value></data>`r`n" +
            '  <data name="ContractStatefulSwitch.OffContent" xml:space="preserve"><value>' +
            [Security.SecurityElement]::Escape($stateLabels[$locale][1]) + "</value></data>`r`n"
        [IO.File]::WriteAllText($path, $text.Replace('</root>', $entries + '</root>'), [Text.UTF8Encoding]::new($false))
    }
    & $checker -StringsRoot $fixtureRoot | Out-Null
    $checks.Add('Accept distinct captions for a future stateful switch')
}
finally {
    foreach ($path in $originalSettings.Keys) {
        [IO.File]::WriteAllText($path, $originalSettings[$path], [Text.UTF8Encoding]::new($false))
    }
}
Test-Rejection 'Broken search URL scheme' 'zh-CN/Settings.resw' '*Search URL example is damaged*' {
    param($text)
    $text.Replace('https://www.example.com/search?q=', 'https//www.example.com/search?q=')
}
Test-Rejection 'Translated example domain' 'zh-CN/Settings.resw' '*Search URL example is damaged*' {
    param($text)
    $text.Replace('www.example.com', 'www.示例.com')
}
Test-Rejection 'Whitespace inside search URL' 'zh-CN/Settings.resw' '*Search URL example is damaged*' {
    param($text)
    $text.Replace('search?q={0}', 'search?q= {0}')
}
Test-Rejection 'Diff setting disagrees with editor' 'zh-CN/Resources.resw' '*Whitespace labels differ*' {
    param($text)
    Replace-Value $text 'DiffViewer_Whitespace' '显示空白字符'
}
Test-Rejection 'Save mislabeled as Cancel' 'zh-CN/Resources.resw' '*Dialog actions have duplicate labels*' {
    param($text)
    [xml] $document = $text
    $cancel = ($document.root.data | Where-Object name -CEQ 'FileRenameDialog_CloseButtonText').value
    Replace-Value $text 'FileRenameDialog_PrimaryButtonText' $cancel
}
Test-Rejection 'Cancel mislabeled as Do not save' 'zh-CN/Resources.resw' '*Dialog actions have duplicate labels*' {
    param($text)
    [xml] $document = $text
    $discard = ($document.root.data | Where-Object name -CEQ 'SetCloseSaveReminderDialog_SecondaryButtonText').value
    Replace-Value $text 'SetCloseSaveReminderDialog_CloseButtonText' $discard
}
Test-Rejection 'Recovery folder action mislabeled as Close' 'zh-CN/Resources.resw' '*Dialog actions have duplicate labels*' {
    param($text)
    [xml] $document = $text
    $close = ($document.root.data | Where-Object name -CEQ 'SessionCorruptionErrorDialog_CloseButtonText').value
    Replace-Value $text 'SessionCorruptionErrorDialog_PrimaryButtonText' $close
}
& $checker -StringsRoot $fixtureRoot | Out-Null
$checks | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'checks.json')
Write-Output "PASS localization contracts: $($checks.Count) checks; fixtures: $fixtureRoot"
