# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE file in the project root for license information.
# ---------------------------------------------------------------------------------------------

param([switch] $InstallMissing)
$ErrorActionPreference = 'Stop'
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$installation = & $vswhere -latest -products '*' -version '[18.0,19.0)' -property installationPath
if (!$installation) { throw 'Visual Studio 2026 is required.' }
$msbuild = Join-Path $installation 'MSBuild\Current\Bin\MSBuild.exe'
$project = Join-Path $PSScriptRoot '..\src\Notepads.Native\WinUIEditor\WinUIEditor.vcxproj'
$solutionDirectory = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\src\'))

function Test-UwpCppTools {
    $complete = $true
    foreach ($platform in @('Win32', 'x64', 'arm64')) {
        $architecture = if ($platform -eq 'Win32') { 'x86' } else { $platform }
        $output = @()
        try {
            $output = & $msbuild $project /nologo /verbosity:quiet /p:Configuration=Release "/p:Platform=$platform" "/p:SolutionDir=$solutionDirectory" /getProperty:PlatformToolset,VCToolsInstallDir,VCTargetsPath,VCToolsVersion 2>&1
            $exitCode = $LASTEXITCODE
            if ($exitCode -ne 0) { throw "MSBuild exited with $exitCode." }
            $properties = ($output -join "`n" | ConvertFrom-Json).Properties
            $installationRoot = [IO.Path]::GetFullPath($installation).TrimEnd('\') + '\'
            foreach ($directory in @($properties.VCToolsInstallDir, $properties.VCTargetsPath)) {
                if (!$directory -or ![IO.Path]::GetFullPath($directory).StartsWith($installationRoot, [StringComparison]::OrdinalIgnoreCase)) {
                    throw 'The selected compiler and targets must come from Visual Studio 2026.'
                }
            }
            if ($properties.PlatformToolset -ne 'v145' -or !$properties.VCToolsVersion) {
                throw 'The native project requires the v145 toolset.'
            }
            $requiredFiles = @(
                (Join-Path $properties.VCToolsInstallDir "bin\Hostx64\$architecture\cl.exe"),
                (Join-Path $properties.VCToolsInstallDir "lib\$architecture\libcpmt.lib"),
                (Join-Path $properties.VCToolsInstallDir "lib\$architecture\uwp\vccorlib.lib"),
                (Join-Path $properties.VCTargetsPath "Platforms\$platform\PlatformToolsets\v145\Toolset.props"),
                (Join-Path $properties.VCTargetsPath "Platforms\$platform\PlatformToolsets\v145\Toolset.targets")
            )
            foreach ($file in $requiredFiles) {
                if (!(Test-Path -LiteralPath $file -PathType Leaf)) { throw "Missing build tool: $file" }
            }
            Write-Host "$architecture uses v145 $($properties.VCToolsVersion) from $($properties.VCToolsInstallDir)."
        } catch {
            Write-Host "$platform tool validation failed: $_"
            Write-Host ($output -join "`n")
            $complete = $false
        }
    }
    return $complete
}

if (!(Test-UwpCppTools)) {
    if (!$InstallMissing) { throw 'Install the latest x86, x64 and ARM64 C++ tools and UWP libraries, or use -InstallMissing.' }
    $installer = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\setup.exe'
    $arguments = @('modify', '--installPath', ('"' + $installation + '"'), '--quiet', '--norestart',
        '--add', 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64',
        '--add', 'Microsoft.VisualStudio.Component.VC.Tools.ARM64',
        '--add', 'Microsoft.VisualStudio.ComponentGroup.UWP.VC',
        '--add', 'Microsoft.VisualStudio.Component.UWP.VC.ARM64')
    $process = Start-Process -FilePath $installer -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -notin @(0, 3010)) { throw "C++ tool installation failed: $($process.ExitCode)" }
    if (!(Test-UwpCppTools)) { throw 'Matching v145 compilers, targets and UWP libraries are required for every architecture.' }
}
Write-Output 'C++ compilers and UWP libraries are available for x86, x64 and ARM64.'
