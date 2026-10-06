# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE file in the project root for license information.
# ---------------------------------------------------------------------------------------------

param([string] $BackupRoot)
$ErrorActionPreference = 'Stop'
$repositoryPath = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (!$BackupRoot) {
    $BackupRoot = Join-Path (Split-Path $repositoryPath -Parent) ((Split-Path $repositoryPath -Leaf) + '-backups')
}

$commit = (& git -C $repositoryPath rev-parse HEAD).Trim()
if ($LASTEXITCODE) { throw 'Cannot identify the source commit.' }
$status = @(& git -C $repositoryPath status --porcelain)
if ($LASTEXITCODE -or $status.Count) { throw 'Commit reviewed source before making its verified backup.' }
$sourceTree = @(& git -C $repositoryPath -c core.quotepath=false ls-tree -r $commit)
if ($LASTEXITCODE) { throw 'Cannot enumerate the committed source tree.' }
if ($sourceTree | Where-Object { $_.StartsWith('160000 ') }) { throw 'This backup expects native source directly in the repository.' }

$backupPath = Join-Path $BackupRoot ('v2-source-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + $commit.Substring(0, 10))
New-Item -ItemType Directory -Path $backupPath -ErrorAction Stop | Out-Null
$bundlePath = Join-Path $backupPath 'Notepads.bundle'
& git -C $repositoryPath bundle create $bundlePath --all HEAD
if ($LASTEXITCODE) { throw 'Git history backup failed.' }
& git -C $repositoryPath bundle verify $bundlePath 2>&1 | Out-File (Join-Path $backupPath 'bundle-verification.txt')
if ($LASTEXITCODE) { throw 'Git bundle verification failed.' }
& git -C $repositoryPath archive --format=zip ('--output=' + (Join-Path $backupPath 'Notepads-source.zip')) $commit
if ($LASTEXITCODE) { throw 'Source archive backup failed.' }

# Verify an actual independent checkout, including the directly maintained
# native source. Git canonical hashes account for each file's checkout EOL.
$restoredPath = Join-Path $backupPath 'restore-check'
& git clone --quiet --no-checkout $bundlePath $restoredPath
if ($LASTEXITCODE) { throw 'Cannot restore the source bundle.' }
& git -C $restoredPath checkout --quiet $commit
if ($LASTEXITCODE) { throw 'Cannot check out the source commit.' }
$expectedBlobs = [Collections.Generic.List[string]]::new()
$relativePaths = [Collections.Generic.List[string]]::new()
foreach ($entry in $sourceTree) {
    if ($entry -notmatch '^\d+ blob ([0-9a-f]+)\t(.+)$') { throw "Unexpected source tree entry: $entry" }
    $expectedBlobs.Add($Matches[1])
    $relativePaths.Add($Matches[2])
}
$actualBlobs = @($relativePaths | & git -C $restoredPath hash-object --stdin-paths)
if ($LASTEXITCODE -or $actualBlobs.Count -ne $expectedBlobs.Count) { throw 'Cannot hash the restored source tree.' }
for ($fileIndex = 0; $fileIndex -lt $expectedBlobs.Count; $fileIndex++) {
    if ($actualBlobs[$fileIndex] -ne $expectedBlobs[$fileIndex]) { throw "Restored source mismatch: $($relativePaths[$fileIndex])" }
}
$verifiedFiles = $relativePaths.Count

$metadata = [ordered]@{
    CreatedUtc = [DateTime]::UtcNow.ToString('o')
    Commit = $commit
    NativeSource = 'src/Notepads.Native; included directly in the same source commit and bundle'
    VerifiedFiles = $verifiedFiles
    Verification = 'Independent bundle clone and checkout; every committed file matches its canonical Git blob.'
    Artifacts = @(Get-ChildItem -LiteralPath $backupPath -File | ForEach-Object {
        [ordered]@{ Name = $_.Name; SHA256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }
    })
}
$metadata | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $backupPath 'manifest.json') -Encoding utf8
@"
Reviewed Notepads V2 source: $commit

To restore into a new directory:
  git clone Notepads.bundle Notepads
  git -C Notepads checkout $commit

The same commit contains the application and native editor. No submodule or
patch step is required. Notepads-source.zip is an additional source archive.
The restore-check directory was independently restored and every tracked file
was verified against the committed Git blob. Artifact SHA-256 hashes are in
manifest.json. Native dependencies can be restored with Initialize-Editor.ps1.
"@ | Set-Content -LiteralPath (Join-Path $backupPath 'README.txt') -Encoding utf8
Write-Output "Verified source backup: $backupPath ($verifiedFiles files)"
