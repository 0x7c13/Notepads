# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Exercise real SDK evaluation and restore; no native build, deployment or desktop input.
param([Parameter(Mandatory)][string] $EvidenceRoot)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$null = New-Item -ItemType Directory -Path $EvidenceRoot -Force
$EvidenceRoot = [IO.Path]::GetFullPath($EvidenceRoot)
$checks = [Collections.Generic.List[string]]::new()
function Check([bool] $Condition, [string] $Name) {
    if (!$Condition) { throw $Name }
    $checks.Add($Name)
}
function Invoke-Probe([string] $Name, [string[]] $Arguments, [string] $ExpectedError = '') {
    $output = & dotnet msbuild $project /nologo @Arguments 2>&1
    $code = $LASTEXITCODE
    $text = $output -join "`n"
    [IO.File]::WriteAllText((Join-Path $EvidenceRoot ($Name + '.log')), $text)
    if ($ExpectedError) {
        Check ($code -ne 0 -and $text.Contains($ExpectedError)) $Name
    } elseif ($code -ne 0) { throw "$Name failed: $text" }
    return $text
}

$project = Join-Path $EvidenceRoot 'Probe.csproj'
$policyPath = [Security.SecurityElement]::Escape((Join-Path $repo 'scripts/NativeAot.targets'))
$projectText = @'
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net10.0</TargetFramework>
    <OutputType>Exe</OutputType>
    <PublishAot>true</PublishAot>
    <RuntimeIdentifier>win-x64</RuntimeIdentifier>
    <ManagePackageVersionsCentrally>false</ManagePackageVersionsCentrally>
  </PropertyGroup>
  <ItemGroup Condition="'$(ExplicitCompiler)' == 'true'">
    <PackageReference Include="Microsoft.DotNet.ILCompiler" Version="0.0.1" />
  </ItemGroup>
  <Import Project="POLICY_PATH" Condition="'$(WithoutPolicy)' != 'true'" />
</Project>
'@
[IO.File]::WriteAllText($project, $projectText.Replace('POLICY_PATH', $policyPath))
$query = @('/t:ProcessFrameworkReferences', '/getItem:PackageReference,RuntimePack,HostILCompilerPack,TargetILCompilerPack',
    '/getProperty:NETCoreSdkVersion,ILCompilerTargetsPath')
$fresh = Invoke-Probe 'fresh' $query | ConvertFrom-Json
$expected = @($fresh.Items.PackageReference | Where-Object Identity -EQ 'Microsoft.DotNet.ILCompiler')[0].Version
Check (![string]::IsNullOrEmpty($expected)) 'Fresh evaluation selects an SDK compiler'

# Reproduce the old package's SingleEntry.targets behavior without requiring an
# old SDK/package download. The current SDK supplies all real restore versions.
$staleDirectory = Join-Path $EvidenceRoot 'stale/0.0.1/build'
$null = New-Item -ItemType Directory -Path $staleDirectory -Force
$staleTargets = Join-Path $staleDirectory 'Microsoft.DotNet.ILCompiler.SingleEntry.targets'
[IO.File]::WriteAllText($staleTargets, @'
<Project>
  <ItemGroup>
    <KnownILCompilerPack Update="Microsoft.DotNet.ILCompiler">
      <ILCompilerPackVersion>0.0.1</ILCompilerPackVersion>
    </KnownILCompilerPack>
  </ItemGroup>
</Project>
'@)
$null = New-Item -ItemType Directory -Path (Join-Path $EvidenceRoot 'obj') -Force
$staleProps = '<Project><PropertyGroup><ILCompilerTargetsPath>' +
    [Security.SecurityElement]::Escape($staleTargets) + '</ILCompilerTargetsPath></PropertyGroup></Project>'
[IO.File]::WriteAllText((Join-Path $EvidenceRoot 'obj/Probe.csproj.nuget.g.props'), $staleProps)
$broken = Invoke-Probe 'negative-control' ($query + '/p:WithoutPolicy=true') | ConvertFrom-Json
Check (@($broken.Items.PackageReference | Where-Object Identity -EQ 'Microsoft.DotNet.ILCompiler')[0].Version -eq '0.0.1') 'Negative control reproduces stale implicit compiler selection'
Check (@($broken.Items.RuntimePack | Where-Object Identity -EQ 'Microsoft.NETCore.App.Runtime.win-x64')[0].NuGetPackageVersion -eq $expected) 'Negative control retains the newer CoreCLR runtime'

foreach ($rid in @('win-x86', 'win-x64', 'win-arm64')) {
    $fixed = Invoke-Probe $rid ($query + "/p:RuntimeIdentifier=$rid") | ConvertFrom-Json
    Check (@($fixed.Items.PackageReference | Where-Object Identity -EQ 'Microsoft.DotNet.ILCompiler')[0].Version -eq $expected) "$rid implicit compiler follows SDK"
    $packs = @($fixed.Items.HostILCompilerPack) + @($fixed.Items.TargetILCompilerPack)
    Check ($packs.Count -gt 0 -and @($packs | Where-Object NuGetPackageVersion -NE $expected).Count -eq 0) "$rid host and cross compiler packs agree"
}
$null = Invoke-Probe 'stale-build-refused' @('/t:PrepareForBuild') 'NOTEPADSBUILD001'
$null = Invoke-Probe 'explicit-compiler-refused' @('/t:ProcessFrameworkReferences', '/p:ExplicitCompiler=true') 'Remove the explicit Microsoft.DotNet.ILCompiler reference'
$disabled = Invoke-Probe 'non-aot' ($query + '/p:PublishAot=false') | ConvertFrom-Json
Check (@($disabled.Items.PackageReference | Where-Object Identity -EQ 'Microsoft.DotNet.ILCompiler').Count -eq 0) 'Non-AOT builds acquire no compiler dependency'

# One normal /restore invocation must both repair generated imports and evaluate
# again. No manual cache deletion, version pin or second restore is allowed.
$null = Invoke-Probe 'automatic-repair' @('/restore', '/t:PrepareForBuild', '/v:minimal')
$assets = Get-Content (Join-Path $EvidenceRoot 'obj/project.assets.json') -Raw | ConvertFrom-Json
Check ($null -ne $assets.libraries.PSObject.Properties["Microsoft.DotNet.ILCompiler/$expected"]) 'Restore downloads the SDK-selected compiler'
$repaired = Invoke-Probe 'repaired' $query | ConvertFrom-Json
$importedVersion = Split-Path (Split-Path (Split-Path $repaired.Properties.ILCompilerTargetsPath -Parent) -Parent) -Leaf
Check ($importedVersion -eq $expected) 'Next evaluation imports the matching compiler targets'
$null = Invoke-Probe 'repaired-build' @('/t:PrepareForBuild', '/v:minimal')
$checks | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'checks.json') -Encoding utf8
Write-Output "PASS: NativeAOT contracts ($($checks.Count) checks); evidence: $EvidenceRoot"
