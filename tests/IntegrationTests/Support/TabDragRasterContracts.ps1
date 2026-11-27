# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Exercise raster oracles without desktop capture or injected input.
param([Parameter(Mandatory)] [string] $EvidenceRoot)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$null = New-Item -ItemType Directory -Path $EvidenceRoot -Force
Add-Type -AssemblyName System.Drawing
if (!('TabDragRasterCapture' -as [type]) -or !('EditorInkSnapshot' -as [type])) {
    $references = @((Get-ChildItem (Join-Path $PSHOME 'ref/*.dll')).FullName +
        [Drawing.Bitmap].Assembly.Location, [Drawing.Point].Assembly.Location)
    $references += @([Drawing.Bitmap].Assembly.GetReferencedAssemblies() |
        Where-Object Name -Like 'System.Private.Windows.*' | ForEach-Object { [Reflection.Assembly]::Load($_).Location })
    foreach ($helper in @(@{ File = 'TabDragRasterCapture'; Type = 'TabDragRasterCapture' },
                         @{ File = 'EditorAutomationInput'; Type = 'EditorInkSnapshot' })) {
        if (!($helper.Type -as [type])) {
            Add-Type -Path (Join-Path $repo "scripts/$($helper.File).cs") -ReferencedAssemblies ($references | Select-Object -Unique)
        }
    }
}
$checks = [Collections.Generic.List[string]]::new()
$release = [TabDragRasterCapture+Frame]@{ Milliseconds = 100; X = [double[]]@(0) }
foreach ($case in @(
    @{ Name = 'Only settled frames cannot verify motion'; Times = @(200, 220, 240); Accepted = $false },
    @{ Name = 'Release and duration boundaries do not count'; Times = @(90, 100, 190, 220); Accepted = $false },
    @{ Name = 'Late in-animation samples cannot distinguish an instantaneous jump'; Times = @(180, 185, 200); Accepted = $false },
    @{ Name = 'One informative frame among several samples is sufficient'; Times = @(115, 200, 240); Accepted = $true },
    @{ Name = 'The same timestamp distinguishes a large jump'; Times = @(157, 200, 240); Jump = 50; Accepted = $true },
    @{ Name = 'The same timestamp cannot distinguish a small jump'; Times = @(157, 200, 240); Jump = 10; Accepted = $false },
    @{ Name = 'A jump equal to the allowance is not distinguishable'; Times = @(145, 200, 240); Jump = 12; Accepted = $false },
    @{ Name = 'A jump above the allowance is distinguishable'; Times = @(145, 200, 240); Jump = 12.1; Accepted = $true },
    @{ Name = 'Missing relevant marker cannot verify motion'; Times = @(116, 132, 200); MissingMarker = $true; Accepted = $false },
    @{ Name = 'Missing release baseline cannot verify motion'; Times = @(116, 132, 200); MissingBaseline = $true; Accepted = $false },
    @{ Name = 'Several genuine in-animation samples are accepted'; Times = @(116, 132, 200); Accepted = $true }
)) {
    $position = if ($case.MissingMarker) { [double]::NaN } else { 0.0 }
    $samples = [TabDragRasterCapture+Frame[]]@($case.Times | ForEach-Object {
        [TabDragRasterCapture+Frame]@{ Milliseconds = $_; X = [double[]]@($position) }
    })
    $jump = if ($case.ContainsKey('Jump')) { $case.Jump } else { 50.0 }
    $release.X[0] = if ($case.MissingBaseline) { [double]::NaN } else { 0.0 }
    $failure = $null
    try { [TabDragRasterCapture]::RequireSettlementSamples($release, $samples, 0, $jump, 1.5) } catch { $failure = $_ }
    if ($case.Accepted -eq [bool]$failure -or ($failure -and $failure.ToString() -notlike '*correctness is unverified*')) {
        throw "$($case.Name) did not enforce sample admission: $failure"
    }
    $checks.Add($case.Name)
}

$snapshot = [EditorInkSnapshot].GetConstructor([Reflection.BindingFlags]'Instance, NonPublic', $null, [type[]]@([Drawing.Bitmap]), $null)
function New-Snapshot([Drawing.Rectangle[]] $Bands) {
    $bitmap = [Drawing.Bitmap]::new(80, 40)
    try {
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.Clear([Drawing.Color]::FromArgb(36, 36, 36))
            foreach ($band in $Bands) { $graphics.FillRectangle([Drawing.Brushes]::Black, $band) }
        }
        finally { $graphics.Dispose() }
        return $snapshot.Invoke([object[]]@($bitmap))
    }
    finally { $bitmap.Dispose() }
}
$baseline = New-Snapshot
foreach ($case in @(
    @{ Name = 'Unchanged preview area'; Bands = @(); Width = 0 },
    @{ Name = 'Full preview body'; Bands = @([Drawing.Rectangle]::new(10, 8, 56, 18)); Width = 56 },
    @{ Name = 'One full-width accent row above a clipped body'; Bands = @(
        [Drawing.Rectangle]::new(10, 7, 56, 1), [Drawing.Rectangle]::new(10, 8, 36, 18)); Width = 36 },
    @{ Name = 'Separated full-width stripes do not form a body'; Bands = @(
        [Drawing.Rectangle]::new(10, 8, 56, 4), [Drawing.Rectangle]::new(10, 13, 56, 4)); Width = 0 },
    @{ Name = 'Shifted rows must overlap across the band height'; Bands = @(
        [Drawing.Rectangle]::new(10, 8, 56, 4), [Drawing.Rectangle]::new(16, 12, 56, 4)); Width = 50 }
)) {
    $width = (New-Snapshot $case.Bands).WidestChangedBand($baseline, 8)
    if ($width -ne $case.Width) { throw "$($case.Name) measured $width px, expected $($case.Width) px." }
    $checks.Add($case.Name)
}
$checks | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'checks.json') -Encoding utf8
Write-Output "PASS: Tab drag raster contracts ($($checks.Count) checks)"
