# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

#requires -Version 7.4
param([string] $EvidenceRoot = '')
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$checker = Join-Path $repo 'scripts/Test-Headers.ps1'
if (!$EvidenceRoot) { $EvidenceRoot = Join-Path $repo ('artifacts/header-contracts/' + [Guid]::NewGuid().ToString('N')) }
New-Item -ItemType Directory -Path $EvidenceRoot -Force | Out-Null
$caseCount = 0
$banner = @'
// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

'@.Replace("`r`n", "`n") + "`n"
$code = $banner + 'class Example { }'
$foreign = "// Copyright 2001, Other Author.`n// Licensed under this fixture's MIT license."

function Write-Fixture([string] $Path, [string] $Text, [bool] $Bom = $false) {
    $full = Join-Path $script:fixture $Path
    New-Item -ItemType Directory -Path (Split-Path $full) -Force | Out-Null
    [IO.File]::WriteAllText($full, $Text, [Text.UTF8Encoding]::new($Bom))
}
function Digest([string] $Text) {
    [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($Text)))
}
function Invoke-Contract([string] $Name, [string] $ExpectedError, [scriptblock] $Change) {
    $script:caseCount++
    $script:fixture = Join-Path $EvidenceRoot $Name
    New-Item -ItemType Directory -Path $script:fixture -Force | Out-Null
    & git -C $script:fixture init -q
    if ($LASTEXITCODE) { throw 'Cannot initialize private header fixture.' }
    Write-Fixture 'LICENSE.txt' "Fixture MIT license.`n"
    Write-Fixture 'own.cs' $code
    Write-Fixture 'vendor/native.cpp' ($foreign + "`n`nint value = 1;")
    Write-Fixture 'vendor/no-notice.h' '#pragma once'
    $script:policy = @{
        ownedXml = @()
        licenses = @(@{ path = 'LICENSE.txt'; sha256 = (Digest "Fixture MIT license.`n") })
        preserved = @(@{
            reason = 'Fixture upstream notice is preserved.'
            license = 'LICENSE.txt'
            headers = @(@{ lines = 2; sha256 = (Digest $foreign); paths = @('vendor/native.cpp') })
            withoutHeader = @('vendor/no-notice.h')
        })
    }
    & $Change
    Write-Fixture 'tests/IntegrationTests/Support/HeaderPolicy.json' ($script:policy | ConvertTo-Json -Depth 8)
    $failure = $null
    try {
        # Deliberately run from outside the fixture: the checker must honor -RepoRoot.
        $result = & $checker -RepoRoot $script:fixture
        $result | Set-Content (Join-Path $script:fixture 'check.log') -Encoding utf8
    }
    catch { $failure = $_.Exception.Message; $failure | Set-Content (Join-Path $script:fixture 'check.log') -Encoding utf8 }
    if ($ExpectedError) {
        if (!$failure -or !$failure.Contains($ExpectedError)) {
            throw "Header contract '$Name' expected '$ExpectedError', got '$failure'."
        }
    }
    elseif ($failure) { throw "Header contract '$Name' unexpectedly failed: $failure" }
}

