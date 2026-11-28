# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

param([Parameter(Mandatory)][string] $EvidenceRoot)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$fixture = Join-Path ([IO.Path]::GetFullPath($EvidenceRoot)) 'fixture'
$scripts = Join-Path $fixture 'scripts'
$null = New-Item -ItemType Directory -Path $scripts -Force
$runner = Join-Path $scripts 'Test.ps1'
Copy-Item -LiteralPath (Join-Path $repo 'scripts/Test.ps1') -Destination $runner
[IO.File]::WriteAllText((Join-Path $scripts 'TestBuildIdentity.ps1'), @'
function Assert-TestBuildIdentity {
    param($Profile, $Configuration, $Platform)
    throw "EXPECTED identity validation: $Profile/$Platform"
}
function Get-TestSourceHashes { throw 'UNEXPECTED build admission' }
'@)
$checks = [Collections.Generic.List[string]]::new()
function Expect-PlatformResult([string] $Group, [string] $Platform, [string] $Message) {
    $failure = $null
    try { & $runner -Suite Integration -Group $Group -Platform $Platform -NoBuild | Out-Null }
    catch { $failure = $_ }
    if (!$failure -or $failure.Exception.Message -notlike $Message) {
        throw "Unexpected platform preflight for $Group/${Platform}: $failure"
    }
    $checks.Add("$Group/${Platform}: $Message")
}
$listed = @(& $runner -Suite Integration -List -Platform x86)
$restricted = @($listed | Where-Object Profile -in @('Desktop', 'Lifecycle'))
if (!$restricted.Count) { throw 'The platform fixture selected no desktop/lifecycle groups.' }
foreach ($group in $restricted) {
    if ($group.Platforms -ne 'x64, ARM64') { throw "Inaccurate platform listing: $($group.Group)" }
    Expect-PlatformResult $group.Group x86 "*Platform x86 is not supported* $($group.Group).*"
}
Expect-PlatformResult PackagedApp x86 '*Platform x86 is not supported*PackagedApp*'
if (Test-Path -LiteralPath (Join-Path $fixture 'artifacts')) {
    throw 'An unsupported platform created artifacts or entered the build/test pipeline.'
}
foreach ($platform in @('x86', 'x64', 'ARM64')) {
    Expect-PlatformResult Editor $platform "*EXPECTED identity validation: App/$platform*"
}
foreach ($platform in @('x64', 'ARM64')) {
    Expect-PlatformResult AppLifecycle $platform "*EXPECTED identity validation: Lifecycle/$platform*"
    Expect-PlatformResult UIAutomation $platform "*EXPECTED identity validation: App/$platform*"
}
if (($listed | Where-Object Group -eq Editor).Platforms -ne 'x86, x64, ARM64' -or
    ($listed | Where-Object Group -eq PackagedApp).Platforms -ne 'x64') {
    throw 'The public platform listing disagrees with the actual preflight.'
}
$markdownTokens = $null
$markdownErrors = $null
$markdownAst = [Management.Automation.Language.Parser]::ParseFile(
    (Join-Path $repo 'scripts/Test-Markdown.ps1'), [ref]$markdownTokens, [ref]$markdownErrors)
if ($markdownErrors.Count) { throw 'The physical Markdown driver did not parse.' }
# Bind its real parameters without evaluating any desktop setup or starting a session.
$markdownBinding = [scriptblock]::Create($markdownAst.ParamBlock.Extent.Text + '; $Platform')
foreach ($platform in @('x86', 'x64', 'ARM64')) {
    if ((& $markdownBinding -Platform $platform) -cne $platform) { throw "The Markdown driver rejected $platform." }
    $checks.Add('Physical Markdown parameter binding: ' + $platform)
}
$checks | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'checks.json') -Encoding utf8
Write-Output "PASS platform preflight contracts: $($checks.Count) checks; no builds, package registration or UI."
