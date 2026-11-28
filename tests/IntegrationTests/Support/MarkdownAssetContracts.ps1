# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

#requires -Version 5.1
# Run the real validator against private source, inventory, consumer and deployment fixtures.
param([string] $EvidenceRoot = '', [string] $ValidatorPath, [string] $ModuleAttributesPath)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
if (!$ValidatorPath) { $ValidatorPath = Join-Path $repo 'src/Notepads.Markdown/scripts/Test-FixedAssets.ps1' }
if (!$EvidenceRoot) { $EvidenceRoot = Join-Path $repo ('artifacts/markdown-asset-contracts/' + [Guid]::NewGuid().ToString('N')) }
$EvidenceRoot = [IO.Path]::GetFullPath($EvidenceRoot)
$module = Join-Path ([IO.Path]::GetFullPath($EvidenceRoot)) 'module'
$checks = [Collections.Generic.List[string]]::new()
foreach ($relative in @('build', 'third-party-notices/NativeSvg/fixture-1.0.0',
    'crates/markdown-core/tests', 'tests/fixtures/m1/inputs')) {
    [IO.Directory]::CreateDirectory((Join-Path $module $relative)) | Out-Null
}

function Write-Fixture([string] $Path, [string] $Text) {
    [IO.File]::WriteAllText($Path, $Text, [Text.UTF8Encoding]::new($false))
}
function Write-JsonFixture([string] $Path, $Value) {
    Write-Fixture $Path ($Value | ConvertTo-Json -Depth 8)
}
function Expect-AssetRejection([string] $Name, [string] $Message, [scriptblock] $Action) {
    $failure = $null
    try { & $Action | Out-Null } catch { $failure = $_ }
    if (!$failure -or $failure.Exception.Message -notlike $Message) {
        throw "$Name did not fail with the expected guard ($Message): $failure"
    }
    $checks.Add($Name)
}
function Invoke-AssetValidation([string] $Check = 'All', [string] $Layout = '') {
    # Fixed fixture identities, independent of each mutation below; canonical member rows are documented in the validator.
    & $ValidatorPath -ModuleRoot $module -Check $Check -DeploymentRoot $Layout -ExpectedRequiredCount 1 -ExpectedWorkloadCount 1 `
        -ExpectedPrimaryNoticeCount 2 -ExpectedPrimaryMembershipSha256 '27AF12405AB983523341069B17D22E947839D1AE2BDA603CD70955FB0862D7E0' `
        -ExpectedSupplementalPackageCount 1 -ExpectedSupplementalNoticeCount 1
}

$required = Join-Path $module 'tests/fixtures/m1/inputs/required.bin'
$workload = Join-Path $module 'tests/fixtures/m1/inputs/workload.bin'
Write-Fixture $required "abc`n"
Write-Fixture $workload "workload`n"
$indexPath = Join-Path $module 'crates/markdown-core/tests/source-baseline-index.tsv'
$index = "required`trequired`tinputs/required.bin`t4`t$((Get-FileHash -LiteralPath $required -Algorithm SHA256).Hash)`ttrue`n" +
    "workload`tworkload`tinputs/workload.bin`t9`t$((Get-FileHash -LiteralPath $workload -Algorithm SHA256).Hash)`ttrue`n"
Write-Fixture $indexPath $index
$records = @()
foreach ($name in @('LICENSE', 'COPYRIGHT')) {
    $relative = "third-party-notices/NativeSvg/fixture-1.0.0/$name"
    $path = Join-Path $module $relative
    Write-Fixture $path "fixed $name fixture`n"
    $records += [ordered]@{ path = $relative; upstream_path = "vendor/svg-bounded/dependencies/fixture-1.0.0/$name";
        bytes = (Get-Item -LiteralPath $path).Length; sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash }
}
$inventoryPath = Join-Path $module 'third-party-notices/NativeSvg/dependency-notices.json'
$inventory = @{ packages = @(@{ name = 'fixture'; version = '1.0.0'; notices = @($records) }) }
Write-JsonFixture $inventoryPath $inventory
$consumerPath = Join-Path $module 'build/Notepads.Markdown.Consumer.targets'
$consumer = '<Project><ItemGroup><Content Include="$(NotepadsMarkdownRoot)third-party-notices\**\*" ' +
    'Link="ThirdPartyNotices\Notepads.Markdown\%(RecursiveDir)%(Filename)%(Extension)" ' +
    'ExcludeFromResourceIndex="true" CopyToOutputDirectory="PreserveNewest" CopyToPublishDirectory="PreserveNewest" /></ItemGroup></Project>'
