# How to Contribute:

You can contribute to Notepads project by:
- Report issues and bugs [here](https://github.com/JasonStein/Notepads/issues)
- Submit feature requests [here](https://github.com/JasonStein/Notepads/issues)
- Create a pull request to help me (Let me know before you do so):
    * Fix an existing bug, prefix title with `fix: `.
    * Implement new features, prefix title with `feat: `.
    * Fix grammar errors or improve my documentations, prefix title with `doc: `.
    * Improve CI/CD pipeline, prefix title with `ci: `.
    * Cleanup code and code refactoring or anything else you want to change in the project not listed above, prefix title with `other: ` or assign a custom prefix with the same format (`label: `).
- Internationalization and localization:
    * My only inputs for the work here is to recommend you guys to use existing phrases that you found in win32 notepad.exe or vs code or notepad++ as much as possible. It makes your translations more consistent and easier to understand by end users.    
    * Since Notepads is still in early beta. I might change texts and add texts now and then for the upcoming months. Whenever that happens, I will notify you in [Notepads Discord Server](https://discord.gg/VqetCub) (Please join it if possible) and in [GitHub Discussions](https://github.com/JasonStein/Notepads/discussions/818) (Subscribe to notifications). If someday you lose the passion, feel free to let me know so I can assign your language to others.
    * OK, here are the steps you need to follow if you want to contribute:
        1. Make sure you can build and run Notepads project on your machine so that you can test it after your work.
        2. Click [here](https://github.com/JasonStein/Notepads/discussions/818) and provide your information.
        3. Do your work and test it on your machine and check your work to make sure it is not breaking any existing layout.
        4. Finish your work and create a PR, prefix PR title with `lang: ` (Example: https://github.com/JasonStein/Notepads/pull/30)
        5. Let me know and I will merge it if it looks good to me.

Note: This repository follows [conventional commits](https://www.conventionalcommits.org/en/v1.0.0/), format your pull request title according to specifications.

# How to Build and Run Notepads from source:
* Make sure your machine is running on Windows 10 1903+.
* Make sure you have Visual Studio 2019 16.2+ installed.
* Make sure you have "Universal Windows Platform development" component installed for Visual Studio.
* Make sure you installed "Windows 10 SDK (10.0.17763.0 + 10.0.19041.0)" as well.
* Open src/Notepads.sln with Visual Studio and set Solution Platform to x64(amd64).
* Once opened, right click on the solution and click on "Restore NuGet Packages".
* Now you should be able to build and run Notepads on your machine. If it fails, try close the solution and reopen it again.

# TL;DR:
This is my first UWP project and I learn as I go. As a result, the code base is not well organized, and it is not well written. The philosophy here is to create a text editor that is easy to use, lightweight and yet stylish instead of creating another Notepad++ or VS Code in anyway. If you are looking for a code/programming editor, you might want to use VS Code instead. If you are looking for a lightweight text editor, you come to the right place. Notepads is here to help you do small things quicker and you should always install and use other editors that suit your need.
