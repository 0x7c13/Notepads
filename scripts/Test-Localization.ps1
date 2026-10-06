# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE file in the project root for license information.
# ---------------------------------------------------------------------------------------------

param([string] $StringsRoot = (Join-Path $PSScriptRoot '../src/Notepads/Strings'))
$ErrorActionPreference = 'Stop'
$resourceFiles = @('Resources.resw', 'Settings.resw', 'Manifest.resw')
$formatItem = [regex]::new('(?<!\{)\{[0-9]+(?:,-?[0-9]+)?(?::[^{}]+)?\}(?!\})')

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
            if ($expected) { $null = [Text.CompositeFormat]::Parse($value) }
        }
        $fileCount++
        $entryCount += $resources.Count
    }
}
Write-Output "PASS localization: $($locales.Count) locales, $fileCount files, $entryCount entries; complete keys, valid XML, single-line entries and matching format placeholders."