Write-Fixture $consumerPath $consumer
$supplementDirectory = Join-Path $module 'third-party-notices/NativeSvg/supplement-1.0.0'
[IO.Directory]::CreateDirectory($supplementDirectory) | Out-Null
$supplementNotice = Join-Path $supplementDirectory 'LICENSE'
Write-Fixture $supplementNotice "fixed supplemental fixture`n"
$supplementBytes = [IO.File]::ReadAllBytes($supplementNotice)
$registryChecksum = 'A' * 64
$supplementInventoryPath = Join-Path $module 'third-party-notices/NativeSvg/supplemental-inventory.json'
$supplementInventory = @{ format_version = 1; packages = @(@{ name = 'supplement'; version = '1.0.0';
    source = 'registry+https://github.com/rust-lang/crates.io-index'; registry_archive_sha256 = $registryChecksum;
    notices = @(@{ path = 'third-party-notices/NativeSvg/supplement-1.0.0/LICENSE';
    bytes = $supplementBytes.Length; sha256 = (Get-FileHash -LiteralPath $supplementNotice -Algorithm SHA256).Hash }) }) }
Write-JsonFixture $supplementInventoryPath $supplementInventory
$liveModule = Join-Path $repo 'src/Notepads.Markdown'
$noteFolders = @('MarkdownCore/code-syntaxes', 'MarkdownCore/front-matter-toml', 'EmojiShortcodes', 'Unicode17Grapheme')
foreach ($relative in @('crates/markdown-core/assets/code-syntaxes',
    'vendor/syntect/assets/default_newlines.packdump') + @($noteFolders | ForEach-Object { "third-party-notices/$_" })) {
    $destination = Join-Path $module $relative
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination))
    Copy-Item -LiteralPath (Join-Path $liveModule $relative) -Destination $destination -Recurse
}
$tomlInventory = Get-Content -LiteralPath (Join-Path $module 'third-party-notices/MarkdownCore/front-matter-toml/dependency-notices.json') -Raw | ConvertFrom-Json
$tomlLock = foreach ($package in $tomlInventory.packages) {
    "[[package]]`nname = `"$($package.name)`"`nversion = `"$($package.version)`"`nsource = `"registry+https://github.com/rust-lang/crates.io-index`"`nchecksum = `"$($package.registry_archive_sha256)`"`n"
}
$tomlLock = $tomlLock -join "`n"
$fixtureFork = Join-Path $module 'vendor/fixed-fork'
[void][IO.Directory]::CreateDirectory($fixtureFork)
Write-JsonFixture (Join-Path $module 'vendor/forks.json') @{
    schema = 'notepads.markdown.forks.v1'
    forks = @(@{ name = 'fixed-fork'; version = '1.0.0'; path = 'vendor/fixed-fork';
        upstream = @{ archive_sha256 = 'C' * 64 }; files = @{ modified = @(); added = @(); omitted = @() } })
}
Write-Fixture (Join-Path $module 'FORKS.md') "### fixed-fork 1.0.0`n`nRelative to the exact release: 0 modified files, 0 added files, 0 omitted files.`n"

$lockPath = Join-Path $module 'Cargo.lock'
$lock = "[[package]]`nname = `"supplement`"`nversion = `"1.0.0`"`nsource = `"registry+https://github.com/rust-lang/crates.io-index`"`nchecksum = `"$registryChecksum`"`n"
$lock += $tomlLock
Write-Fixture $lockPath $lock
Invoke-AssetValidation | Out-Null
$checks.Add('Fixed sources, copied original LICENSE/COPYRIGHT bodies and one notice glob are accepted')

