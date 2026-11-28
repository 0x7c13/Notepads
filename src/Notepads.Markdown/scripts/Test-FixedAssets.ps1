# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE file in the project root for license information.
# ---------------------------------------------------------------------------------------------

#requires -Version 5.1
[CmdletBinding()]
param(
    [ValidateSet('All', 'Sources', 'Licenses', 'Forks')][string] $Check = 'All',
    [string] $ModuleRoot,
    [string] $ConsumerTargetsPath,
    [string] $DeploymentRoot,
    [ValidateRange(0, 2147483647)][int] $ExpectedRequiredCount = 913,
    [ValidateRange(0, 2147483647)][int] $ExpectedWorkloadCount = 5,
    [ValidateRange(0, 2147483647)][int] $ExpectedPrimaryNoticeCount = 59,
    [string] $ExpectedPrimaryMembershipSha256 = '64C92F65C316D2919308293CAC3EC95025C5338F340D933BF8BE0E8E3EE3B75C',
    [ValidateRange(0, 2147483647)][int] $ExpectedSupplementalPackageCount = 21,
    [ValidateRange(0, 2147483647)][int] $ExpectedSupplementalNoticeCount = 41
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (!$ModuleRoot) { $ModuleRoot = Split-Path -Parent $PSScriptRoot }
$ModuleRoot = [IO.Path]::GetFullPath($ModuleRoot)
if (!$ConsumerTargetsPath) { $ConsumerTargetsPath = Join-Path $ModuleRoot 'build/Notepads.Markdown.Consumer.targets' }
if ($DeploymentRoot -and $Check -notin @('All', 'Licenses')) { throw '-DeploymentRoot requires the Licenses or All check.' }

# Primary membership is SHA-256 over ordinal-sorted UTF-8 rows, each followed by LF:
# canonical inventory path<TAB>decimal byte length<TAB>uppercase original SHA-256.
# This freezes primary notice membership without keeping a second notice list.

function Get-FixedAssetPath([string] $Root, [string] $Relative) {
    if (!$Relative -or [IO.Path]::IsPathRooted($Relative) -or $Relative.Contains(':') -or
        @($Relative -split '[\\/]' | Where-Object { $_ -in @('', '.', '..') }).Count) {
        throw "Invalid fixed asset path: $Relative"
    }
    $base = [IO.Path]::GetFullPath($Root).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $path = [IO.Path]::GetFullPath((Join-Path $base $Relative))
    if (!$path.StartsWith($base, [StringComparison]::OrdinalIgnoreCase)) { throw "Fixed asset path escapes its root: $Relative" }
    return $path
}

function Assert-FixedAsset([string] $Path, [long] $Bytes, [string] $Sha256) {
    if ($Bytes -lt 0 -or $Sha256 -cnotmatch '^[0-9A-Fa-f]{64}$') { throw "Invalid fixed asset identity: $Path" }
    if (!(Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Fixed asset is missing: $Path" }
    if ((Get-Item -LiteralPath $Path).Length -ne $Bytes -or
        (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ine $Sha256) {
        throw "Fixed asset bytes changed: $Path"
    }
}

function Test-FrozenSourceInputs {
    $index = Join-Path $ModuleRoot 'crates/markdown-core/tests/source-baseline-index.tsv'
    $fixtures = Join-Path $ModuleRoot 'tests/fixtures/m1'
    $ids = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    $paths = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $required = 0
    $workload = 0
    $number = 0
    foreach ($line in [IO.File]::ReadAllLines($index)) {
        $number++
        if (!$line -or $line.StartsWith('#')) { continue }
        $fields = $line.Split([char]9)
        if ($fields.Count -ne 6 -or $fields[0] -cnotin @('required', 'workload') -or
            $fields[1] -cnotmatch '^[A-Za-z0-9][A-Za-z0-9._-]*$' -or
            !$fields[2].StartsWith('inputs/', [StringComparison]::Ordinal) -or
            $fields[3] -cnotmatch '^(0|[1-9][0-9]*)$' -or
            $fields[4] -cnotmatch '^[0-9A-Fa-f]{64}$' -or $fields[5] -cnotin @('true', 'false')) {
            throw "Invalid fixed source index row $number."
        }
        $path = Get-FixedAssetPath $fixtures $fields[2]
        if (!$ids.Add($fields[1]) -or !$paths.Add($path)) { throw "Duplicate fixed source index row $number." }
        Assert-FixedAsset $path ([long]$fields[3]) $fields[4]
        if ($fields[0] -eq 'required') { $required++ } else { $workload++ }
    }
    if ($required -ne $ExpectedRequiredCount -or $workload -ne $ExpectedWorkloadCount -or !$paths.Count) {
        throw "Fixed source membership changed: expected $ExpectedRequiredCount required/$ExpectedWorkloadCount workload, found $required/$workload."
    }
    return $paths.Count
}


function Get-NoticeMembershipSha256($Records) {
    $rows = [Collections.Generic.List[string]]::new()
    foreach ($record in $Records) {
        $rows.Add($record.Relative + "`t" + $record.Bytes.ToString([Globalization.CultureInfo]::InvariantCulture) + "`t" + $record.Sha256.ToUpperInvariant())
    }
    $rows.Sort([StringComparer]::Ordinal)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [Text.Encoding]::UTF8.GetBytes([string]::Join("`n", $rows.ToArray()) + "`n")
        return ([BitConverter]::ToString($algorithm.ComputeHash($bytes))).Replace('-', '')
    } finally { $algorithm.Dispose() }
}

function Add-NoticeRecord($Expected, $Package, $Notice, [bool] $Primary) {
    $relative = [string]$Notice.path
    $prefix = 'third-party-notices/NativeSvg/' + $Package.name + '-' + $Package.version + '/'
    $name = [IO.Path]::GetFileName($relative)
    if (!$name -or !$relative.StartsWith($prefix, [StringComparison]::Ordinal) -or ($Primary -and $relative -cne ($prefix + $name))) {
        throw "Invalid NativeSvg notice member path: $relative"
    }
    $path = Get-FixedAssetPath $ModuleRoot $relative
    if ($Primary) {
        $upstream = [string]$Notice.upstream_path
        $origins = @('fork', 'dependencies', 'png') | ForEach-Object {
            'vendor/svg-bounded/' + $_ + '/' + $Package.name + '-' + $Package.version + '/' + $name
        }
        if ($upstream -cnotin $origins) {
            throw "Invalid NativeSvg upstream notice path: $upstream"
        }
        $null = Get-FixedAssetPath $ModuleRoot $upstream
    }
    if ($Expected.ContainsKey($relative)) { throw "Duplicate NativeSvg notice member: $relative" }
    Assert-FixedAsset $path ([long]$Notice.bytes) ([string]$Notice.sha256)
    $Expected.Add($relative, [pscustomobject]@{
        Relative = $relative; Bytes = [long]$Notice.bytes; Sha256 = [string]$Notice.sha256
    })
}

function Assert-NoticePackageIdentity($Package) {
    if ($Package.name -cnotmatch '^[a-z0-9][a-z0-9_-]*$' -or
        $Package.version -cnotmatch '^[0-9]+\.[0-9]+\.[0-9]+([-+][A-Za-z0-9.-]+)?$' -or
        $null -eq $Package.notices -or !@($Package.notices).Count) {
        throw 'Invalid NativeSvg notice package identity.'
    }
}

function Test-NativeSvgNotices($Expected) {
    $inventoryPath = Join-Path $ModuleRoot 'third-party-notices/NativeSvg/dependency-notices.json'
    $inventory = [IO.File]::ReadAllText($inventoryPath) | ConvertFrom-Json
    $packages = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($package in $inventory.packages) {
        Assert-NoticePackageIdentity $package
        if (!$packages.Add($package.name + '/' + $package.version)) { throw 'Duplicate NativeSvg notice package identity.' }
        foreach ($notice in $package.notices) { Add-NoticeRecord $Expected $package $notice $true }
    }
    if ($Expected.Count -ne $ExpectedPrimaryNoticeCount) {
        throw "NativeSvg primary notice membership changed: expected $ExpectedPrimaryNoticeCount, found $($Expected.Count)."
    }
    if ($ExpectedPrimaryMembershipSha256 -cnotmatch '^[0-9A-Fa-f]{64}$' -or
        (Get-NoticeMembershipSha256 $Expected.Values) -ine $ExpectedPrimaryMembershipSha256) {
        throw 'NativeSvg primary notice member identities changed.'
    }
    return $Expected.Count
}

function Test-SupplementalSvgNotices($Expected) {
    $inventoryPath = Join-Path $ModuleRoot 'third-party-notices/NativeSvg/supplemental-inventory.json'
    $inventory = [IO.File]::ReadAllText($inventoryPath) | ConvertFrom-Json
    if ($inventory.format_version -ne 1 -or @($inventory.packages).Count -ne $ExpectedSupplementalPackageCount) {
        throw 'Supplemental SVG notice package membership changed.'
    }
    $before = $Expected.Count
    $lock = [IO.File]::ReadAllText((Join-Path $ModuleRoot 'Cargo.lock'))
    $blocks = [regex]::Matches($lock, '(?ms)^\[\[package\]\]\r?\nname = "([^"]+)"\r?\nversion = "([^"]+)".*?(?=^\[\[package\]\]|\z)')
    $packages = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($package in $inventory.packages) {
        Assert-NoticePackageIdentity $package
        if ($package.registry_archive_sha256 -cnotmatch '^[0-9A-Fa-f]{64}$' -or
            !$packages.Add($package.name + '/' + $package.version)) {
            throw 'Invalid supplemental SVG notice package identity.'
        }
        $matching = @($blocks | Where-Object { $_.Groups[1].Value -ceq $package.name -and $_.Groups[2].Value -ceq $package.version })
        if ($matching.Count -ne 1) { throw "Supplemental SVG notice package is not uniquely locked: $($package.name)/$($package.version)" }
        $source = [regex]::Match($matching[0].Value, '(?m)^source = "([^"]+)"').Groups[1].Value
        $checksum = [regex]::Match($matching[0].Value, '(?m)^checksum = "([^"]+)"').Groups[1].Value
        if ($package.source -ceq 'registry+https://github.com/rust-lang/crates.io-index') {
            if ($source -cne $package.source -or $checksum -ine $package.registry_archive_sha256) {
                throw "Supplemental SVG notice registry identity differs from Cargo.lock: $($package.name)"
            }
        } else {
            $manifest = 'vendor/svg-bounded/fork/' + $package.name + '-' + $package.version + '/Cargo.toml'
            if ($package.source -cne $manifest -or $source -or $checksum) {
                throw "Supplemental SVG notice fork identity differs from Cargo.lock: $($package.name)"
            }
            $manifestPath = Get-FixedAssetPath $ModuleRoot $manifest
            $text = [IO.File]::ReadAllText($manifestPath)
            $packageBlock = [regex]::Match($text, '(?ms)^\[package\]\r?\n.*?(?=^\[|\z)').Value
            $name = [regex]::Match($packageBlock, '(?m)^name\s*=\s*"([^"]+)"').Groups[1].Value
            $version = [regex]::Match($packageBlock, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
            if ($name -cne $package.name -or $version -cne $package.version) { throw "Supplemental SVG notice fork manifest identity changed: $manifest" }
        }
        foreach ($notice in $package.notices) { Add-NoticeRecord $Expected $package $notice $false }
    }
    $count = $Expected.Count - $before
    if ($count -ne $ExpectedSupplementalNoticeCount) {
        throw "Supplemental SVG notice body membership changed: expected $ExpectedSupplementalNoticeCount, found $count."
    }
    return $count
}

function Get-ContentMetadata($Item, [string] $Name) {
    $children = @($Item.SelectNodes("*[local-name()='$Name']"))
    if ($children.Count -gt 1 -or ($Item.HasAttribute($Name) -and $children.Count)) { throw "Duplicate notice deployment metadata: $Name" }
    if ($children.Count) {
        if ($children[0].GetAttribute('Condition')) { throw "Conditional notice deployment metadata: $Name" }
        return $children[0].InnerText
    }
    return $Item.GetAttribute($Name)
}

function Test-NoticeDeployment($Expected) {
    [xml]$consumer = [IO.File]::ReadAllText($ConsumerTargetsPath)
    $include = '$(NotepadsMarkdownRoot)third-party-notices\**\*'
    $link = 'ThirdPartyNotices\Notepads.Markdown\%(RecursiveDir)%(Filename)%(Extension)'
    $content = @($consumer.SelectNodes("//*[local-name()='Content']"))
    $routes = @($content | Where-Object { $_.GetAttribute('Include').Replace('/', '\') -ceq $include })
    if ($routes.Count -ne 1) { throw 'NativeSvg notice deployment requires one third-party-notices content glob.' }
    $route = $routes[0]
    $ancestor = $route
    while ($ancestor -is [Xml.XmlElement]) {
        if ($ancestor.GetAttribute('Condition')) { throw 'NativeSvg notice deployment glob must be unconditional.' }
        $ancestor = $ancestor.ParentNode
    }
    if ($route.GetAttribute('Exclude') -or $route.GetAttribute('Remove') -or $route.GetAttribute('Update') -or
        (Get-ContentMetadata $route 'Link') -cne $link -or
        (Get-ContentMetadata $route 'ExcludeFromResourceIndex') -cne 'true' -or
        (Get-ContentMetadata $route 'CopyToOutputDirectory') -cne 'PreserveNewest' -or
        (Get-ContentMetadata $route 'CopyToPublishDirectory') -cne 'PreserveNewest') {
        throw 'NativeSvg notice deployment glob metadata changed.'
    }
    foreach ($item in $content) {
        if ([object]::ReferenceEquals($item, $route)) { continue }
        $itemLink = Get-ContentMetadata $item 'Link'
        if ($item.OuterXml -match 'third-party-notices' -or $itemLink.StartsWith('ThirdPartyNotices\Notepads.Markdown\NativeSvg\', [StringComparison]::OrdinalIgnoreCase)) {
            throw 'NativeSvg notice deployment has a duplicate or conflicting content route.'
        }
    }
    foreach ($import in $consumer.SelectNodes("//*[local-name()='Import']")) {
        if ([IO.Path]::GetFileName($import.GetAttribute('Project')) -ieq 'Notepads.Markdown.Licenses.props') {
            throw 'NativeSvg notice deployment still imports the duplicate legacy license declarations.'
        }
    }
    $noticeRoot = Join-Path $ModuleRoot 'third-party-notices/NativeSvg'
    $actual = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($directory in @(Get-ChildItem -LiteralPath $noticeRoot -Directory -Force)) {
        foreach ($file in @(Get-ChildItem -LiteralPath $directory.FullName -File -Recurse -Force)) {
            $relative = 'third-party-notices/NativeSvg/' + $file.FullName.Substring($noticeRoot.Length + 1).Replace('\', '/')
            if (!$actual.Add($relative) -or !$Expected.ContainsKey($relative)) { throw "NativeSvg notice directory contains an unlisted body: $relative" }
        }
    }
    if (!$actual.SetEquals([string[]]$Expected.Keys)) { throw 'NativeSvg notice directory does not cover the fixed inventory.' }
    if ($DeploymentRoot) {
        foreach ($record in $Expected.Values) {
            $destination = 'ThirdPartyNotices/Notepads.Markdown/' + $record.Relative.Substring('third-party-notices/'.Length)
            Assert-FixedAsset (Get-FixedAssetPath $DeploymentRoot $destination) $record.Bytes $record.Sha256
        }
        $deployedRoot = Get-FixedAssetPath $DeploymentRoot 'ThirdPartyNotices/Notepads.Markdown/NativeSvg'
        foreach ($directory in @(Get-ChildItem -LiteralPath $deployedRoot -Directory -Force)) {
            foreach ($file in @(Get-ChildItem -LiteralPath $directory.FullName -File -Recurse -Force)) {
                $relative = 'third-party-notices/NativeSvg/' + $file.FullName.Substring($deployedRoot.Length + 1).Replace('\', '/')
                if (!$Expected.ContainsKey($relative)) { throw "Deployed NativeSvg notice directory contains an unlisted body: $relative" }
            }
        }
    }
}

function Test-ForkLedger {
    $ledger = Get-Content -LiteralPath (Join-Path $ModuleRoot 'vendor/forks.json') -Raw | ConvertFrom-Json
    if ($ledger.schema -cne 'notepads.markdown.forks.v1' -or !$ledger.forks) {
        throw 'Invalid Markdown fork ledger.'
    }
    $documentation = Get-Content -LiteralPath (Join-Path $ModuleRoot 'FORKS.md') -Raw
    $counts = [Collections.Generic.Dictionary[string,string]]::new([StringComparer]::Ordinal)
    foreach ($match in [regex]::Matches($documentation,
        '(?m)^### (?<fork>[^\r\n]+)\r?\n(?:(?!^### )[\s\S])*?^Relative to the exact release: (?<counts>\d+ modified files, \d+ added files, \d+ omitted files)\.')) {
        $counts.Add($match.Groups['fork'].Value, $match.Groups['counts'].Value)
    }

    $paths = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $roots = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($fork in $ledger.forks) {
        $name = "$($fork.name) $($fork.version)"
        $root = Get-FixedAssetPath $ModuleRoot $fork.path
        if (!$fork.path.StartsWith('vendor/', [StringComparison]::Ordinal) -or !$roots.Add($root) -or
            !(Test-Path -LiteralPath $root -PathType Container) -or $fork.upstream.archive_sha256 -notmatch '^[0-9a-f]{64}$') {
            throw "Invalid fork identity: $name"
        }

        $files = $fork.files
        $expected = "$($files.modified.Count) modified files, $($files.added.Count) added files, $($files.omitted.Count) omitted files"
        if (!$counts.ContainsKey($name) -or $counts[$name] -cne $expected) {
            throw "Fork file counts in FORKS.md differ from vendor/forks.json: $name"
        }
        [void]$counts.Remove($name)
        foreach ($kind in @('modified', 'added', 'omitted')) {
            foreach ($entry in $files.$kind) {
                $path = Get-FixedAssetPath $root $entry.path
                if (!$paths.Add($path)) { throw "Duplicate fork ledger path: $path" }
                if ($kind -ne 'added' -and $entry.upstream_sha256 -notmatch '^[0-9a-f]{64}$') {
                    throw "Invalid upstream fork hash: $path"
                }
                if ($kind -eq 'omitted') {
                    if (Test-Path -LiteralPath $path) { throw "Omitted fork path is present: $path" }
                }
                elseif ($kind -eq 'added') {
                    Assert-FixedAsset $path $entry.bytes $entry.sha256
                }
                elseif (!(Test-Path -LiteralPath $path -PathType Leaf) -or
                    (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ine $entry.local_sha256) {
                    throw "Fork file is missing or its bytes differ from the ledger: $path"
                }
            }
        }
    }
    if ($counts.Count) { throw "FORKS.md lists forks missing from the ledger: $($counts.Keys -join ', ')" }
    Write-Output "PASS: Markdown fork ledger ($($roots.Count) forks; $($paths.Count) modified, added or omitted paths)."
}

function Test-DeployedNotice([string] $Relative, [long] $Bytes, [string] $Sha256) {
    if (!$DeploymentRoot) { return }
    $destination = 'ThirdPartyNotices/Notepads.Markdown/' + $Relative.Substring('third-party-notices/'.Length)
    $path = Get-FixedAssetPath $DeploymentRoot $destination
    # MSIX part names escape '+' in SemVer paths. Loose test layouts retain '+'.
    if ($destination.Contains('+') -and !(Test-Path -LiteralPath $path -PathType Leaf)) {
        $path = Get-FixedAssetPath $DeploymentRoot ($destination.Replace('+', '%2B'))
    }
    Assert-FixedAsset $path $Bytes $Sha256
}

function Test-CodeSyntaxAssets {
    $syntax = 'crates/markdown-core/assets/code-syntaxes'
    $notices = 'third-party-notices/MarkdownCore/code-syntaxes'
    $manifest = Get-FixedAssetPath $ModuleRoot "$notices/syntax-assets.json"
    $records = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
    $records = @($records)
    $expected = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    [void]$expected.Add('vendor/syntect/assets/default_newlines.packdump')
    foreach ($file in Get-ChildItem -LiteralPath (Get-FixedAssetPath $ModuleRoot $syntax) -File) {
        [void]$expected.Add("$syntax/$($file.Name)")
    }
    foreach ($file in Get-ChildItem -LiteralPath (Get-FixedAssetPath $ModuleRoot $notices) -Filter '*-LICENSE.txt' -File) {
        [void]$expected.Add("$notices/$($file.Name)")
    }
    if ($records.Count -ne 11 -or $expected.Count -ne 11 -or
        !$expected.SetEquals([string[]]@($records | ForEach-Object { $_.path }))) {
        throw 'Markdown code syntax inventory must cover exactly the fixed base, grammar, pack and license files.'
    }
    foreach ($entry in $records) {
        $path = Get-FixedAssetPath $ModuleRoot $entry.path
        Assert-FixedAsset $path $entry.bytes $entry.sha256
        if ($entry.path.StartsWith("$notices/", [StringComparison]::Ordinal)) {
            Test-DeployedNotice $entry.path $entry.bytes $entry.sha256
        }
    }
    Write-Output "PASS: Markdown code syntax inputs ($($records.Count) fixed grammars, packs, and license files)."
}

function Test-EmojiNotices {
    $directory = 'third-party-notices/EmojiShortcodes'
    $documentation = [IO.File]::ReadAllText((Get-FixedAssetPath $ModuleRoot "$directory/README.md"))
    $rows = [regex]::Matches($documentation, '(?m)^\| \[[^\]]+\]\((LICENSE-[A-Z]+)\) \| `([A-F0-9]{64})` \|\r?$')
    $expected = [Collections.Generic.HashSet[string]]::new([string[]]@(
        'LICENSE-APACHE', 'LICENSE-GEMOJI', 'LICENSE-MIT', 'LICENSE-UNICODE'), [StringComparer]::Ordinal)
    foreach ($row in $rows) {
        $name = $row.Groups[1].Value
        if (!$expected.Remove($name)) { throw "Unexpected emoji notice inventory member: $name" }
        $relative = "$directory/$name"
        $path = Get-FixedAssetPath $ModuleRoot $relative
        if (!(Test-Path -LiteralPath $path -PathType Leaf)) { throw "Fixed asset is missing: $path" }
        $bytes = (Get-Item -LiteralPath $path).Length
        Assert-FixedAsset $path $bytes $row.Groups[2].Value
        Test-DeployedNotice $relative $bytes $row.Groups[2].Value
    }
    if ($expected.Count) { throw 'Emoji notice inventory is incomplete.' }
}

function Test-TomlNotices {
    $directory = 'third-party-notices/MarkdownCore/front-matter-toml'
    $manifest = Get-FixedAssetPath $ModuleRoot "$directory/dependency-notices.json"
    $inventory = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
    $lock = [IO.File]::ReadAllText((Join-Path $ModuleRoot 'Cargo.lock'))
    $packages = [Collections.Generic.HashSet[string]]::new([string[]]@(
        'serde_spanned', 'toml', 'toml_datetime', 'toml_parser', 'winnow'), [StringComparer]::Ordinal)
    $paths = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($package in $inventory.packages) {
        if (!$packages.Remove($package.name)) { throw "Unexpected TOML notice package: $($package.name)" }
        $registryIdentity = '(?ms)^\[\[package\]\]\r?\nname = "' + [regex]::Escape($package.name) +
            '"\r?\nversion = "' + [regex]::Escape($package.version) +
            '"\r?\nsource = "registry\+https://github\.com/rust-lang/crates\.io-index"\r?\nchecksum = "([a-f0-9]{64})"'
        $locked = [regex]::Matches($lock, $registryIdentity)
        if ($locked.Count -ne 1 -or $locked[0].Groups[1].Value -ine $package.registry_archive_sha256) {
            throw "TOML notice registry identity differs from Cargo.lock: $($package.name)"
        }
        $prefix = "$directory/$($package.name)-$($package.version)/"
        foreach ($entry in $package.notices) {
            $relative = [string]$entry.path
            if (!$relative.StartsWith($prefix, [StringComparison]::Ordinal) -or !$paths.Add($relative)) {
                throw "Unexpected TOML notice inventory member: $relative"
            }
            Assert-FixedAsset (Get-FixedAssetPath $ModuleRoot $relative) $entry.bytes $entry.sha256
            Test-DeployedNotice $relative $entry.bytes $entry.sha256
        }
    }
    if ($packages.Count -or $paths.Count -ne 9) { throw 'TOML notice inventory is incomplete.' }
}

$sources = 0
$notices = 0
if ($Check -in @('All', 'Forks')) { Test-ForkLedger }
if ($Check -in @('All', 'Sources')) { $sources = Test-FrozenSourceInputs }
if ($Check -in @('All', 'Sources', 'Licenses')) { Test-CodeSyntaxAssets }
if ($Check -in @('All', 'Licenses')) {
    $expected = [Collections.Generic.Dictionary[string,object]]::new([StringComparer]::OrdinalIgnoreCase)
    $notices = (Test-NativeSvgNotices $expected) + (Test-SupplementalSvgNotices $expected)
    Test-NoticeDeployment $expected
    $graphemeNotice = 'third-party-notices/Unicode17Grapheme/Unicode-license.txt'
    $graphemeNoticeSha256 = 'E7A93B009565CFCE55919A381437AC4DB883E9DA2126FA28B91D12732BC53D96'
    Assert-FixedAsset (Get-FixedAssetPath $ModuleRoot $graphemeNotice) 1995 $graphemeNoticeSha256
    Test-DeployedNotice $graphemeNotice 1995 $graphemeNoticeSha256
    Test-EmojiNotices
    Test-TomlNotices
}
Write-Output "PASS: Markdown fixed assets ($sources source inputs; $notices distinct NativeSvg notices; check=$Check)."
