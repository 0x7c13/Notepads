# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Build identity uses content, not mtimes: --NoBuild must never run a stale binary.

function Get-TestSourceFiles {
    param([string] $Profile)
    $repo = Split-Path $PSScriptRoot -Parent
    $roots = switch ($Profile) {
        'Unit' { @('tests/UnitTests', 'src/Notepads/Features/Documents', 'src/Notepads/Features/Sessions/Contracts',
            'src/Notepads/Features/Sessions/Validation', 'src/Notepads/Features/WebSearch') }
        'Headless' { @('tests/IntegrationTests/Headless', 'src/Notepads/Features/Documents', 'src/Notepads/Features/Sessions',
            'src/Notepads/Infrastructure/Diagnostics') }
        'App' { @('src', 'tests/IntegrationTests/App', 'tests/Shared') }
        'Product' { @('src') }
        'NativeDiff' { @('tests/IntegrationTests/Support/NativeDiff.Tests', 'src/Notepads.Native') }
        'RegexText' { @('tests/IntegrationTests/Support/RegexText.Tests', 'src/Notepads.Native') }
        'DragPeer' { @('tests/IntegrationTests/Support/DragDrop.Peer') }
        default { throw "Unknown build profile: $Profile" }
    }
    $files = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $generatedDirectories = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    # Legacy VC builds wrote beside the solution/project before _buildUwp was
    # introduced. Scope these paths: architecture folders can also hold real
    # platform sources or assets and must not be excluded by name alone.
    foreach ($relative in @('src', 'src/Notepads.Native/WinUIEditor')) {
        foreach ($configuration in @('Debug', 'Release', 'Production')) {
            $null = $generatedDirectories.Add([IO.Path]::GetFullPath((Join-Path $repo "$relative/$configuration")))
            foreach ($architecture in @('ARM', 'ARM64', 'x64', 'x86', 'Win32')) {
                $null = $generatedDirectories.Add([IO.Path]::GetFullPath((Join-Path $repo "$relative/$architecture/$configuration")))
            }
        }
    }
    $generatedNames = @('bin', 'obj', '.git', '.vs', 'packages', '__pycache__', 'TestResults',
        '_buildUwp', '_buildWinUI3', 'Generated Files', 'AppPackages', 'BundleArtifacts')
    $projectInputs = if ($Profile -in @('Headless', 'App')) { @('tests/IntegrationTests/IntegrationTests.csproj', 'tests/IntegrationTests/Directory.Build.props') } else { @() }
    if ($Profile -eq 'App') { $projectInputs += 'tests/IntegrationTests/Package.appxmanifest' }
    foreach ($relative in (@('Directory.Packages.props', 'global.json', '.editorconfig') + $projectInputs)) {
        $path = Join-Path $repo $relative
        if (Test-Path -LiteralPath $path -PathType Leaf) { $null = $files.Add([IO.Path]::GetFullPath($path)) }
    }
    $pending = [Collections.Generic.Stack[string]]::new()
    foreach ($relative in $roots) { $pending.Push((Join-Path $repo $relative)) }
    while ($pending.Count) {
        $directory = $pending.Pop()
        if (!(Test-Path -LiteralPath $directory -PathType Container)) { throw "Missing source directory: $directory" }
        foreach ($entry in Get-ChildItem -LiteralPath $directory -Force) {
            if ($entry.PSIsContainer -and ($entry.Name -in $generatedNames -or $generatedDirectories.Contains($entry.FullName))) { continue }
            if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Source fingerprint refuses a reparse point: $($entry.FullName)" }
            if ($entry.PSIsContainer) { $pending.Push($entry.FullName) }
            elseif ($entry.Name -notin @('.git', '.DS_Store')) { $null = $files.Add($entry.FullName) }
        }
    }
    return @($files | Sort-Object)
}

