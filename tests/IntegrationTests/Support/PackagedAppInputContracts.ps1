# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

param([Parameter(Mandatory)][string] $EvidenceRoot)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$null = New-Item -ItemType Directory -Path $EvidenceRoot -Force
$source = [IO.File]::ReadAllText((Join-Path $repo 'scripts/PackagedAppDriver.cs'))
$name = 'PackagedInputFixture' + [Guid]::NewGuid().ToString('N')
$source = $source.Replace('class PackagedAppDriver', "class $name").Replace('public PackagedAppDriver(', "public $name(")

# Exercise the real batching/cleanup code. Replace its OS boundaries before
# compilation so even a broken driver cannot inject input into this desktop.
$native = '[DllImport("user32.dll", SetLastError = true)] private static extern uint SendInput(uint count, Input[] input, int size);'
if (!$source.Contains($native)) { throw 'The fixture must replace the native input boundary.' }
$source = $source.Replace($native, @'
public static uint Accepted;
    public static bool RejectFirstRelease;
    public static readonly System.Collections.Generic.List<string> Releases = new();
    private static uint SendInput(uint count, Input[] input, int size) {
        if (count > 1) return Accepted;
        Releases.Add(input[0].Key + ":" + input[0].Scan + ":" + input[0].Flags);
        return RejectFirstRelease && Releases.Count == 1 ? 0u : count;
    }
'@)
$start = $source.IndexOf('private static void RequireFocus(uint pid)')
$end = $source.IndexOf('private static void Send(ushort key', [Math]::Max(0, $start))
if ($start -lt 0 -or $end -le $start) { throw 'The fixture must isolate foreground observation.' }
$source = $source.Remove($start, $end - $start).Insert($start, "private static void RequireFocus(uint pid) { }`n`n    ")
$sourcePath = Join-Path $EvidenceRoot 'InputFixture.cs'
[IO.File]::WriteAllText($sourcePath, $source)
$fixture = @(Add-Type -Path $sourcePath -PassThru | Where-Object Name -eq $name)[0]

$cases = @(
    @{ Control = $false; Accepted = 0; Releases = @() },
    @{ Control = $false; Accepted = 1; Releases = @('65:0:2') },
    @{ Control = $false; Accepted = 2; Releases = @() },
    @{ Control = $false; Accepted = 3; Releases = @('0:124:6') },
    @{ Control = $false; Accepted = 4; Releases = @() },
    @{ Control = $true; Accepted = 0; Releases = @() },
    @{ Control = $true; Accepted = 1; Releases = @('17:0:2') },
    @{ Control = $true; Accepted = 2; Releases = @('65:0:2', '17:0:2') },
    @{ Control = $true; Accepted = 3; Releases = @('17:0:2') },
    @{ Control = $true; Accepted = 4; Releases = @() },
    @{ Control = $true; Accepted = 5; Releases = @('0:124:6') },
    @{ Control = $true; Accepted = 6; Releases = @() },
    @{ Control = $true; Accepted = 2; Releases = @('65:0:2', '17:0:2'); RejectFirstRelease = $true }
)
foreach ($case in $cases) {
    $fixture::Accepted = $case.Accepted
    $fixture::RejectFirstRelease = $case.RejectFirstRelease -eq $true
    $fixture::Releases.Clear()
    $failure = $null
    try { $fixture::KeyThenCharacter(0, 0x41, [char] '|', $case.Control) }
    catch { $failure = $_.Exception.GetBaseException() }
    $partial = $case.Accepted -lt $(if ($case.Control) { 6 } else { 4 })
    if ($partial -ne ($null -ne $failure) -or ($failure -and $failure -isnot [ComponentModel.Win32Exception])) {
        throw "Unexpected batch result: Control=$($case.Control), accepted=$($case.Accepted)."
    }
    if (($fixture::Releases -join '|') -cne ($case.Releases -join '|')) {
        throw "Incorrect releases: Control=$($case.Control), accepted=$($case.Accepted), failRelease=$($case.RejectFirstRelease)."
    }
}
$cases | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'verified-cases.json')
Write-Output "PASS: packaged input cleanup contracts ($($cases.Count) cases; inert OS input)."