Invoke-Contract 'canonical-lf' '' {}
Invoke-Contract 'bom-crlf' '' { Write-Fixture 'own.cs' ($code.Replace("`n", "`r`n")) $true }
Invoke-Contract 'single-year' '' { Write-Fixture 'own.cs' ($code.Replace('2020-2026', '2019')) }
Invoke-Contract 'header-only' '' { Write-Fixture 'own.cs' $banner }
Invoke-Contract 'powershell' '' { Write-Fixture 'task.ps1' ($code.Replace('//', '#').Replace('class Example { }', 'param()')) }
Invoke-Contract 'native-header' '' { Write-Fixture 'own.hpp' ($banner + '#pragma once') }
Invoke-Contract 'unicode-and-spaces' '' { Write-Fixture '目录/a b.cs' $code }
Invoke-Contract 'missing' 'own.cs:1:' { Write-Fixture 'own.cs' 'class Example { }' }
Invoke-Contract 'leading-indent' 'own.cs:1:' { Write-Fixture 'own.cs' (' ' + $code) }
Invoke-Contract 'leading-blank' 'own.cs:1:' { Write-Fixture 'own.cs' ("`n" + $code) }
Invoke-Contract 'content-indent' 'own.cs:1:' { Write-Fixture 'own.cs' ($code.Replace('//  Copyright', '// Copyright')) }
Invoke-Contract 'separator-width' 'own.cs:1:' { Write-Fixture 'own.cs' ($code.Replace('---', '--')) }
Invoke-Contract 'wrong-owner' 'own.cs:1:' { Write-Fixture 'own.cs' ($code.Replace('Jiaqi', 'Someone')) }
Invoke-Contract 'wrong-license' 'own.cs:1:' { Write-Fixture 'own.cs' ($code.Replace('LICENSE.txt', 'LICENSE')) }
Invoke-Contract 'year-order' 'own.cs:1:' { Write-Fixture 'own.cs' ($code.Replace('2020-2026', '2026-2020')) }
Invoke-Contract 'year-syntax' 'own.cs:1:' { Write-Fixture 'own.cs' ($code.Replace('2020-2026', '20-26')) }
Invoke-Contract 'trailing-space' 'own.cs:1:' { Write-Fixture 'own.cs' ($code.Replace('reserved.', 'reserved. ')) }
Invoke-Contract 'missing-blank' 'own.cs:5:' { Write-Fixture 'own.cs' ($code.Replace("`n`n", "`n")) }
Invoke-Contract 'extra-blank' 'own.cs:5:' { Write-Fixture 'own.cs' ($code.Replace("`n`n", "`n`n`n")) }
Invoke-Contract 'space-in-blank' 'own.cs:5:' { Write-Fixture 'own.cs' ($code.Replace("`n`n", "`n `n")) }
Invoke-Contract 'directive-first' 'own.cs:1:' { Write-Fixture 'own.cs' ("#nullable enable`n" + $code) }
Invoke-Contract 'untracked-source' 'new.cs:1:' { Write-Fixture 'new.cs' 'class New { }' }
Invoke-Contract 'ignored-output' '' { Write-Fixture '.gitignore' "ignored/`n"; Write-Fixture 'ignored/output.g.cs' 'generated body' }
Invoke-Contract 'tracked-ignore' 'ignored/tracked.cs:1:' {
    Write-Fixture '.gitignore' "ignored/`n"; Write-Fixture 'ignored/tracked.cs' 'class Tracked { }'
    & git -C $script:fixture add -f -- ignored/tracked.cs
    if ($LASTEXITCODE) { throw 'Cannot stage private fixture.' }
}
Invoke-Contract 'deleted-source' '' {
    Write-Fixture 'deleted.cs' 'bad header'
    & git -C $script:fixture add -- deleted.cs
    if ($LASTEXITCODE) { throw 'Cannot stage private fixture.' }
    [IO.File]::Delete((Join-Path $script:fixture 'deleted.cs'))
}
Invoke-Contract 'vendor-body-change' '' { Write-Fixture 'vendor/native.cpp' ($foreign + "`n`nint changed = 2;") }
Invoke-Contract 'vendor-bom-crlf' '' { Write-Fixture 'vendor/native.cpp' ($foreign.Replace("`n", "`r`n") + "`r`nint changed;") $true }
Invoke-Contract 'vendor-notice-change' 'vendor/native.cpp: preserved notice/license changed' {
    Write-Fixture 'vendor/native.cpp' ($foreign.Replace('Other Author', 'Wrong Author') + "`nint changed;")
}
Invoke-Contract 'vendor-notice-removed' 'vendor/native.cpp: preserved notice is truncated' { Write-Fixture 'vendor/native.cpp' 'int changed;' }
Invoke-Contract 'vendor-no-blanket-exception' 'vendor/new.cpp:1:' { Write-Fixture 'vendor/new.cpp' ($foreign + "`nint changed;" ) }
Invoke-Contract 'vendor-deleted-exception' 'Stale header policy path' { [IO.File]::Delete((Join-Path $script:fixture 'vendor/native.cpp')) }
Invoke-Contract 'vendor-ignored-exception' 'Stale header policy path' { Write-Fixture '.gitignore' 'vendor/native.cpp' }
Invoke-Contract 'duplicate-policy-path' 'Duplicate header policy path' { $script:policy.preserved[0].headers[0].paths += 'vendor/native.cpp' }
Invoke-Contract 'invalid-policy-path' 'exact repository-relative path' { $script:policy.preserved[0].headers[0].paths = @('../external.cpp') }
Invoke-Contract 'wildcard-policy-path' 'exact repository-relative path' { $script:policy.preserved[0].headers[0].paths = @('vendor/*.cpp') }
Invoke-Contract 'policy-reason' 'needs a reason' { $script:policy.preserved[0].reason = '' }
Invoke-Contract 'unprotected-license' 'needs a reason' { $script:policy.preserved[0].license = 'other-license' }
Invoke-Contract 'no-header-became-stale' 'no-header exception is stale' { Write-Fixture 'vendor/no-notice.h' ($foreign + "`n#pragma once") }
Invoke-Contract 'body-notice-is-not-header' '' { Write-Fixture 'vendor/no-notice.h' "#pragma once`n// Copyright in body." }
Invoke-Contract 'license-removed' 'Stale header policy path' { [IO.File]::Delete((Join-Path $script:fixture 'LICENSE.txt')) }
Invoke-Contract 'license-changed' 'LICENSE.txt: preserved notice/license changed' { Write-Fixture 'LICENSE.txt' 'changed terms' }
Invoke-Contract 'invalid-hash' 'Invalid SHA256' { $script:policy.preserved[0].headers[0].sha256 = 'bad' }
Invoke-Contract 'invalid-range' 'Invalid preserved-header range' { $script:policy.preserved[0].headers[0].lines = 0 }
Invoke-Contract 'policy-ignored' 'Header policy is missing or ignored' {
    Write-Fixture '.gitignore' 'tests/IntegrationTests/Support/HeaderPolicy.json'
}
Invoke-Contract 'only-foreign-files' 'selected no first-party source files' {
    [IO.File]::Delete((Join-Path $script:fixture 'own.cs'))
}
Invoke-Contract 'owned-xml' '' {
    $script:policy.ownedXml = @('project.props')
    Write-Fixture 'project.props' "<!--`n  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.`n  See LICENSE.txt in the project root for license information.`n-->`n`n<Project />"
}
Invoke-Contract 'owned-xml-deleted-header' 'project.props:1:' {
    $script:policy.ownedXml = @('project.props'); Write-Fixture 'project.props' '<Project />'
}
Invoke-Contract 'foreign-xml-declaration' '' {
    $notice = "<?xml version=`"1.0`" encoding=`"utf-8`"?>`n<!-- Copyright Other Author. -->"
    $script:policy.preserved[0].headers += @{ lines = 2; sha256 = (Digest $notice); paths = @('vendor/project.props') }
    Write-Fixture 'vendor/project.props' ($notice + "`n<Project />")
}
# Check the actual policy too: these upstream files place indented notices after
# a block comment. A short hash range once protected only their description.
$actualPolicy = Get-Content (Join-Path $PSScriptRoot 'HeaderPolicy.json') -Raw | ConvertFrom-Json
foreach ($suffix in @('cxx', 'h')) {
    $actualPath = "src/Notepads.Native/scintilla/winui/ScintillaWin.$suffix"
    $actualHeader = @($actualPolicy.preserved.headers | Where-Object { $actualPath -in $_.paths })
    if ($actualHeader.Count -ne 1) { throw "Missing unique notice policy for $actualPath" }
    foreach ($credit in @('upstream', 'modification')) {
        Invoke-Contract "actual-scintilla-$suffix-$credit" 'vendor/native.cpp: preserved notice/license changed' {
            $content = [IO.File]::ReadAllText((Join-Path $repo $actualPath))
            $name = if ($credit -eq 'upstream') { 'Neil Hodgson' } else { 'Jiaqi (0x7c13) Liu' }
            if (!$content.Contains($name)) { throw "Actual notice no longer contains $name; update the rejection fixture." }
            $script:policy.preserved[0].headers[0].lines = $actualHeader[0].lines
            $script:policy.preserved[0].headers[0].sha256 = $actualHeader[0].sha256
            Write-Fixture 'vendor/native.cpp' ($content.Replace($name, 'WRONG ATTRIBUTION'))
        }
    }
}
Invoke-Contract 'runner-propagates-failure' 'own.cs:1:' {
    Write-Fixture 'own.cs' 'class MissingHeader { }'
    foreach ($name in @('Test.ps1', 'Test-Headers.ps1', 'TestBuildIdentity.ps1')) {
        Write-Fixture "scripts/$name" ([IO.File]::ReadAllText((Join-Path $repo "scripts/$name")))
    }
    Write-Fixture 'tests/IntegrationTests/Support/HeaderPolicy.json' ($script:policy | ConvertTo-Json -Depth 8)
    $runnerLog = Join-Path $script:fixture 'runner.log'
    & (Get-Process -Id $PID).Path -NoProfile -NonInteractive -File (Join-Path $script:fixture 'scripts/Test.ps1') `
        -Suite Integration -Group Headers *> $runnerLog
    if ($LASTEXITCODE -eq 0 -or !(Get-Content -LiteralPath $runnerLog -Raw).Contains('own.cs:1:')) {
        throw 'Runner did not exit with the actual header failure.'
    }
    $reports = @(Get-ChildItem -LiteralPath (Join-Path $script:fixture 'artifacts/test-runs') -Filter results.json -Recurse)
    if ($reports.Count -ne 1) { throw 'Runner did not publish exactly one report.' }
    $report = Get-Content -LiteralPath $reports[0].FullName -Raw | ConvertFrom-Json
    if ($report.Name -ne 'Headers' -or $report.Status -ne 'FAIL') { throw 'Runner did not record Headers as FAIL.' }
}
$notRoot = Join-Path $EvidenceRoot 'not-a-git-root'
New-Item -ItemType Directory -Path $notRoot -Force | Out-Null
$rootFailure = $null
try { & $checker -RepoRoot $notRoot | Out-Null }
catch { $rootFailure = $_.Exception.Message }
if (!$rootFailure -or !$rootFailure.Contains('requires a Git working-tree root')) { throw 'Checker silently used an ancestor repository.' }
$caseCount++
Write-Output "PASS Header contracts: $caseCount cases (real Git enumeration and checker, including rejection paths)."
