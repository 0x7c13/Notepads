# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

[CmdletBinding(PositionalBinding = $false)]
param(
    [string] $Destination,
    [Parameter(Position = 0)]
    [string] $BackupRoot,
    [string] $RepositoryRoot = (Join-Path $PSScriptRoot '..')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repositoryPath = (Resolve-Path -LiteralPath $RepositoryRoot).ProviderPath.TrimEnd('\', '/')
if (!(Test-Path -LiteralPath (Join-Path $repositoryPath '.git') -PathType Container)) {
    throw 'Source/history backup requires a local .git directory; external Git worktree metadata is not included implicitly.'
}
if ($Destination -and $BackupRoot) { throw 'Specify Destination or BackupRoot, not both.' }
if (!$Destination) {
    if (!$BackupRoot) { $BackupRoot = Join-Path $env:USERPROFILE '.notepads-backups' }
    $Destination = Join-Path $BackupRoot ('Notepads-source-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [Guid]::NewGuid().ToString('N'))
}
$backupProvider = $null
$backupDrive = $null
$resolvedBackupPath = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath(
    $Destination, [ref]$backupProvider, [ref]$backupDrive)
if ($backupProvider.Name -ne 'FileSystem') { throw 'Source/history backup destination requires a filesystem path.' }
$backupPath = [IO.Path]::GetFullPath($resolvedBackupPath).TrimEnd('\', '/')
if ($backupPath.Equals($repositoryPath, [StringComparison]::OrdinalIgnoreCase) -or
    $backupPath.StartsWith($repositoryPath + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Source/history backups must be outside the repository.'
}
if (Test-Path -LiteralPath $backupPath) { throw 'Backup destination exists; use a new destination instead of overwriting.' }

function Assert-BackupPlainPath([string] $Path) {
    $current = [IO.Path]::GetFullPath($Path)
    while ($current) {
        if (Test-Path -LiteralPath $current) {
            $entry = Get-Item -LiteralPath $current -Force
            if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Backup refuses a reparse point: $current" }
        }
        $parent = [IO.Path]::GetDirectoryName($current)
        if ($parent -eq $current) { break }
        $current = $parent
    }
}

function Get-BackupSourceFiles {
    $files = [Collections.Generic.List[string]]::new()
    $generatedNames = @('.git', '.tools', 'artifacts', 'packages', 'target', 'obj', 'bin',
        '_buildUwp', '_buildWinUI3', 'AppPackages', 'BundleArtifacts', 'Generated Files',
        'TestResults', '__pycache__', '.vs', '.idea')
    $legacyOutputs = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($relative in @('src', 'src/Notepads.Native/WinUIEditor', 'src/Notepads.Editor/WinUIEditor')) {
        foreach ($configuration in @('Debug', 'Release', 'Production')) {
            $null = $legacyOutputs.Add([IO.Path]::GetFullPath((Join-Path $repositoryPath "$relative/$configuration")))
            foreach ($architecture in @('ARM', 'ARM64', 'x64', 'x86', 'Win32')) {
                $null = $legacyOutputs.Add([IO.Path]::GetFullPath((Join-Path $repositoryPath "$relative/$architecture/$configuration")))
            }
        }
    }
    $markdownRoot = [IO.Path]::GetFullPath((Join-Path $repositoryPath 'src/Notepads.Markdown')).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $documentsRoot = [IO.Path]::GetFullPath((Join-Path $repositoryPath 'docs')).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    # History is inventoried separately without source-output exclusions. In
    # particular, .git/logs and every object/index file belong to the checkpoint.
    foreach ($tree in @(@{ Path = $repositoryPath; History = $false }, @{ Path = (Join-Path $repositoryPath '.git'); History = $true })) {
        $pending = [Collections.Generic.Stack[string]]::new()
        $pending.Push($tree.Path)
        while ($pending.Count) {
            $directory = $pending.Pop()
            foreach ($entry in Get-ChildItem -LiteralPath $directory -Force) {
                $document = $entry.FullName.StartsWith($documentsRoot, [StringComparison]::OrdinalIgnoreCase)
                if ($entry.PSIsContainer -and !$tree.History -and !$document) {
                    $sourceBin = $entry.Name -eq 'bin' -and [IO.Path]::GetFileName($directory) -eq 'src' -and
                        $entry.FullName.StartsWith($markdownRoot, [StringComparison]::OrdinalIgnoreCase)
                    if (($entry.Name -in $generatedNames -and !$sourceBin) -or $legacyOutputs.Contains($entry.FullName)) { continue }
                }
                if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Source inventory refuses a reparse point: $($entry.FullName)" }
                if ($entry.PSIsContainer) { $pending.Push($entry.FullName) }
                elseif ($tree.History -or $document -or $entry.Extension -notin @('.user', '.log', '.binlog', '.pyc', '.pyo')) { $files.Add($entry.FullName) }
            }
        }
    }
    return @($files | Sort-Object)
}

function Get-BackupHistoryDirectories([string] $HistoryRoot) {
    $root = [IO.Path]::GetFullPath($HistoryRoot).TrimEnd('\', '/')
    $directories = [Collections.Generic.List[string]]::new()
    $pending = [Collections.Generic.Stack[string]]::new()
    $pending.Push($root)
    while ($pending.Count) {
        $directory = $pending.Pop()
        $directories.Add('.git' + $directory.Substring($root.Length).Replace('\', '/'))
        foreach ($entry in Get-ChildItem -LiteralPath $directory -Force -Directory) {
            if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "History directory inventory refuses a reparse point: $($entry.FullName)"
            }
            $pending.Push($entry.FullName)
        }
    }
    return @($directories | Sort-Object)
}

Assert-BackupPlainPath $repositoryPath
Assert-BackupPlainPath (Join-Path $repositoryPath '.git')
Assert-BackupPlainPath $backupPath
# Copying local metadata cannot make borrowed or lazily fetched Git objects
# self-contained. Refuse these repositories before creating any destination.
$gitPath = Join-Path $repositoryPath '.git'
foreach ($name in @('GIT_DIR', 'GIT_WORK_TREE', 'GIT_COMMON_DIR', 'GIT_INDEX_FILE', 'GIT_SHALLOW_FILE',
    'GIT_OBJECT_DIRECTORY', 'GIT_ALTERNATE_OBJECT_DIRECTORIES')) {
    if (![string]::IsNullOrEmpty([Environment]::GetEnvironmentVariable($name, 'Process'))) {
        throw "Source/history backup does not support Git environment override $name; use the repository's local metadata."
    }
}
if (Test-Path -LiteralPath (Join-Path $gitPath 'commondir')) {
    throw 'Source/history backup requires self-contained Git objects; external common Git directories are not supported.'
}
$worktreesPath = Join-Path $gitPath 'worktrees'
if (Test-Path -LiteralPath $worktreesPath) {
    Assert-BackupPlainPath $worktreesPath
    if (@(Get-ChildItem -LiteralPath $worktreesPath -Force).Count) {
        throw 'Source/history backup does not support registered linked worktrees; preserve their sources separately and use a self-contained Git bundle.'
    }
}
# An absolute core.worktree would leave Git in the copy operating on the source.
# Resolve includes and inherited configuration in a child with an explicit Git
# directory; do not change the caller's Git environment or native exit status.
$gitCommand = Get-Command git -CommandType Application -ErrorAction Stop | Select-Object -First 1
$gitStart = [Diagnostics.ProcessStartInfo]::new()
$gitStart.FileName = $gitCommand.Source
$gitStart.Arguments = 'config --includes --get core.worktree'
$gitStart.WorkingDirectory = $repositoryPath
$gitStart.UseShellExecute = $false
$gitStart.CreateNoWindow = $true
$gitStart.RedirectStandardOutput = $true
$gitStart.RedirectStandardError = $true
$gitStart.EnvironmentVariables['GIT_DIR'] = $gitPath
$gitProcess = [Diagnostics.Process]::new()
$gitProcess.StartInfo = $gitStart
try {
    $null = $gitProcess.Start()
    $gitOutput = $gitProcess.StandardOutput.ReadToEndAsync()
    $gitError = $gitProcess.StandardError.ReadToEndAsync()
    if (!$gitProcess.WaitForExit(30000)) {
        $gitProcess.Kill()
        throw 'Git configuration validation timed out; no backup was created.'
    }
    $null = $gitOutput.GetAwaiter().GetResult()
    $gitErrorText = $gitError.GetAwaiter().GetResult()
    if ($gitProcess.ExitCode -eq 0) {
        throw 'Source/history backup does not support core.worktree; the copied metadata must not redirect the restored worktree.'
    }
    if ($gitProcess.ExitCode -ne 1) {
        throw "Cannot resolve Git worktree configuration: $gitErrorText"
    }
}
finally { $gitProcess.Dispose() }
foreach ($relative in @('objects/info/alternates', 'objects/info/http-alternates')) {
    if (Test-Path -LiteralPath (Join-Path $gitPath $relative)) {
        throw 'Source/history backup requires self-contained Git objects; alternate object stores are not supported.'
    }
}
if (@(Get-ChildItem -LiteralPath (Join-Path $gitPath 'objects/pack') -Filter '*.promisor' -File -ErrorAction SilentlyContinue).Count -or
    ((Test-Path -LiteralPath (Join-Path $gitPath 'config') -PathType Leaf) -and
     [IO.File]::ReadAllText((Join-Path $gitPath 'config')) -match '(?im)^\s*(partialclone|promisor)\s*=')) {
    throw 'Source/history backup requires self-contained Git objects; partial clones and promisor objects are not supported.'
}
$files = @(Get-BackupSourceFiles)
$historyDirectories = @(Get-BackupHistoryDirectories $gitPath)
if (!$files.Count) { throw 'Source inventory is empty.' }
foreach ($name in @('backup-manifest.json', 'backup-summary.json')) {
    if (Test-Path -LiteralPath (Join-Path $repositoryPath $name)) { throw "Backup receipt would overwrite a source path: $name" }
}
$null = New-Item -ItemType Directory -Path $backupPath -ErrorAction Stop
# Git requires objects/ and refs/ even when there are no loose objects/refs.
# File copying alone loses these directories in unborn or fully packed repos.
foreach ($relative in $historyDirectories) {
    $directory = Join-Path $backupPath $relative
    $null = [IO.Directory]::CreateDirectory($directory)
    Assert-BackupPlainPath $directory
}
$manifest = [Collections.Generic.List[object]]::new()
foreach ($source in $files) {
    $relative = $source.Substring($repositoryPath.Length + 1).Replace('\', '/')
    $copy = Join-Path $backupPath $relative
    $null = [IO.Directory]::CreateDirectory((Split-Path -Parent $copy))
    Assert-BackupPlainPath (Split-Path -Parent $copy)
    [IO.File]::Copy($source, $copy, $false)
    $sourceHash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
    $copyHash = (Get-FileHash -LiteralPath $copy -Algorithm SHA256).Hash
    if ($sourceHash -cne $copyHash) { throw "Backup SHA256 mismatch: $relative" }
    $manifest.Add([pscustomobject]@{ Path = $relative; SHA256 = $sourceHash; Bytes = (Get-Item -LiteralPath $copy).Length })
}

# Publish receipts only after source membership and both copies have been read
# back again. A failed/in-flight checkpoint remains an unverified destination.
$finalFiles = @(Get-BackupSourceFiles)
if ($finalFiles.Count -ne $files.Count -or ($finalFiles -join "`0") -cne ($files -join "`0")) {
    throw 'Source membership changed during backup; no completion receipt was published.'
}
$copiedFiles = @(Get-ChildItem -LiteralPath $backupPath -Recurse -Force -File)
if ($copiedFiles.Count -ne $manifest.Count) { throw 'Backup payload membership changed; no completion receipt was published.' }
$finalHistoryDirectories = @(Get-BackupHistoryDirectories $gitPath)
$copiedHistoryDirectories = @(Get-BackupHistoryDirectories (Join-Path $backupPath '.git'))
if (($finalHistoryDirectories -join "`0") -cne ($historyDirectories -join "`0") -or
    ($copiedHistoryDirectories -join "`0") -cne ($historyDirectories -join "`0")) {
    throw 'Git directory membership changed; no completion receipt was published.'
}
foreach ($entry in $manifest) {
    $source = Join-Path $repositoryPath $entry.Path
    $copy = Join-Path $backupPath $entry.Path
    if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -cne $entry.SHA256 -or
        (Get-FileHash -LiteralPath $copy -Algorithm SHA256).Hash -cne $entry.SHA256 -or
        (Get-Item -LiteralPath $source).Length -ne $entry.Bytes -or (Get-Item -LiteralPath $copy).Length -ne $entry.Bytes) {
        throw "Source or backup changed before final verification: $($entry.Path)"
    }
}
$manifestPath = Join-Path $backupPath 'backup-manifest.json'
[IO.File]::WriteAllText($manifestPath, (ConvertTo-Json -InputObject $manifest.ToArray() -Depth 4), [Text.UTF8Encoding]::new($false))
$manifestHash = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash
$summary = [ordered]@{
    Schema = 1; Status = 'VERIFIED'; CreatedUtc = [DateTime]::UtcNow.ToString('o')
    Repository = $repositoryPath; Backup = $backupPath; VerifiedFiles = $manifest.Count
    Bytes = ($manifest | Measure-Object -Property Bytes -Sum).Sum
    Verification = 'Current source, ignored documents and local .git copied; every file SHA256 verified, then source membership and both copies rechecked.'
    Manifest = $manifestPath; ManifestSHA256 = $manifestHash
    HistoryDirectories = $historyDirectories
}
$summaryPath = Join-Path $backupPath 'backup-summary.json'
[IO.File]::WriteAllText($summaryPath, (ConvertTo-Json -InputObject $summary -Depth 4), [Text.UTF8Encoding]::new($false))
[pscustomobject]@{ Backup = $backupPath; VerifiedFiles = $manifest.Count; Manifest = $manifestPath; ManifestSHA256 = $manifestHash; Summary = $summaryPath }