$fixtureScripts = Join-Path $module 'scripts'
[IO.Directory]::CreateDirectory($fixtureScripts) | Out-Null
$fixtureValidator = Join-Path $fixtureScripts 'Test-FixedAssets.ps1'
Copy-Item -LiteralPath $ValidatorPath -Destination $fixtureValidator
$engines = [Collections.Generic.List[string]]::new()
$engineName = if ($PSVersionTable.PSEdition -eq 'Core') { 'pwsh.exe' } else { 'powershell.exe' }
$engines.Add((Join-Path $PSHOME $engineName))
if ($PSVersionTable.PSEdition -eq 'Core' -and $env:SystemRoot) {
    $windowsPowerShell = Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'
    if (Test-Path -LiteralPath $windowsPowerShell -PathType Leaf) { $engines.Add($windowsPowerShell) }
}
foreach ($engine in $engines) {
    $childLog = Join-Path $EvidenceRoot ('cli-default-root-' + [IO.Path]::GetFileNameWithoutExtension($engine) + '.log')
    # Exercise -File parameter binding with the actual validator under module/scripts.
    # Deliberately pass only fixture identities: ModuleRoot and ConsumerTargetsPath must default correctly.
    & $engine -NoProfile -ExecutionPolicy Bypass -File $fixtureValidator -ExpectedRequiredCount 1 -ExpectedWorkloadCount 1 `
        -ExpectedPrimaryNoticeCount 2 -ExpectedPrimaryMembershipSha256 '27AF12405AB983523341069B17D22E947839D1AE2BDA603CD70955FB0862D7E0' `
        -ExpectedSupplementalPackageCount 1 -ExpectedSupplementalNoticeCount 1 *> $childLog
    if ($LASTEXITCODE -ne 0 -or @([IO.File]::ReadAllLines($childLog)) -cnotcontains
        'PASS: Markdown fixed assets (2 source inputs; 3 distinct NativeSvg notices; check=All).') {
        throw "Validator -File default-root contract failed in $engine; child output: $childLog"
    }
    $checks.Add('Real -File defaults resolve the fixture module and consumer in ' + [IO.Path]::GetFileName($engine))
}

Write-Fixture $required "xyz`n"
Expect-AssetRejection 'Same-length source replacement' '*Fixed asset bytes changed:*' { Invoke-AssetValidation Sources }
Write-Fixture $required "abc`n"
[IO.File]::Move($required, $required + '.missing')
Expect-AssetRejection 'Missing source input' '*Fixed asset is missing:*' { Invoke-AssetValidation Sources }
[IO.File]::Move($required + '.missing', $required)
Write-Fixture $indexPath ($index.Split([char]10)[0] + "`n")
Expect-AssetRejection 'Removed fixed source membership' '*Fixed source membership changed:*' { Invoke-AssetValidation Sources }
Write-Fixture $indexPath ($index + $index.Split([char]10)[0] + "`n")
Expect-AssetRejection 'Duplicate fixed source row' '*Duplicate fixed source index row*' { Invoke-AssetValidation Sources }
Write-Fixture $indexPath $index

$syntaxManifest = Join-Path $module 'third-party-notices/MarkdownCore/code-syntaxes/syntax-assets.json'
$syntaxJson = [IO.File]::ReadAllText($syntaxManifest)
Write-Fixture $syntaxManifest '[]'
Expect-AssetRejection 'Empty code syntax inventory' '*code syntax inventory must cover exactly*' { Invoke-AssetValidation Sources }
$syntaxRecords = @($syntaxJson | ConvertFrom-Json)
$syntaxRecords[-1] = $syntaxRecords[0]
Write-JsonFixture $syntaxManifest $syntaxRecords
Expect-AssetRejection 'Duplicate code syntax inventory entry' '*code syntax inventory must cover exactly*' { Invoke-AssetValidation Licenses }
Write-Fixture $syntaxManifest $syntaxJson

