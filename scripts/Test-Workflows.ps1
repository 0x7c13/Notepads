# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

#requires -Version 7.4
param([string[]] $Workflow = @())
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
$platform = switch ($architecture) {
    'Arm64' { 'arm64' }
    'X64' { 'amd64' }
    default { throw "Workflow validation requires Windows x64 or ARM64; found $architecture." }
}
$version = '1.7.12'
$expectedHash = if ($platform -eq 'arm64') { 'cadcf7ea4efe3a68728893813643cebe1185e5b1d4be5b96245f65c9a4d5ea41' }
    else { '6e7241b51e6817ea6a047693d8e6fed13b31819c9a0dd6c5a726e1592d22f6e9' }
$toolRoot = Join-Path $repo ".tools/actionlint/$version/$platform"
$archive = Join-Path $toolRoot 'actionlint.zip'
$null = New-Item -ItemType Directory -Path $toolRoot -Force
if (!(Test-Path -LiteralPath $archive)) {
    Invoke-WebRequest "https://github.com/rhysd/actionlint/releases/download/v$version/actionlint_${version}_windows_$platform.zip" -OutFile $archive
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expectedHash) {
    throw "actionlint download checksum mismatch: $archive"
}
# Re-extract the verified archive so a stale or modified executable is never trusted.
Expand-Archive -LiteralPath $archive -DestinationPath $toolRoot -Force
if (!$Workflow.Count) {
    $Workflow = @(Get-ChildItem -LiteralPath (Join-Path $repo '.github/workflows') -File |
        Where-Object Extension -in '.yml', '.yaml' | ForEach-Object FullName)
}
if (!$Workflow.Count) { throw 'No workflows selected.' }
& (Join-Path $toolRoot 'actionlint.exe') -shellcheck= -pyflakes= @Workflow
if ($LASTEXITCODE -ne 0) { throw "Workflow validation failed (exit $LASTEXITCODE)." }
Write-Output "PASS: actionlint validated $($Workflow.Count) workflow(s)."
