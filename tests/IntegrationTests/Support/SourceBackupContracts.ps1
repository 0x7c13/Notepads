# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

[CmdletBinding()]
param([Parameter(Mandatory)][string] $EvidenceRoot)
$ErrorActionPreference = 'Stop'
$null = [IO.Directory]::CreateDirectory([IO.Path]::GetFullPath($EvidenceRoot))
$isolatedGitConfig = Join-Path ([IO.Path]::GetFullPath($EvidenceRoot)) 'empty-global-git-config'
[IO.File]::WriteAllText($isolatedGitConfig, '')
$configurationEnvironment = @{}
foreach ($name in @('GIT_CONFIG_NOSYSTEM', 'GIT_CONFIG_GLOBAL')) {
    $configurationEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
try {
$env:GIT_CONFIG_NOSYSTEM = '1'
$env:GIT_CONFIG_GLOBAL = $isolatedGitConfig
$helper = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../scripts/Backup-Source.ps1'))
$fixture = Join-Path ([IO.Path]::GetFullPath($EvidenceRoot)) 'source'
$checks = [Collections.Generic.List[string]]::new()
function Write-Fixture([string] $Relative, [string] $Text) {
    $path = Join-Path $fixture $Relative
    $null = [IO.Directory]::CreateDirectory((Split-Path -Parent $path))
    [IO.File]::WriteAllText($path, $Text, [Text.UTF8Encoding]::new($false))
}
function Expect-Rejection([string] $Name, [string] $Message, [scriptblock] $Action) {
    $failure = $null
    try { & $Action | Out-Null } catch { $failure = $_ }
    if (!$failure -or $failure.Exception.Message -notlike $Message) { throw "$Name did not fail with expected guard ($Message): $failure" }
    $checks.Add($Name)
}
$sourceBins = @(
    'src/Notepads.Markdown/vendor/ratex-bounded/ratex-layout/src/bin/layout.rs',
    'src/Notepads.Markdown/vendor/ratex-bounded/ratex-lexer/src/bin/lex.rs',
    'src/Notepads.Markdown/vendor/ratex-bounded/ratex-parser/src/bin/parse.rs',
    'src/Notepads.Markdown/vendor/ratex-control/ratex-layout/src/bin/layout.rs',
    'src/Notepads.Markdown/vendor/ratex-control/ratex-lexer/src/bin/lex.rs',
    'src/Notepads.Markdown/vendor/ratex-control/ratex-parser/src/bin/parse.rs')
$included = @('.git/HEAD', '.git/logs/HEAD.log', '.git/objects/ab/history', '.git/bin/history', '.git/obj/index',
    '.gitignore', 'docs/private ignored.md', 'docs/reference/bin/example.rs', 'docs/original-run.log', 'untracked.cs',
    'src/Notepads.App/Assets/ARM64/Release/icon.png', 'src/Notepads.Editor/scintilla/win32/ScintillaWin.cxx') + $sourceBins
$excluded = @('.tools/cargo/bin/cargo.exe', 'artifacts/evidence/out.json',
    'src/_buildUwp/x64/Release/Editor.dll', 'src/_buildWinUI3/x64/Debug/Editor.obj',
    'src/Notepads.App/bin/Release/App.dll', 'src/Notepads.App/obj/Release/App.g.cs',
    'src/Notepads.App/AppPackages/App.msix', 'src/Notepads.App/BundleArtifacts/App.msixbundle',
    'src/ARM64/Release/legacy.dll', 'src/Notepads.Editor/WinUIEditor/Win32/Release/legacy.lib',
    'src/Notepads.Native/WinUIEditor/x64/Release/legacy.lib',
    'src/Notepads.Markdown/crates/core/bin/Release/generated.dll',
    'src/Notepads.Markdown/target/release/core.lib', 'src/Notepads.Markdown/windows/packages/CppWinRT/input.h',
    'src/Notepads.Markdown/artifacts/native/component.dll', '.vs/state', 'tests/TestResults/report.xml',
    'tests/__pycache__/fixture.pyc', 'scripts/build.log', 'scripts/build.binlog')
foreach ($relative in $included + $excluded) { Write-Fixture $relative ("fixture bytes: " + $relative) }
Write-Fixture '.gitignore' "docs/`n**/bin/`n"
$backup = Join-Path $EvidenceRoot 'checkpoint'
$result = & $helper -RepositoryRoot $fixture -Destination $backup
$manifest = Get-Content -LiteralPath $result.Manifest -Raw | ConvertFrom-Json
if ($result.VerifiedFiles -ne $included.Count -or $manifest.Count -ne $included.Count) { throw 'Backup source membership is incomplete or includes generated output.' }
foreach ($relative in $included) {
    $entry = @($manifest | Where-Object Path -eq $relative)
    if ($entry.Count -ne 1 -or !(Test-Path -LiteralPath (Join-Path $backup $relative) -PathType Leaf) -or
        (Get-FileHash -LiteralPath (Join-Path $fixture $relative) -Algorithm SHA256).Hash -cne $entry[0].SHA256 -or
        (Get-FileHash -LiteralPath (Join-Path $backup $relative) -Algorithm SHA256).Hash -cne $entry[0].SHA256) {
        throw "Missing or incorrect source/history backup: $relative"
    }
}
foreach ($relative in $excluded) {
    if ($relative -in $manifest.Path -or (Test-Path -LiteralPath (Join-Path $backup $relative))) { throw "Generated output entered backup: $relative" }
}
$summary = Get-Content -LiteralPath $result.Summary -Raw | ConvertFrom-Json
if ($summary.Status -ne 'VERIFIED' -or $summary.ManifestSHA256 -cne (Get-FileHash -LiteralPath $result.Manifest -Algorithm SHA256).Hash) {
    throw 'Backup completion receipt is not bound to its source manifest.'
}
$checks.Add('Current/ignored sources, all six Cargo src/bin entries and complete .git metadata retain exact SHA256')
$checks.Add('Native/MSBuild/Cargo/tool caches and generated bin paths remain excluded; real platform assets remain included')
$checks.Add('Completion receipt binds the verified file inventory')
Expect-Rejection 'Existing destination is preserved' '*destination exists*' { & $helper -RepositoryRoot $fixture -Destination $backup }
Expect-Rejection 'Normalized repository-internal destination is rejected' '*must be outside the repository*' {
    & $helper -RepositoryRoot $fixture -Destination (Join-Path $fixture 'docs/../unsafe')
}
Expect-Rejection 'Conflicting destination options are rejected' '*not both*' {
    & $helper -RepositoryRoot $fixture -Destination (Join-Path $EvidenceRoot 'unused') -BackupRoot (Join-Path $EvidenceRoot 'unused-parent')
}
$compatibility = & $helper -RepositoryRoot $fixture -BackupRoot (Join-Path $EvidenceRoot 'parent')
if ($compatibility.VerifiedFiles -ne $included.Count) { throw 'BackupRoot compatibility dropped source files.' }
$checks.Add('BackupRoot creates a new verified checkpoint without requiring a clean commit')
$positional = & $helper (Join-Path $EvidenceRoot 'parent') -RepositoryRoot $fixture
if ($positional.VerifiedFiles -ne $included.Count -or
    (Split-Path -Parent $positional.Backup) -ne (Join-Path $EvidenceRoot 'parent')) {
    throw 'The original positional BackupRoot invocation no longer creates a child checkpoint.'
}
$checks.Add('Positional BackupRoot retains the original existing-parent invocation')
foreach ($kind in @('alternates', 'http-alternates', 'partial-config', 'promisor-config', 'promisor-pack')) {
    $historyFixture = Join-Path $EvidenceRoot ('history-' + $kind)
    $null = [IO.Directory]::CreateDirectory((Join-Path $historyFixture '.git/objects/pack'))
    $null = [IO.Directory]::CreateDirectory((Join-Path $historyFixture '.git/objects/info'))
    $metadata = switch ($kind) {
        'alternates' { 'objects/info/alternates' }
        'http-alternates' { 'objects/info/http-alternates' }
        'promisor-pack' { 'objects/pack/fixture.promisor' }
        default { 'config' }
    }
    $value = switch ($kind) {
        'partial-config' { "[extensions]`npartialClone = origin`n" }
        'promisor-config' { "[remote `"origin`"]`npromisor = true`n" }
        default { 'external object dependency' }
    }
    [IO.File]::WriteAllText((Join-Path (Join-Path $historyFixture '.git') $metadata), $value)
    $historyBackup = Join-Path $EvidenceRoot ('blocked-' + $kind)
    Expect-Rejection "Non-self-contained history is refused: $kind" '*self-contained Git objects*' {
        & $helper -RepositoryRoot $historyFixture -Destination $historyBackup
    }
    if (Test-Path -LiteralPath $historyBackup) { throw 'Rejected history published a partial destination.' }
}
$junction = Join-Path $EvidenceRoot 'junction'
$null = New-Item -ItemType Junction -Path $junction -Target $fixture -ErrorAction Stop
Expect-Rejection 'Destination junction cannot redirect the checkpoint into source' '*reparse point*' {
    & $helper -RepositoryRoot $fixture -Destination (Join-Path $junction 'unsafe')
}
if (Test-Path -LiteralPath (Join-Path $fixture 'unsafe')) { throw 'Rejected destination changed the source tree.' }
Write-Fixture 'backup-manifest.json' 'existing source receipt name'
Expect-Rejection 'Receipt name cannot overwrite a source file' '*would overwrite a source path*' {
    & $helper -RepositoryRoot $fixture -Destination (Join-Path $EvidenceRoot 'receipt-collision')
}
if (Test-Path -LiteralPath (Join-Path $EvidenceRoot 'receipt-collision')) { throw 'Receipt rejection created a partial backup.' }

function Invoke-BackupGit([string[]] $GitArguments) {
    $previousPreference = $ErrorActionPreference
    try {
        # Git may emit informational notices on stderr for an unborn repository.
        $ErrorActionPreference = 'Continue'
        $output = & git @GitArguments 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally { $ErrorActionPreference = $previousPreference }
    if ($exitCode -ne 0) { throw "Git fixture failed ($exitCode): $($GitArguments -join ' '): $output" }
    return $output
}
foreach ($kind in @('packed', 'unborn')) {
    $gitSource = Join-Path $EvidenceRoot ('real-git-' + $kind)
    $gitBackup = Join-Path $EvidenceRoot ('real-git-backup-' + $kind)
    Invoke-BackupGit @('init', '--quiet', $gitSource) | Out-Null
    $null = [IO.Directory]::CreateDirectory((Join-Path $gitSource '.git/private-empty/nested'))
    $null = [IO.Directory]::CreateDirectory((Join-Path $gitSource '.git/worktrees'))
    [IO.File]::WriteAllText((Join-Path $gitSource 'fixture.txt'), 'source bytes')
    if ($kind -eq 'packed') {
        Invoke-BackupGit @('-C', $gitSource, 'add', 'fixture.txt') | Out-Null
        $hooksConfiguration = 'core.hooksPath=' + (Join-Path $gitSource '.git/private-empty')
        Invoke-BackupGit @('-C', $gitSource, '-c', $hooksConfiguration, '-c', 'commit.gpgsign=false',
            '-c', 'user.name=Backup fixture', '-c', 'user.email=fixture@example.invalid',
            'commit', '--quiet', '-m', 'backup fixture') | Out-Null
        Invoke-BackupGit @('-C', $gitSource, 'pack-refs', '--all', '--prune') | Out-Null
        if (@(Get-ChildItem -LiteralPath (Join-Path $gitSource '.git/refs') -Recurse -File).Count) {
            throw 'The packed-ref fixture still has loose refs.'
        }
    }
    $real = & $helper -RepositoryRoot $gitSource -Destination $gitBackup
    $realSummary = Get-Content -LiteralPath $real.Summary -Raw | ConvertFrom-Json
    foreach ($relative in @('.git/objects', '.git/refs', '.git/private-empty/nested', '.git/worktrees')) {
        if (!(Test-Path -LiteralPath (Join-Path $gitBackup $relative) -PathType Container) -or
            $relative -notin $realSummary.HistoryDirectories) {
            throw "The $kind backup dropped an empty Git directory: $relative"
        }
    }
    $gitDirectoryArgument = '--git-dir=' + (Join-Path $gitBackup '.git')
    $sourceHead = Invoke-BackupGit @('-C', $gitSource, 'symbolic-ref', 'HEAD')
    $backupHead = Invoke-BackupGit @($gitDirectoryArgument, 'symbolic-ref', 'HEAD')
    if ($sourceHead -cne $backupHead) { throw 'The restored Git HEAD differs from the source.' }
    Invoke-BackupGit @($gitDirectoryArgument, 'fsck', '--full') | Out-Null
    if ($kind -eq 'packed') {
        Invoke-BackupGit @($gitDirectoryArgument, 'cat-file', '-e', 'HEAD:fixture.txt') | Out-Null
    }
    $checks.Add("Real $kind Git backup preserves empty directories and independently passes fsck")
}
$registeredSource = Join-Path $EvidenceRoot 'real-git-packed'
$linkedWorktree = Join-Path $EvidenceRoot 'linked-working-copy'
$registeredBackup = Join-Path $EvidenceRoot 'blocked-registered-worktree'
$registeredHooks = 'core.hooksPath=' + (Join-Path $registeredSource '.git/private-empty')
Invoke-BackupGit @('-C', $registeredSource, '-c', $registeredHooks, 'worktree', 'add', '--quiet', '--detach', $linkedWorktree, 'HEAD') | Out-Null
$linkedPointer = Join-Path $linkedWorktree '.git'
$linkedPointerBefore = (Get-FileHash -LiteralPath $linkedPointer -Algorithm SHA256).Hash
Expect-Rejection 'Registered linked worktrees are refused before copying metadata' '*does not support registered linked worktrees*' {
    & $helper -RepositoryRoot $registeredSource -Destination $registeredBackup
}
if (Test-Path -LiteralPath $registeredBackup) { throw 'Rejected linked-worktree registry created a partial backup.' }
if ((Get-FileHash -LiteralPath $linkedPointer -Algorithm SHA256).Hash -cne $linkedPointerBefore) {
    throw 'Rejected backup changed the original linked-worktree pointer.'
}
foreach ($name in @('GIT_DIR', 'GIT_WORK_TREE', 'GIT_COMMON_DIR', 'GIT_INDEX_FILE', 'GIT_SHALLOW_FILE',
    'GIT_OBJECT_DIRECTORY', 'GIT_ALTERNATE_OBJECT_DIRECTORIES')) {
    $previousValue = [Environment]::GetEnvironmentVariable($name, 'Process')
    $blocked = Join-Path $EvidenceRoot ('blocked-env-' + $name)
    try {
        [Environment]::SetEnvironmentVariable($name, (Join-Path $EvidenceRoot 'external-git-state'), 'Process')
        Expect-Rejection "External Git environment is refused: $name" "*Git environment override $name*" {
            & $helper -RepositoryRoot $fixture -Destination $blocked
        }
        if (Test-Path -LiteralPath $blocked) { throw 'Rejected Git environment created a partial backup.' }
    }
    finally {
        if ($null -eq $previousValue) { Remove-Item -LiteralPath "Env:\$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $previousValue, 'Process') }
    }
}
$commonSource = Join-Path $EvidenceRoot 'external-common-source'
$null = [IO.Directory]::CreateDirectory((Join-Path $commonSource '.git'))
[IO.File]::WriteAllText((Join-Path $commonSource '.git/commondir'), '../external')
$commonBackup = Join-Path $EvidenceRoot 'blocked-common'
Expect-Rejection 'External common Git metadata is refused' '*external common Git directories*' {
    & $helper -RepositoryRoot $commonSource -Destination $commonBackup
}
if (Test-Path -LiteralPath $commonBackup) { throw 'Rejected common Git directory created a partial backup.' }
foreach ($kind in @('direct', 'included', 'global')) {
    $worktreeSource = Join-Path $EvidenceRoot ('worktree-' + $kind)
    $worktreeBackup = Join-Path $EvidenceRoot ('blocked-worktree-' + $kind)
    Invoke-BackupGit @('init', '--quiet', $worktreeSource) | Out-Null
    $configuration = if ($kind -eq 'direct') { Join-Path $worktreeSource '.git/config' }
        elseif ($kind -eq 'global') { $isolatedGitConfig }
        else { Join-Path $EvidenceRoot 'external-worktree-config' }
    Invoke-BackupGit @('config', '--file', $configuration, 'core.worktree', $worktreeSource) | Out-Null
    if ($kind -eq 'included') {
        Invoke-BackupGit @('-C', $worktreeSource, 'config', 'include.path', $configuration) | Out-Null
    }
    Expect-Rejection "Git worktree redirection is refused: $kind" '*does not support core.worktree*' {
        & $helper -RepositoryRoot $worktreeSource -Destination $worktreeBackup
    }
    if (Test-Path -LiteralPath $worktreeBackup) { throw 'Rejected worktree redirection created a partial backup.' }
    [IO.File]::WriteAllText($isolatedGitConfig, '')
}
$pathSource = Join-Path $EvidenceRoot 'real-git-unborn'
$pathLocation = [IO.Path]::GetFullPath((Join-Path $EvidenceRoot 'path-location'))
$pathProcess = [IO.Path]::GetFullPath((Join-Path $EvidenceRoot 'path-process'))
$null = [IO.Directory]::CreateDirectory($pathLocation)
$null = [IO.Directory]::CreateDirectory($pathProcess)
$originalLocation = Get-Location
$originalProcessDirectory = [Environment]::CurrentDirectory
$driveName = 'BackupFixture' + [Guid]::NewGuid().ToString('N').Substring(0, 8)
$driveCreated = $false
try {
    Set-Location -LiteralPath $pathLocation
    [Environment]::CurrentDirectory = $pathProcess
    foreach ($kind in @('Destination', 'NamedRoot', 'PositionalRoot')) {
        $relative = 'relative-' + $kind
        $pathResult = switch ($kind) {
            'Destination' { & $helper -RepositoryRoot $pathSource -Destination $relative }
            'NamedRoot' { & $helper -RepositoryRoot $pathSource -BackupRoot $relative }
            'PositionalRoot' { & $helper $relative -RepositoryRoot $pathSource }
        }
        $actual = if ($kind -eq 'Destination') { $pathResult.Backup } else { Split-Path -Parent $pathResult.Backup }
        if ($actual -ne (Join-Path $pathLocation $relative) -or
            (Test-Path -LiteralPath (Join-Path $pathProcess $relative))) {
            throw "Relative $kind ignored the PowerShell working location."
        }
        $checks.Add("Relative $kind follows PowerShell location independently of process cwd")
    }
    $providerTarget = Join-Path $pathLocation 'provider-target'
    $providerResult = & $helper -RepositoryRoot $pathSource -Destination ('FileSystem::' + $providerTarget)
    if ($providerResult.Backup -ne $providerTarget) { throw 'Filesystem provider-qualified backup path changed.' }
    $checks.Add('Filesystem provider-qualified destinations retain their filesystem target')
    $null = New-PSDrive -Name $driveName -PSProvider FileSystem -Root $pathLocation
    $driveCreated = $true
    $driveResult = & $helper -RepositoryRoot $pathSource -Destination ($driveName + ':\drive-target')
    if ($driveResult.Backup -ne (Join-Path $pathLocation 'drive-target')) { throw 'Filesystem PSDrive backup path changed.' }
    $checks.Add('Filesystem PSDrive destinations resolve before path containment checks')
    Set-Location -LiteralPath $pathSource
    Expect-Rejection 'Relative destination inside source is rejected using PowerShell location' '*must be outside the repository*' {
        & $helper -RepositoryRoot $pathSource -Destination 'inside-source'
    }
    if ((Test-Path -LiteralPath (Join-Path $pathSource 'inside-source')) -or
        (Test-Path -LiteralPath (Join-Path $pathProcess 'inside-source'))) {
        throw 'Rejected relative source-internal destination created output.'
    }
    $environmentName = 'NotepadsBackupFixture' + [Guid]::NewGuid().ToString('N')
    Expect-Rejection 'Non-filesystem destination providers are rejected' '*requires a filesystem path*' {
        & $helper -RepositoryRoot $pathSource -Destination ('Env:\' + $environmentName)
    }
    if (Test-Path -LiteralPath ('Env:\' + $environmentName)) { throw 'Rejected backup wrote an environment variable.' }
}
finally {
    Set-Location -LiteralPath $originalLocation.Path
    [Environment]::CurrentDirectory = $originalProcessDirectory
    if ($driveCreated) { Remove-PSDrive -Name $driveName -Force }
}
if ((Get-Location).Path -ne $originalLocation.Path -or [Environment]::CurrentDirectory -ne $originalProcessDirectory) {
    throw 'Backup path fixtures did not restore the caller location.'
}
$checks | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'checks.json') -Encoding utf8
Write-Output "PASS source backup contracts: $($checks.Count) checks; private fixtures only, no build/UI/deletion."
}
finally {
    foreach ($name in $configurationEnvironment.Keys) {
        if ($null -eq $configurationEnvironment[$name]) {
            Remove-Item -LiteralPath "Env:\$name" -ErrorAction SilentlyContinue
        }
        else { [Environment]::SetEnvironmentVariable($name, $configurationEnvironment[$name], 'Process') }
    }
}