$notice = Join-Path $module 'third-party-notices/NativeSvg/fixture-1.0.0/COPYRIGHT'
$noticeBytes = [IO.File]::ReadAllBytes($notice)
$mutated = [byte[]]$noticeBytes.Clone()
$mutated[0] = [byte][char]'X'
[IO.File]::WriteAllBytes($notice, $mutated)
Expect-AssetRejection 'Same-length copied original notice replacement' '*Fixed asset bytes changed:*' { Invoke-AssetValidation Licenses }
$changed = $inventory | ConvertTo-Json -Depth 8 | ConvertFrom-Json
$changed.packages[0].notices[1].sha256 = (Get-FileHash -LiteralPath $notice -Algorithm SHA256).Hash
Write-JsonFixture $inventoryPath $changed
Expect-AssetRejection 'Coordinated same-length body and inventory hash replacement' '*primary notice member identities changed*' { Invoke-AssetValidation Licenses }
[IO.File]::WriteAllBytes($notice, $noticeBytes)
Write-JsonFixture $inventoryPath $inventory
[IO.File]::Move($notice, $notice + '.missing')
Expect-AssetRejection 'Omitted copied COPYRIGHT body' '*Fixed asset is missing:*' { Invoke-AssetValidation Licenses }
[IO.File]::Move($notice + '.missing', $notice)
$changed = $inventory | ConvertTo-Json -Depth 8 | ConvertFrom-Json
$changed.packages[0].notices = @($changed.packages[0].notices[0])
Write-JsonFixture $inventoryPath $changed
Expect-AssetRejection 'Removed primary inventory member' '*primary notice membership changed:*' { Invoke-AssetValidation Licenses }
$changed = $inventory | ConvertTo-Json -Depth 8 | ConvertFrom-Json
$changed.packages[0].notices += $changed.packages[0].notices[0]
Write-JsonFixture $inventoryPath $changed
Expect-AssetRejection 'Duplicate primary inventory member' '*Duplicate NativeSvg notice member:*' { Invoke-AssetValidation Licenses }
$changed = $inventory | ConvertTo-Json -Depth 8 | ConvertFrom-Json
$changed.packages += $changed.packages[0]
Write-JsonFixture $inventoryPath $changed
Expect-AssetRejection 'Duplicate primary package identity' '*Duplicate NativeSvg notice package identity*' { Invoke-AssetValidation Licenses }
$changed = $inventory | ConvertTo-Json -Depth 8 | ConvertFrom-Json
$changed.packages[0].notices[1].path = 'third-party-notices/NativeSvg/fixture-1.0.0/NOTICE'
$changed.packages[0].notices[1].upstream_path = 'vendor/svg-bounded/dependencies/fixture-1.0.0/NOTICE'
[IO.File]::Move($notice, (Join-Path (Split-Path $notice -Parent) 'NOTICE'))
Write-JsonFixture $inventoryPath $changed
Expect-AssetRejection 'Same-count canonical notice member substitution' '*primary notice member identities changed*' { Invoke-AssetValidation Licenses }
[IO.File]::Move((Join-Path (Split-Path $notice -Parent) 'NOTICE'), $notice)
$changed = $inventory | ConvertTo-Json -Depth 8 | ConvertFrom-Json
$changed.packages[0].notices[1].path = 'third-party-notices/NativeSvg/fixture-1.0.0/../COPYRIGHT'
Write-JsonFixture $inventoryPath $changed
Expect-AssetRejection 'Notice destination traversal' '*Invalid NativeSvg notice member path:*' { Invoke-AssetValidation Licenses }
$changed = $inventory | ConvertTo-Json -Depth 8 | ConvertFrom-Json
$changed.packages[0].notices[1].upstream_path = 'vendor/../../COPYRIGHT'
Write-JsonFixture $inventoryPath $changed
Expect-AssetRejection 'Upstream provenance traversal' '*Invalid NativeSvg upstream notice path:*' { Invoke-AssetValidation Licenses }
Write-JsonFixture $inventoryPath $inventory

Write-Fixture $consumerPath '<Project />'
Expect-AssetRejection 'Omitted notice deployment glob' '*requires one third-party-notices content glob*' { Invoke-AssetValidation Licenses }
Write-Fixture $consumerPath $consumer.Replace('</ItemGroup>', ($consumer.Substring($consumer.IndexOf('<Content'), $consumer.IndexOf('/>') + 2 - $consumer.IndexOf('<Content')) + '</ItemGroup>'))
Expect-AssetRejection 'Duplicate notice deployment glob' '*requires one third-party-notices content glob*' { Invoke-AssetValidation Licenses }
Write-Fixture $consumerPath $consumer.Replace('CopyToOutputDirectory="PreserveNewest"', 'CopyToOutputDirectory="Never"')
Expect-AssetRejection 'Disabled notice output copying' '*deployment glob metadata changed*' { Invoke-AssetValidation Licenses }
Write-Fixture $consumerPath $consumer.Replace('CopyToPublishDirectory="PreserveNewest"', 'CopyToPublishDirectory="Never"')
Expect-AssetRejection 'Disabled notice publish copying' '*deployment glob metadata changed*' { Invoke-AssetValidation Licenses }
Write-Fixture $consumerPath $consumer.Replace('ThirdPartyNotices\Notepads.Markdown\%', 'WrongDestination\%')
Expect-AssetRejection 'Changed notice destination metadata' '*deployment glob metadata changed*' { Invoke-AssetValidation Licenses }
Write-Fixture $consumerPath $consumer.Replace('<ItemGroup>', '<ItemGroup Condition="false">')
Expect-AssetRejection 'Conditional notice deployment group' '*glob must be unconditional*' { Invoke-AssetValidation Licenses }
Write-Fixture $consumerPath $consumer.Replace('<Content ', '<Content Exclude="**\COPYRIGHT" ')
Expect-AssetRejection 'COPYRIGHT omitted by glob exclusion' '*deployment glob metadata changed*' { Invoke-AssetValidation Licenses }
Write-Fixture $consumerPath $consumer.Replace('</ItemGroup>', '<Content Include="extra" Link="ThirdPartyNotices\Notepads.Markdown\NativeSvg\fixture-1.0.0\COPYRIGHT" /></ItemGroup>')
Expect-AssetRejection 'Conflicting explicit notice copy declaration' '*duplicate or conflicting content route*' { Invoke-AssetValidation Licenses }
Write-Fixture $consumerPath $consumer.Replace('<Project>', '<Project><Import Project="Notepads.Markdown.Licenses.props" />')
Expect-AssetRejection 'Legacy duplicate license import' '*duplicate legacy license declarations*' { Invoke-AssetValidation Licenses }
Write-Fixture $consumerPath $consumer
$extraNotice = Join-Path (Split-Path $notice -Parent) 'EXTRA'
Write-Fixture $extraNotice 'unexpected notice body'
Expect-AssetRejection 'Unlisted source body included by real notice glob' '*contains an unlisted body:*' { Invoke-AssetValidation Licenses }
[IO.File]::Delete($extraNotice)

