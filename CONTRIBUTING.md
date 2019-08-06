# How to Contribute:

You can contribute to Notepads project by:
- Report issues and bugs [here](https://github.com/JasonStein/Notepads/issues)
- Submit feature requests [here](https://github.com/JasonStein/Notepads/issues)
- Create a pull request to help me (Let me know before you do so):
    * Fix an existing bug.
    * Implement new features.
    * Cleanup code and code refactoring.
    * Fix grammar errors or improve my documentations.
- Internationalization and localization:
    * My only inputs for the work here is to recommend you guys to use existing phrases that you found in win32 notepad.exe or vs code or notepad++ as much as possible. It makes your translations more consistent and easier to understand by end users.    
    * Since Notepads is still in early beta. I might change texts and add texts now and then for the upcoming months. Whenever that happens, I will notify you in [Notepads Discord Server](https://discord.gg/VqetCub) (Please join it if possible). If someday you lose the passion, feel free to let me know so I can assign your language to others.
    * OK, here are the steps you need to follow if you want to contribute:
        1. Make sure you can build and run Notepads project on your machine so that you can test it after your work.
        2. Click [here](https://github.com/JasonStein/Notepads/issues/33) and provide your information.
        3. Do your work and test it on your machine and check your work to make sure it is not breaking any existing layout.
        4. Finish your work and create a PR (Example: https://github.com/JasonStein/Notepads/pull/30)
        5. Let me know and I will merge it if it looks good to me.

# How to Build and Run Notepads.sln:
* Make sure your machine is running on Windows 10 1809+.
* Make sure you have Visual Studio 2019 16.1+ installed.
* Make sure you have "Universal Windows Platform development" component installed for Visual Studio.
* Make sure you installed "Windows 10 SDK (10.0.18362.0)" as well.
* Open Notepads.sln with Visual Studio and set Solution Platform to x64(amd64).
* Right click on the solution and click on "Restore NuGet Packages".
* Now you should be able to build and run Notepads on your machine. If it fails, try close the solution and reopen it again.

# Additional Info:
This is my first UWP project and I learn as I go. So, the code base is not well organized, and it is not well written. Btw, I am not using MVVM pattern at all, so deal with it for now. I am sorry if my code makes you hard to understand and I know there is a lot to improve. The only thing that I want to mention here is that you should not use my code as your UWP tutorial or guide. However, I am still proud that I managed to finish the whole project in a month, and have it released on Microsoft Store. For the upcoming months, I will put more efforts on making it more reliable and yet easy to use. The philosophy is to create a text editor that easy to use, light weighted and yet stylish instead of creating another Notepad++ or VS Code in anyway. If you are looking for a code/programming editor, you might want to use VS Code instead. If you are looking for a lightweight text editor, you come to the right place. As I said, Notepads is here to help you do small things quicker and you should always install and use other editors that suit your need.
