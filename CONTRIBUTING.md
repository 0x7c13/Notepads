# How to Contribute:

You can contribute to Notepads project by:
- Report issues and bugs [here](https://github.com/0x7c13/Notepads/issues)
- Submit feature requests [here](https://github.com/0x7c13/Notepads/issues)
- Create a pull request to help me (Let me know before you do so):
    * Fix an existing bug, prefix title with `fix: `.
    * Implement new features, prefix title with `feat: `.
    * Fix grammar errors or improve my documentations, prefix title with `doc: `.
    * Improve CI/CD pipeline, prefix title with `ci: `.
    * Cleanup code and code refactoring or anything else you want to change in the project not listed above, prefix title with `other: ` or assign a custom prefix with the same format (`label: `).
- Internationalization and localization:
    * My only inputs for the work here is to recommend you guys to use existing phrases that you found in win32 notepad.exe or vs code or notepad++ as much as possible. It makes your translations more consistent and easier to understand by end users.    
    * Since Notepads is still in early beta. I might change texts and add texts now and then for the upcoming months. Whenever that happens, I will notify you in [Notepads Discord Server](https://discord.gg/VqetCub) (Please join it if possible) and in [GitHub Discussions](https://github.com/0x7c13/Notepads/discussions/818) (Subscribe to notifications). If someday you lose the passion, feel free to let me know so I can assign your language to others.
    * OK, here are the steps you need to follow if you want to contribute:
        1. Make sure you can build and run Notepads project on your machine so that you can test it after your work.
        2. Click [here](https://github.com/0x7c13/Notepads/discussions/818) and provide your information.
        3. Do your work and test it on your machine and check your work to make sure it is not breaking any existing layout.
        4. Finish your work and create a PR, prefix PR title with `lang: ` (Example: https://github.com/0x7c13/Notepads/pull/30)
        5. Let me know and I will merge it if it looks good to me.
        Notes: You should use the language code as your folder name listed here: https://docs.microsoft.com/en-us/windows/uwp/publish/supported-languages

Note: This repository follows [conventional commits](https://www.conventionalcommits.org/en/v1.0.0/), format your pull request title according to specifications.

# How to Build and Run Notepads from source:

V2 is work in progress. Only compatibility with released V1 data is required;
intermediate V2 development formats are not release contracts.

On a Windows x64 or ARM64 machine, download/clone this repository, open Windows
PowerShell in its root, and run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\Run-Dev.ps1
```

The script reuses a complete toolchain when available. Otherwise it downloads the
native-host .NET SDK selected by `global.json` into `.tools/notepads-dev` and installs
**Build Tools 2026** with v145 C++/UWP, managed UWP tasks and Windows SDK 10.0.26100.
Visual Studio IDE and PowerShell 7 are unnecessary. Initial setup needs internet,
several GB of free space and administrator approval for Build Tools or Windows
Developer Mode. Microsoft installers come from official HTTPS endpoints; the script
checks their signature and the SDK archive's SHA-512 before using them.

The default is **Release Native AOT**, targeting the host architecture. It restores
dependencies, builds, registers the `Notepads-Dev` layout and launches it. This is a
separate identity from the Store app; rerunning updates the development registration
and retains its data. Close an existing development instance before updating. Logs,
framework packages and the registered layout live in `artifacts/dev/<run-id>`;
**keep that layout while it is registered**. No signing certificate is required for
this local development registration.

```powershell
.\scripts\Run-Dev.ps1 -CheckOnly              # Inspect tools; no downloads/build/deployment
.\scripts\Run-Dev.ps1 -NoInstall              # Build/run using existing tools only
.\scripts\Run-Dev.ps1 -BuildOnly              # Build/extract; no registration or launch
.\scripts\Run-Dev.ps1 -Configuration Debug    # Managed runtime, faster development builds
.\scripts\Run-Dev.ps1 -Platform x64 -BuildOnly # Also supports x86 and ARM64
```

`-CheckOnly` also reports Developer Mode without changing it. `-BuildOnly` skips
Developer Mode and registration; `-NoInstall` skips toolchain installation but
still registers/runs the app and enables Developer Mode if needed.

For offline testers, open a CI run's **Artifacts** and download
`notepads-offline-x64`, `notepads-offline-ARM64` or `notepads-offline-x86` for the target
machine. These are regular CI outputs, not a separate manual workflow.
Alternatively, run `.\scripts\New-OfflinePackage.ps1` in Windows PowerShell.
This builds Production Native AOT for x64/ARM64 and writes
a ZIP plus SHA-256 receipt to your desktop. It contains the app, matching framework
packages, a PowerShell 5.1 installer and instructions; testers extract it to a
permanent folder and double-click `Start.cmd`. No toolchain or network download is
needed on the target. `Install.ps1 -CheckOnly` verifies files/dependencies without
changing settings or registration. These builds display **Notepads**, with no Dev
label. They use the Production identity `19282JackieLiu.Notepads-Beta` and can update
an existing package with that identity and reuse its data; other identities remain
separate. Save/back up work before trying V2. The installer never uninstalls apps,
resets data or force-closes processes. Keep the extracted folder after installation.

New installations enable session snapshots and use four spaces for Tab by default.
Existing saved preferences and detected file indentation are preserved.

Build Tools has its own [Microsoft license terms](https://visualstudio.microsoft.com/license-terms/vs2026-ga-diagnostic-buildtools/);
omitting the IDE does not remove license requirements. The terms generally require
a qualifying Visual Studio license, with a narrower exception for building third-party
open-source C++ dependencies. Check eligibility for the free Community license or
your organization's license before use.

For IDE debugging, install Visual Studio 2026 with the same components, open
`src/Notepads.sln`, and choose Debug and x86/x64/ARM64. Validate changes to reflection,
serialization, WinRT or XAML bindings in an actual Release Native AOT run as well.

Run `./scripts/Test.ps1` for fast unit tests. Select the relevant integration groups
with `./scripts/Test.ps1 -Suite Integration -List`; see the [test guide](tests/README.md)
for isolation requirements and coverage boundaries. Tests use `Notepads.Editor.Tests`,
not your installed Notepads or development package. Physical input tests require an
exclusive desktop. Changes to WinRT, XAML, trimming or serialization also need an
actual Release Native AOT application test run.

[CI.md](CI.md) documents the required GitHub checks, local reproduction commands,
unsigned package artifacts, hosted UI tests and optional desktop workflow. CI retains the
current Windows build 17763 minimum and x86/x64/ARM64 package matrix; a future
platform decision must update the projects, package checks and CI together.

# TL;DR:
This is my first UWP project and I learn as I go. As a result, the code base is not well organized, and it is not well written. The philosophy here is to create a text editor that is easy to use, lightweight and yet stylish instead of creating another Notepad++ or VS Code in anyway. If you are looking for a code/programming editor, you might want to use VS Code instead. If you are looking for a lightweight text editor, you come to the right place. Notepads is here to help you do small things quicker and you should always install and use other editors that suit your need.