$supplementChanged = [byte[]]$supplementBytes.Clone()
$supplementChanged[0] = [byte][char]'X'
[IO.File]::WriteAllBytes($supplementNotice, $supplementChanged)
Expect-AssetRejection 'Same-length supplemental notice replacement' '*Fixed asset bytes changed:*' { Invoke-AssetValidation Licenses }
[IO.File]::WriteAllBytes($supplementNotice, $supplementBytes)
Write-Fixture $lockPath $lock.Replace($registryChecksum, ('B' * 64))
Expect-AssetRejection 'Supplemental registry checksum mismatch' '*registry identity differs from Cargo.lock:*' { Invoke-AssetValidation Licenses }
Write-Fixture $lockPath $lock.Replace('registry+https://github.com/rust-lang/crates.io-index', 'git+https://example.invalid/supplement')
Expect-AssetRejection 'Supplemental registry source mismatch' '*registry identity differs from Cargo.lock:*' { Invoke-AssetValidation Licenses }
Write-Fixture $lockPath ($lock + $lock)
Expect-AssetRejection 'Duplicate locked supplemental identity' '*not uniquely locked:*' { Invoke-AssetValidation Licenses }
Write-Fixture $lockPath $lock
$changed = $supplementInventory | ConvertTo-Json -Depth 8 | ConvertFrom-Json
$changed.packages[0].notices += $changed.packages[0].notices[0]
Write-JsonFixture $supplementInventoryPath $changed
Expect-AssetRejection 'Duplicate supplemental notice member' '*Duplicate NativeSvg notice member:*' { Invoke-AssetValidation Licenses }
Write-JsonFixture $supplementInventoryPath $supplementInventory

$changed = $supplementInventory | ConvertTo-Json -Depth 8 | ConvertFrom-Json
$changed.packages = @()
Write-JsonFixture $supplementInventoryPath $changed
Expect-AssetRejection 'Removed supplemental package membership' '*Supplemental SVG notice package membership changed*' { Invoke-AssetValidation Licenses }
$supplementCopyright = Join-Path $supplementDirectory 'COPYRIGHT'
Copy-Item -LiteralPath $supplementNotice -Destination $supplementCopyright
$changed = $supplementInventory | ConvertTo-Json -Depth 8 | ConvertFrom-Json
$extraRecord = $changed.packages[0].notices[0] | ConvertTo-Json | ConvertFrom-Json
$extraRecord.path = 'third-party-notices/NativeSvg/supplement-1.0.0/COPYRIGHT'
$changed.packages[0].notices += $extraRecord
Write-JsonFixture $supplementInventoryPath $changed
Expect-AssetRejection 'Added supplemental body membership' '*Supplemental SVG notice body membership changed:*' { Invoke-AssetValidation Licenses }
[IO.File]::Delete($supplementCopyright)
Write-JsonFixture $supplementInventoryPath $supplementInventory

