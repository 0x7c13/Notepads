# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

#requires -Version 7.4
param([string] $RepoRoot = (Join-Path $PSScriptRoot '..'))
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$RepoRoot = (Resolve-Path -LiteralPath $RepoRoot).Path
$sourceExtensions = @('.cs', '.c', '.cpp', '.cxx', '.h', '.hpp', '.idl', '.inc', '.ps1', '.psm1')
$xmlExtensions = @('.xaml', '.csproj', '.vcxproj', '.props', '.targets')

# NUL separation preserves spaces, quotes and non-ASCII filenames. Include new files
# before git add, and tracked files even if a later ignore rule matches them.
$gitRoot = & git -C $RepoRoot rev-parse --show-toplevel
if ($LASTEXITCODE -ne 0 -or !$gitRoot -or [IO.Path]::GetFullPath($gitRoot) -ne $RepoRoot) {
    throw "Header check requires a Git working-tree root: $RepoRoot"
}
$raw = & git -C $RepoRoot -c core.quotepath=false ls-files -z --cached --others --exclude-standard
if ($LASTEXITCODE -ne 0) { throw "Cannot enumerate Git files under $RepoRoot." }
$files = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
foreach ($path in (($raw -join "`n").Split([char]0, [StringSplitOptions]::RemoveEmptyEntries))) {
    if ([IO.File]::Exists((Join-Path $RepoRoot $path))) { $null = $files.Add($path) }
}
if (!$files.Count) { throw 'Header check selected no repository files.' }
$policyPath = 'tests/IntegrationTests/Support/HeaderPolicy.json'
if (!$files.Contains($policyPath)) { throw "Header policy is missing or ignored: $policyPath" }
$policy = Get-Content -LiteralPath (Join-Path $RepoRoot $policyPath) -Raw | ConvertFrom-Json
$registered = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$preserved = [Collections.Generic.Dictionary[string,object]]::new([StringComparer]::Ordinal)
$ownedXml = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$licenses = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$errors = [Collections.Generic.List[string]]::new()

function Register-Path([string] $Path) {
    if (!$Path -or $Path -match '[:\\*?\[\]]' -or $Path.StartsWith('/') -or
        @($Path.Split('/') | Where-Object { $_ -in @('', '.', '..') }).Count) {
        throw "Policy requires an exact repository-relative path: $Path"
    }
    if (!$registered.Add($Path)) { throw "Duplicate header policy path: $Path" }
    if (!$files.Contains($Path)) { throw "Stale header policy path (missing or ignored): $Path" }
}

function Read-Text([string] $Path) {
    # StreamReader accepts a BOM. Only newline encoding is normalized for hashes;
    # indentation, trailing spaces and all protected notice text remain significant.
    [IO.File]::ReadAllText((Join-Path $RepoRoot $Path), [Text.UTF8Encoding]::new($false, $true)).Replace("`r`n", "`n")
}

function Assert-Hash([string] $Path, [string] $Text, [string] $Expected) {
    if ($Expected -cnotmatch '^[0-9A-F]{64}$') { throw "Invalid SHA256 in header policy: $Path" }
    $actual = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($Text)))
    if ($actual -cne $Expected) { $errors.Add("${Path}: preserved notice/license changed; review its attribution before updating the policy.") }
}

foreach ($license in $policy.licenses) {
    Register-Path $license.path
    $null = $licenses.Add($license.path)
    Assert-Hash $license.path (Read-Text $license.path) $license.sha256
}
if (!$licenses.Contains('LICENSE.txt')) { throw 'Header policy must protect the root LICENSE.txt.' }
foreach ($path in $policy.ownedXml) {
    Register-Path $path
    if ([IO.Path]::GetExtension($path) -notin $xmlExtensions) { throw "Invalid XML notice path: $path" }
    $null = $ownedXml.Add($path)
}
foreach ($group in $policy.preserved) {
    if ([string]::IsNullOrWhiteSpace($group.reason) -or !$licenses.Contains($group.license)) {
        throw 'Each preserved-header group needs a reason and a protected license.'
    }
    foreach ($header in $group.headers) {
        if ($header.lines -isnot [long] -and $header.lines -isnot [int]) { throw 'Header line count must be an integer.' }
        if ($header.lines -lt 1 -or $header.lines -gt 256 -or !$header.paths.Count) { throw 'Invalid preserved-header range.' }
        foreach ($path in $header.paths) {
            Register-Path $path
            if ([IO.Path]::GetExtension($path) -notin ($sourceExtensions + $xmlExtensions)) {
                throw "Preserved header outside source/XML scope: $path"
            }
            $preserved.Add($path, $header)
        }
    }
    foreach ($path in $group.withoutHeader) {
        Register-Path $path
        if ([IO.Path]::GetExtension($path) -notin $sourceExtensions) { throw "No-header exception outside source scope: $path" }
        $preserved.Add($path, $null)
    }
}

$ownCount = 0
$separator = '-' * 93
foreach ($path in ($files | Sort-Object)) {
    $extension = [IO.Path]::GetExtension($path)
    if ($extension -notin $sourceExtensions -and !$ownedXml.Contains($path) -and !$preserved.ContainsKey($path)) { continue }
    $text = Read-Text $path
    $lines = $text.Split("`n")
    if ($preserved.ContainsKey($path)) {
        $header = $preserved[$path]
        if ($null -eq $header) {
            # These exact WinUIEdit files originally had no per-file notice. Adding
            # a leading comment requires review/removal of the old exception.
            if ($text -match '\A\s*(?://|/\*|<!--)') { $errors.Add("${path}: no-header exception is stale; review the new leading comment.") }
        }
        elseif ($lines.Length -lt $header.lines) { $errors.Add("${path}: preserved notice is truncated.") }
        else { Assert-Hash $path ($lines[0..($header.lines - 1)] -join "`n") $header.sha256 }
        continue
    }
    $ownCount++
    $xml = $ownedXml.Contains($path)
    $prefix = if ($xml) { '' } elseif ($extension -in @('.ps1', '.psm1')) { '#' } else { '//' }
    $yearLine = [regex]::Escape("$prefix  Copyright (c) ") + '(?<from>\d{4})(?:-(?<to>\d{4}))?, Jiaqi \(0x7c13\) Liu\. All rights reserved\.'
    $validYear = $lines.Length -ge 2 -and $lines[1] -cmatch "^$yearLine`$"
    if ($validYear -and $Matches.ContainsKey('to') -and [int]$Matches.from -gt [int]$Matches.to) { $validYear = $false }
    $opening = if ($xml) { '<!--' } else { "$prefix $separator" }
    $closing = if ($xml) { '-->' } else { "$prefix $separator" }
    $licenseLine = "$prefix  See LICENSE.txt in the project root for license information."
    if ($lines.Length -lt 4 -or $lines[0] -cne $opening -or !$validYear -or
        $lines[2] -cne $licenseLine -or $lines[3] -cne $closing) {
        $errors.Add("${path}:1: expected the canonical copyright banner (two spaces, original year(s), owner and LICENSE.txt).")
    }
    elseif ($lines.Length -lt 6 -or $lines[4] -cne '' -or ($lines.Length -gt 6 -and $lines[5] -ceq '')) {
        $errors.Add("${path}:5: expected exactly one empty line after the copyright banner.")
    }
}
if (!$ownCount) { throw 'Header check selected no first-party source files.' }
if ($errors.Count) { throw ("Header check failed ($($errors.Count) violations):`n" + ($errors -join "`n")) }
Write-Output ("PASS Headers: {0} canonical files, {1} preserved exceptions, {2} license files." -f
    $ownCount, $preserved.Count, $licenses.Count)