function Get-TestSourceHashes {
    param([string] $Profile)
    $repo = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot -Parent)).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $hashes = [ordered]@{}
    foreach ($path in Get-TestSourceFiles $Profile) {
        $hashes[$path.Substring($repo.Length).Replace('\', '/')] = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
    }
    return $hashes
}

function Get-TestArtifactFiles {
    param([string] $Profile, [string] $Configuration, [string] $Platform)
    $repo = Split-Path $PSScriptRoot -Parent
    $runtime = 'win-' + $Platform.ToLowerInvariant()
    $root = switch ($Profile) {
        'Unit' { Join-Path $repo "tests/UnitTests/bin/$Configuration/net10.0" }
        'Headless' { Join-Path $repo "tests/IntegrationTests/bin/Headless/$Configuration/net10.0" }
        'App' { Join-Path $repo "tests/IntegrationTests/bin/App/$Platform/$Configuration/net10.0-windows10.0.26100.0/$runtime" }
        'Product' { Join-Path $repo "src/Notepads/obj/$Platform/$Configuration/net10.0-windows10.0.26100.0/$runtime" }
        'NativeDiff' { Join-Path $repo "tests/IntegrationTests/Support/NativeDiff.Tests/bin/x64/$Configuration" }
        'RegexText' { Join-Path $repo "tests/IntegrationTests/Support/RegexText.Tests/bin/x64/$Configuration" }
        'DragPeer' { Join-Path $repo "tests/IntegrationTests/Support/DragDrop.Peer/bin/$Configuration/net10.0-windows/$runtime" }
        default { throw "Unknown artifact profile: $Profile" }
    }
    if ($Profile -eq 'Product') {
        $assembly = Join-Path $root 'Notepads.dll'
        if (!(Test-Path -LiteralPath $assembly)) { throw "Missing compiled product assembly: $assembly" }
        return @($assembly)
    }
    if ($Profile -eq 'App') {
        $recipePath = Join-Path $root 'IntegrationTests.build.appxrecipe'
        if (!(Test-Path -LiteralPath $recipePath)) { throw "Missing app recipe: $recipePath" }
        [xml] $recipe = Get-Content -LiteralPath $recipePath
        $paths = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        $null = $paths.Add($recipePath)
        foreach ($group in $recipe.Project.ItemGroup) {
            foreach ($file in $group.ChildNodes) {
                if (!$file.PackagePath -or !$file.Include) { continue }
                $path = [IO.Path]::GetFullPath([Uri]::UnescapeDataString($file.Include))
                if (!(Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing packaged artifact: $path" }
                $null = $paths.Add($path)
            }
        }
        if ($paths.Count -lt 2) { throw 'The app recipe has no package payload.' }
        return @($paths | Sort-Object)
    }
    if (!(Test-Path -LiteralPath $root -PathType Container)) { throw "Missing built output: $root" }
    $files = @(Get-ChildItem -LiteralPath $root -File -Recurse | ForEach-Object FullName | Sort-Object)
    if (!$files.Count) { throw "Empty built output: $root" }
    return $files
}

function Get-TestIdentityPath {
    param([string] $Profile, [string] $Configuration, [string] $Platform)
    $repo = Split-Path $PSScriptRoot -Parent
    return Join-Path $repo "artifacts/test-builds/$Profile-$Configuration-$Platform.json"
}

function Write-TestBuildIdentity {
    param([string] $Profile, [string] $Configuration, [string] $Platform, $BeforeSources)
    $sources = Get-TestSourceHashes $Profile
    if (!$BeforeSources -or ($BeforeSources | ConvertTo-Json -Compress -Depth 3) -cne ($sources | ConvertTo-Json -Compress -Depth 3)) {
        throw 'Source inputs changed during the build; no reusable build identity was published. Rebuild after edits finish.'
    }
    $artifacts = [ordered]@{}
    foreach ($file in Get-TestArtifactFiles $Profile $Configuration $Platform) {
        $artifacts[[IO.Path]::GetFullPath($file)] = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash
    }
    $runtime = if ($Profile -in @('App', 'Product', 'DragPeer')) { 'win-' + $Platform.ToLowerInvariant() } else { 'portable-or-x64-helper' }
    $document = [ordered]@{
        Schema = 1; Profile = $Profile; Configuration = $Configuration; Platform = $Platform; Runtime = $runtime
        Sdk = (& dotnet --version); CreatedUtc = [DateTime]::UtcNow.ToString('O'); Sources = $sources; Artifacts = $artifacts
    }
    $path = Get-TestIdentityPath $Profile $Configuration $Platform
    New-Item -ItemType Directory -Path (Split-Path $path) -Force | Out-Null
    $temporary = $path + '.' + [Guid]::NewGuid().ToString('N') + '.tmp'
    $document | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $temporary -Encoding utf8
    Move-Item -LiteralPath $temporary -Destination $path -Force
    return $path
}

function Assert-TestBuildIdentity {
    param([string] $Profile, [string] $Configuration, [string] $Platform)
    $path = Get-TestIdentityPath $Profile $Configuration $Platform
    if (!(Test-Path -LiteralPath $path)) { throw "No verified build identity exists. Run scripts/Test.ps1 without -NoBuild first: $path" }
    $identity = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
    if ($identity.Schema -ne 1 -or $identity.Profile -ne $Profile -or $identity.Configuration -ne $Configuration -or $identity.Platform -ne $Platform -or
        $identity.Sdk -ne (& dotnet --version)) { throw 'Build profile, configuration, platform or SDK does not match the reusable output.' }
    $sources = Get-TestSourceHashes $Profile
    $saved = @($identity.Sources.PSObject.Properties)
    if ($saved.Count -ne $sources.Count) { throw 'The source file set changed since the build; rebuild before testing.' }
    foreach ($entry in $saved) {
        if (!$sources.Contains($entry.Name) -or $sources[$entry.Name] -cne $entry.Value) { throw "Source changed since build: $($entry.Name)" }
    }
    $files = @(Get-TestArtifactFiles $Profile $Configuration $Platform)
    $artifacts = @($identity.Artifacts.PSObject.Properties)
    if ($files.Count -ne $artifacts.Count) { throw 'The built artifact set changed; rebuild before testing.' }
    $current = [Collections.Generic.HashSet[string]]::new([string[]]$files, [StringComparer]::OrdinalIgnoreCase)
    foreach ($entry in $artifacts) {
        if (!$current.Contains($entry.Name) -or !(Test-Path -LiteralPath $entry.Name -PathType Leaf) -or
            (Get-FileHash -LiteralPath $entry.Name -Algorithm SHA256).Hash -cne $entry.Value) { throw "Built artifact changed: $($entry.Name)" }
    }
    return $path
}
