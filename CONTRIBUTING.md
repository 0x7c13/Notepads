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
* Make sure your machine is running on Windows 10 1903+.
* Install Visual Studio 2026 with UWP .NET tools, the SDK selected by `global.json`, Windows SDK 10.0.26100.0, and the latest C++ UWP tools (v145).
* Open `src/Notepads.sln` and select Debug with x86, x64 or ARM64.
* Restore NuGet packages, then build and run the app. Debug uses the managed runtime for debugging.
* Publish Release or Production with Native AOT before testing changes to reflection, serialization, WinRT or XAML bindings. From a Visual Studio Developer PowerShell, run `msbuild src/Notepads/Notepads.csproj /restore /t:Publish /p:Configuration=Release /p:Platform=x64` (or choose x86/ARM64).

# TL;DR:
This is my first UWP project and I learn as I go. As a result, the code base is not well organized, and it is not well written. The philosophy here is to create a text editor that is easy to use, lightweight and yet stylish instead of creating another Notepad++ or VS Code in anyway. If you are looking for a code/programming editor, you might want to use VS Code instead. If you are looking for a lightweight text editor, you come to the right place. Notepads is here to help you do small things quicker and you should always install and use other editors that suit your need.