$nestedNotice = Join-Path $supplementDirectory '.licenses/Contributor'
[IO.Directory]::CreateDirectory((Split-Path $nestedNotice -Parent)) | Out-Null
[IO.File]::Move($supplementNotice, $nestedNotice)
$changed = $supplementInventory | ConvertTo-Json -Depth 8 | ConvertFrom-Json
$changed.packages[0].notices[0].path = 'third-party-notices/NativeSvg/supplement-1.0.0/.licenses/Contributor'
Write-JsonFixture $supplementInventoryPath $changed
Invoke-AssetValidation Licenses | Out-Null
$checks.Add('Nested original supplemental contributor notices are accepted unchanged')
[IO.File]::Move($nestedNotice, $supplementNotice)
Write-JsonFixture $supplementInventoryPath $supplementInventory

$fork = Join-Path $module 'vendor/svg-bounded/fork/supplement-1.0.0/Cargo.toml'
[IO.Directory]::CreateDirectory((Split-Path $fork -Parent)) | Out-Null
Write-Fixture $fork "[package]`nname = `"supplement`"`nversion = `"1.0.0`"`n"
$changed = $supplementInventory | ConvertTo-Json -Depth 8 | ConvertFrom-Json
$changed.packages[0].source = 'vendor/svg-bounded/fork/supplement-1.0.0/Cargo.toml'
Write-JsonFixture $supplementInventoryPath $changed
Write-Fixture $lockPath ("[[package]]`nname = `"supplement`"`nversion = `"1.0.0`"`n" + $tomlLock)
Invoke-AssetValidation Licenses | Out-Null
$checks.Add('Local supplemental fork identity remains distinct from registry lock identity')
Write-Fixture $fork "[package]`nname = `"another`"`nversion = `"1.0.0`"`n"
Expect-AssetRejection 'Supplemental fork manifest identity mismatch' '*fork manifest identity changed:*' { Invoke-AssetValidation Licenses }
Write-JsonFixture $supplementInventoryPath $supplementInventory
Write-Fixture $lockPath $lock

$layout = Join-Path $EvidenceRoot 'layout'
$noticeDirectory = Join-Path $layout 'ThirdPartyNotices/Notepads.Markdown/NativeSvg/fixture-1.0.0'
[IO.Directory]::CreateDirectory($noticeDirectory) | Out-Null
foreach ($name in @('LICENSE', 'COPYRIGHT')) { Copy-Item -LiteralPath (Join-Path $module "third-party-notices/NativeSvg/fixture-1.0.0/$name") -Destination $noticeDirectory }
$supplementDeployed = Join-Path $layout 'ThirdPartyNotices/Notepads.Markdown/NativeSvg/supplement-1.0.0/LICENSE'
[IO.Directory]::CreateDirectory((Split-Path $supplementDeployed -Parent)) | Out-Null
Copy-Item -LiteralPath $supplementNotice -Destination $supplementDeployed
foreach ($folder in $noteFolders) {
    $destination = Join-Path $layout "ThirdPartyNotices/Notepads.Markdown/$folder"
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination))
    Copy-Item -LiteralPath (Join-Path $module "third-party-notices/$folder") -Destination $destination -Recurse
}
Invoke-AssetValidation Licenses $layout | Out-Null
$checks.Add('Every actual deployed notice body matches the fixed inventory')
$emojiDeployed = Join-Path $layout 'ThirdPartyNotices/Notepads.Markdown/EmojiShortcodes/LICENSE-UNICODE'
[IO.File]::Move($emojiDeployed, $emojiDeployed + '.missing')
Expect-AssetRejection 'Missing deployed emoji notice' '*Fixed asset is missing:*' { Invoke-AssetValidation Licenses $layout }
[IO.File]::Move($emojiDeployed + '.missing', $emojiDeployed)
$graphemeDeployed = Join-Path $layout 'ThirdPartyNotices/Notepads.Markdown/Unicode17Grapheme/Unicode-license.txt'
[IO.File]::Move($graphemeDeployed, $graphemeDeployed + '.missing')
Expect-AssetRejection 'Missing deployed Unicode grapheme notice' '*Fixed asset is missing:*' { Invoke-AssetValidation Licenses $layout }
[IO.File]::Move($graphemeDeployed + '.missing', $graphemeDeployed)
$deployed = Join-Path $noticeDirectory 'COPYRIGHT'
[IO.File]::WriteAllBytes($deployed, $mutated)
Expect-AssetRejection 'Same-length deployed notice replacement' '*Fixed asset bytes changed:*' { Invoke-AssetValidation Licenses $layout }
[IO.File]::WriteAllBytes($deployed, $noticeBytes)
[IO.File]::Move($deployed, $deployed + '.missing')
Expect-AssetRejection 'Missing deployed COPYRIGHT body' '*Fixed asset is missing:*' { Invoke-AssetValidation Licenses $layout }
[IO.File]::Move($deployed + '.missing', $deployed)
$extraDeployed = Join-Path $noticeDirectory 'EXTRA'
Write-Fixture $extraDeployed 'unexpected deployed body'
Expect-AssetRejection 'Extra deployed package body' '*Deployed NativeSvg notice directory contains an unlisted body:*' { Invoke-AssetValidation Licenses $layout }
[IO.File]::Delete($extraDeployed)
Invoke-AssetValidation All $layout | Out-Null
$checks.Add('Restored source, inventory, consumer and layout fixtures are accepted')

