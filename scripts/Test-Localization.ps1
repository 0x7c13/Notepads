# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

param([string] $StringsRoot = (Join-Path $PSScriptRoot '../src/Notepads/Strings'))
$ErrorActionPreference = 'Stop'
$resourceFiles = @('Resources.resw', 'Settings.resw', 'Manifest.resw')
$formatItem = [regex]::new('(?<!\{)\{[0-9]+(?:,-?[0-9]+)?(?::[^{}]+)?\}(?!\})')
$sharedValues = Import-PowerShellDataFile (Join-Path $PSScriptRoot 'Localization.SharedValues.psd1')
$catalogue = @{}
# Only these controls use fixed action captions. Future stateful switches may
# legitimately have different on/off text and are not covered by this contract.
$fixedToggleLabels = @(
    'AdvancedPage_StatusBarSettings_ShowHideStatusBarToggleSwitch',
    'PersonalizationPage_AccentColorSettings_UseWindowsAccentColorToggleSwitch',
    'TextAndEditorPage_TextWrappingSettings_ToggleSwitch',
    'AdvancedPage_SessionSnapshotSettings_OnOffToggleSwitch',
    'AdvancedPage_LaunchPreferenceSettings_AlwaysOpenNewWindowToggleSwitch',
    'TextAndEditorPage_LineHighlighterSettings_ToggleSwitch',
    'TextAndEditorPage_LineNumbersSettings_ToggleSwitch',
    'AdvancedPage_SmartCopySettings_EnableSmartCopyToggleSwitch',
    'AdvancedPage_LaunchPreferenceSettings_ExitWhenLastTabClosedToggleSwitch',
    'EditorWhitespace', 'EditorIndentGuides', 'EditorBraceMatching', 'EditorOccurrences'
)

function Read-Resources([string] $Path) {
    $text = [IO.File]::ReadAllText($Path)
    $document = [Xml.XmlDocument]::new()
    $document.LoadXml($text)
    $entries = @($document.SelectNodes('/root/data'))
    $singleLines = [regex]::Matches($text, '(?m)^  <data\b[^\r\n]*</data>\r?$')
    if ($singleLines.Count -ne $entries.Count) { throw "Resource entries must each occupy one line: $Path" }
    $result = [Collections.Generic.Dictionary[string,string]]::new([StringComparer]::Ordinal)
    foreach ($entry in $entries) {
        $name = $entry.GetAttribute('name')
        $value = $entry.SelectSingleNode('value')
        if ([string]::IsNullOrWhiteSpace($name) -or !$value) { throw "Invalid resource entry: $Path" }
        foreach ($child in $entry.ChildNodes) {
            if ($child.NodeType -in @([Xml.XmlNodeType]::Text, [Xml.XmlNodeType]::CDATA) -and ![string]::IsNullOrWhiteSpace($child.InnerText)) {
                throw "Unexpected text outside resource value/comment '$name': $Path"
            }
        }
        if (!$result.TryAdd($name, $value.InnerText)) { throw "Duplicate resource '$name': $Path" }
    }
    return ,$result
}

$locales = @(Get-ChildItem -LiteralPath $StringsRoot -Directory)
$fileCount = 0
$entryCount = 0
foreach ($fileName in $resourceFiles) {
    $english = Read-Resources (Join-Path $StringsRoot ('en-US/' + $fileName))
    foreach ($locale in $locales) {
        $path = Join-Path $locale.FullName $fileName
        $resources = Read-Resources $path
        if (!$catalogue.ContainsKey($locale.Name)) { $catalogue[$locale.Name] = @{} }
        $catalogue[$locale.Name][$fileName] = $resources
        if ($resources.Count -ne $english.Count) { throw "Resource key count differs from en-US: $path" }
        foreach ($name in $english.Keys) {
            if (!$resources.ContainsKey($name)) { throw "Missing resource '$name': $path" }
            $value = $resources[$name]
            if (![string]::IsNullOrWhiteSpace($english[$name]) -and [string]::IsNullOrWhiteSpace($value)) {
                throw "Empty translation '$name': $path"
            }
            $expected = @($formatItem.Matches($english[$name]) | ForEach-Object Value | Sort-Object) -join '|'
            $actual = @($formatItem.Matches($value) | ForEach-Object Value | Sort-Object) -join '|'
            if ($actual -cne $expected) { throw "Format placeholders differ for '$name': $path" }
            if ($expected) {
                try { $null = [Text.CompositeFormat]::Parse($value) }
                catch { throw "Invalid composite format for '$name': $path" }
            }
            if ($locale.Name -cne 'en-US' -and $value.Trim() -ceq $english[$name].Trim()) {
                $shared = $sharedValues["$fileName/$name"]
                if (!$shared -or $shared.Value -cne $value.Trim() -or
                    ($shared.Locales -cnotcontains '*' -and $shared.Locales -cnotcontains $locale.Name)) {
                    throw "Unreviewed English copy for '$name': $path"
                }
            }
        }
        $fileCount++
        $entryCount += $resources.Count
    }
}

foreach ($locale in $locales) {
    $settings = $catalogue[$locale.Name]['Settings.resw']
    $resources = $catalogue[$locale.Name]['Resources.resw']
    # These switches show a fixed action label, not a changing on/off caption.
    foreach ($toggle in $fixedToggleLabels) {
        $name = "$toggle.OffContent"
        $onName = "$toggle.OnContent"
        if (!$settings.ContainsKey($name) -or !$settings.ContainsKey($onName) -or $settings[$name] -cne $settings[$onName]) {
            throw "Toggle labels differ for '$name': $($locale.Name)"
        }
    }
    $urlKey = 'TextAndEditorPage_SearchEngineSettings_CustomSearchUrlRadioButton_CustomUrlErrorReport.Text'
    if (!$settings[$urlKey].Contains('https://www.example.com/search?q={0}', [StringComparison]::Ordinal)) {
        throw "Search URL example is damaged: $($locale.Name)"
    }
    if ($resources['DiffViewer_Whitespace'] -cne $settings['EditorWhitespace.OffContent']) {
        throw "Whitespace labels differ between editor and diff: $($locale.Name)"
    }
    # Separate actions in one dialog must remain distinguishable. This catches
    # Save accidentally translated as Cancel, and Cancel as Don't Save.
    foreach ($dialog in @('AppCloseSaveReminderDialog', 'SetCloseSaveReminderDialog', 'FileRenameDialog', 'RevertAllChangesConfirmationDialog', 'SessionCorruptionErrorDialog')) {
        $labels = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        foreach ($button in @('CloseButtonText', 'PrimaryButtonText', 'SecondaryButtonText')) {
            $key = "${dialog}_$button"
            if ($resources.ContainsKey($key) -and !$labels.Add($resources[$key].Trim())) {
                throw "Dialog actions have duplicate labels for '$dialog': $($locale.Name)"
            }
        }
    }
}
Write-Output "PASS localization: $($locales.Count) locales, $fileCount files, $entryCount entries; XML, keys, placeholders, English copies, URLs and action labels validated."
