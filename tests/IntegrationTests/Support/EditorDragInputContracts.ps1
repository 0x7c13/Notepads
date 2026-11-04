# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

param([Parameter(Mandatory)][string] $EvidenceRoot)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$null = New-Item -ItemType Directory -Path $EvidenceRoot -Force
$source = [IO.File]::ReadAllText((Join-Path $repo 'scripts/EditorDragInput.cs'))
$name = 'DragInputFixture' + [Guid]::NewGuid().ToString('N')
$source = $source.Replace('class EditorDragInput', "class $name").Replace('public EditorDragInput(', "public $name(")

# Replace SendInput before compilation; no fixture can inject desktop input.
# Bypass construction so cleanup runs without acquiring a window or DPI context.
$native = '[DllImport("user32.dll", SetLastError = true)] private static extern uint SendInput(uint count, Input[] inputs, int size);'
if (!$source.Contains($native)) { throw 'The fixture must replace the native input boundary.' }
$source = $source.Replace($native, @'
public static ushort RejectKey;
    public static readonly System.Collections.Generic.List<ushort> Releases = new();
    private static uint SendInput(uint count, Input[] inputs, int size) {
        if (count != 1 || inputs[0].Type != 1 || inputs[0].KeyFlags != 2)
            throw new InvalidOperationException("The cleanup fixture only accepts key releases.");
        Releases.Add(inputs[0].VirtualKey);
        return inputs[0].VirtualKey == RejectKey ? 0u : count;
    }
'@)
$sourcePath = Join-Path $EvidenceRoot 'DragInputFixture.cs'
[IO.File]::WriteAllText($sourcePath, $source)
$fixture = @(Add-Type -Path $sourcePath -PassThru | Where-Object Name -eq $name)[0]
$fields = @{ escapeDown = 27; altDown = 18; shiftDown = 16; controlDown = 17 }
$cases = @(
    @{ Keys = @(); Expected = @() },
    @{ Keys = @('controlDown'); Expected = @(17) },
    @{ Keys = @('controlDown', 'shiftDown'); Expected = @(16, 17) },
    @{ Keys = @('controlDown', 'altDown'); Expected = @(18, 17) },
    @{ Keys = @($fields.Keys); Expected = @(27, 18, 16, 17) }
)
foreach ($reject in @(27, 18, 16, 17)) {
    $cases += @{ Keys = @($fields.Keys); Expected = @(27, 18, 16, 17); Reject = $reject }
}
foreach ($case in $cases) {
    $driver = [Runtime.CompilerServices.RuntimeHelpers]::GetUninitializedObject($fixture)
    foreach ($key in $case.Keys) { $fixture.GetField($key, 'NonPublic, Instance').SetValue($driver, $true) }
    $fixture::Releases.Clear()
    $fixture::RejectKey = [ushort]$case.Reject
    $failure = $null
    try { $driver.ReleaseOwnedKeys() } catch { $failure = $_ }
    if ([bool]$case.Reject -ne [bool]$failure -or
        ($fixture::Releases -join ',') -cne ($case.Expected -join ',')) {
        throw "Cleanup must attempt every owned release even if a release fails: $($case.Reject)."
    }
    foreach ($field in $fields.Keys) {
        $held = $fixture.GetField($field, 'NonPublic, Instance').GetValue($driver)
        if ($held -ne ($fields[$field] -eq $case.Reject)) { throw "Incorrect key ownership after cleanup: $field." }
    }
    if ($driver.HasPressedInput -ne [bool]$case.Reject) { throw 'Cleanup lost ownership of a failed release.' }
    $fixture::RejectKey = 0
    $fixture::Releases.Clear()
    $driver.ReleaseOwnedKeys()
    $expectedRetry = if ($case.Reject) { "$($case.Reject)" } else { '' }
    if ($driver.HasPressedInput -or ($fixture::Releases -join ',') -cne $expectedRetry) {
        throw 'Retry must release only the keys still owned by the driver.'
    }
}
$cases | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'verified-cases.json')
Write-Output "PASS: drag input cleanup contracts ($($cases.Count) cases; inert OS input)."
