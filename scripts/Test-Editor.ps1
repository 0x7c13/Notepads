# Copyright (c) 2026, Jiaqi (0x7c13) Liu. See LICENSE.
param(
    [int] $TimeoutSeconds = 120,
    [ValidateSet('Debug', 'Release')] [string] $Configuration = 'Release',
    [ValidateSet('x64', 'ARM64')] [string] $Platform = 'x64',
    [switch] $Preview,
    [switch] $BidiPreview,
    [ValidateSet('All', 'Search', 'NativeStorage', 'RegexMemory', 'Persistence', 'SessionResilience', 'Performance', 'Syntax', 'Diff')]
    [string] $TestGroup = 'All'
)
$arguments = if ($BidiPreview) { '--bidi-preview' }
    elseif ($Preview) { '--preview' }
    else { switch ($TestGroup) {
        'Search' { '--search' }
        'NativeStorage' { '--native-storage' }
        'RegexMemory' { '--regex-memory' }
        'Persistence' { '--persistence' }
        'SessionResilience' { '--session-resilience' }
        'Performance' { '--performance' }
        'Syntax' { '--syntax' }
        'Diff' { '--diff' }
        default { '' }
    } }
& (Join-Path $PSScriptRoot 'Test-IsolatedUwp.ps1') -Project Editor -Platform $Platform -Configuration $Configuration `
    -TestArguments @($arguments) -TimeoutSeconds $TimeoutSeconds -WaitForUserExit:($Preview -or $BidiPreview)