foreach ($nullNotices in @($true, $false)) {
    $changed = $inventory | ConvertTo-Json -Depth 8 | ConvertFrom-Json
    $emptyPackage = @{ name = 'empty'; version = '1.0.0'; notices = @() }
    if ($nullNotices) { $emptyPackage.notices = $null }
    $changed.packages += $emptyPackage
    Write-JsonFixture $inventoryPath $changed
    $case = if ($nullNotices) { 'Null notice package' } else { 'Empty notice package' }
    Expect-AssetRejection $case '*Invalid NativeSvg notice package identity.*' { Invoke-AssetValidation Licenses }
}
Write-JsonFixture $inventoryPath $inventory

$gitRoot = Join-Path $EvidenceRoot 'checkout-input'
$checkout = Join-Path $EvidenceRoot 'checkout-output'
$sourcePath = 'src/Notepads.Markdown/tests/fixtures/m1/inputs/cm.0001.bin'
$licensePath = 'src/Notepads.Markdown/third-party-notices/NativeSvg/fixture-1.0.0/COPYRIGHT'
$grammarPath = 'src/Notepads.Markdown/crates/markdown-core/assets/code-syntaxes/Example.sublime-syntax'
foreach ($relative in @($sourcePath, $licensePath, $grammarPath)) {
    [IO.Directory]::CreateDirectory((Split-Path (Join-Path $gitRoot $relative) -Parent)) | Out-Null
    Write-Fixture (Join-Path $gitRoot $relative) "frozen LF bytes`n"
}
Write-Fixture (Join-Path $gitRoot '.gitattributes') "* text=auto`r`n"
if (!$ModuleAttributesPath) { $ModuleAttributesPath = Join-Path (Split-Path $ValidatorPath -Parent) '../.gitattributes' }
$attributes = [IO.File]::ReadAllText($ModuleAttributesPath)
Write-Fixture (Join-Path $gitRoot 'src/Notepads.Markdown/.gitattributes') ($attributes.Replace("`r`n", "`n").Replace("`n", "`r`n"))
& git -C $gitRoot init --quiet
if ($LASTEXITCODE) { throw 'Private fixture git init failed.' }
& git -C $gitRoot -c core.autocrlf=true add -- .gitattributes src/Notepads.Markdown/.gitattributes $sourcePath $licensePath $grammarPath 2>$null
if ($LASTEXITCODE) { throw 'Private fixture git add failed.' }
[IO.Directory]::CreateDirectory($checkout) | Out-Null
& git -C $gitRoot -c core.autocrlf=true checkout-index --all --force ('--prefix=' + $checkout.Replace('\', '/') + '/')
if ($LASTEXITCODE) { throw 'Private fixture git checkout failed.' }
foreach ($relative in @($sourcePath, $licensePath, $grammarPath)) {
    if ((Get-FileHash -LiteralPath (Join-Path $gitRoot $relative) -Algorithm SHA256).Hash -cne
        (Get-FileHash -LiteralPath (Join-Path $checkout $relative) -Algorithm SHA256).Hash) {
        throw "Fixed asset checkout changed with core.autocrlf=true: $relative"
    }
}
$checks.Add('Actual core.autocrlf=true Git checkout preserves frozen input and copied notice bytes')
$checks | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'checks.json') -Encoding utf8
Write-Output "PASS: Markdown asset contracts ($($checks.Count) checks); private evidence: $EvidenceRoot"
