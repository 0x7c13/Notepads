# Notepads App [ Beta v0.9.1.0 ]

Get the latest version of Notepads in the [Microsoft Store](https://www.microsoft.com/store/apps/9nhl4nsc67wm). 

*Only works on Windows 10 1809 and above.

## What is Notepads and why do I care?

I have waited long enough for a modern windows 10 notepad app to come before I have to do it myself. Don’t get me wrong, Notepad++, Atom, VS Code and Sublime are good text editors. I have used most of them and I will continue use them in the future. However, they are either too heavy or looks old. I really need a win32 notepad.exe replacement that feels light, simple and clean to help me get things done as quickly as possible and use it as a turnaround text editor to quickly modify config files or write some notes. Most importantly, it has to be blazingly fast and beautiful. 

So here comes the “Notepads” 🎉 (s stands for Sets).

* A modern, stylish text editor with minimum design.
* Blazingly fast, feels like win32 notepad.exe but looks better.
* Launch from command line or powershell by typing: "notepads" or "notepads %path-to-your-file%".
* Multi-line handwriting support.
* Built-in Markdown file preview viewer + built-in diff viewer. [Work in progress]

![Screenshot Dark](ScreenShots/Notepads_SC_B_2.png?raw=true "Dark")
![Screenshot Light_ThemeSettings](ScreenShots/Notepads_SC_W_2.png?raw=true "Light")

******* 📣 Notepads App is still under active development. *******

## Things are not working or not implemented in this version:

* File Print.
* App is in English only. (I will add more later)
* Markdown file preview viewer + built-in diff viewer.

## Supported windows versions:

* Only works on Windows 10 1809 and above. Please check your Windows 10 version before installation.

## Cheat sheet:

* Ctrl+N/T to create new tab.
* Ctrl+(Shift)+Tab to switch between tabs.
* Ctrl+"+" to increase font size and Ctrl+"-" to decrease font size. Ctrl+"0" to reset font size to default.

## Author’s Notes:

The beta is intended to collect feedback, report bugs and glitches. For more info and issue reporting, please use [Github Issues](https://github.com/JasonStein/Notepads/issues). You can also join Notepads Discord server and chat with me directly: [Notepads Discord Server](https://discord.gg/VqetCub)

## Platform limitation (UWP):

* If you drag a file into Notepads, file save picker will ask you to save it before closing due to UWP restriction.
* Editor view resizing glitch is caused by system control and won't fix for now.
* You won't be able to save files to system folders due to UWP restriction (windows, system32 etc.).
* Notepads does not work well with large file, so I am setting the file size limit to 1MB for now.
* You can not associate potentially harmful file types (.ps1, .bat, .xaml etc) with Notepads.

## Downloads:

Please head over to [Github Releases](https://github.com/JasonStein/Notepads/releases) section to download latest release.
Or get the latest version of Notepads in the [Microsoft Store](https://www.microsoft.com/store/apps/9nhl4nsc67wm).

## Roadmap:

### Phase 0 (3 weeks - 1 month):

* Have all basic features implemented including find and replace, print and settings.
* Fix active bugs and glitches as much as possible.
* Launch it on Microsoft Store and setup release pipeline on GitHub.
* Create a landing website -> www.NotepadsApp.com

### Phase 1 (2-3 months):

* Improve Find and Replace UX.
* Automatic file save and restore.
* Add Markdown file preview.
* Add diff viewer.

### Phase 2 (By end of 2019):

* I will visit .Net core 3.0, Windows Apps, Xaml Island to see if I can port it over to win32 or WPF to get rid of UWP limitations and restrictions.

## Disclaimer and Privacy statement:

To be 100% transparent, Notepads is not and will never collect user information in terms of user privacy. I might use analytics tools to collect usage data like how many times it has been downloaded or been used but that’s it. I will not track your IP or listen your typings or read any of your files and send it over to me, or third parties. Feel free to check the source code as well.

On the other hand, you might noticed that I work for Microsoft. However, Notepads is just my personal side project and I do it for fun and for good (To empower every person and every organization on the planet to achieve more😃). I do not work for Windows team, nor do I work for any Microsoft’s UX/App team. I am not expert on creating Windows apps either. I learned how to code UWP as soon as I started this project which is like only few weeks back. So don’t put too much hope on me or treat it as a project sponsored by Microsoft.

## Special Thanks:

* [Yi Zhou](http://zhouyiwork.com/) - App icon designer, Notepads App Icon was inspired by the new icon for Windows Terminal. 
* Alexandru Sterpu - App Tester, who helped me a lot during preview/beta testing.

## Stay tuned 📢:

* [Original Reddit Post](https://www.reddit.com/r/Windows10/comments/btx5qs/my_design_implementation_of_modern_fluent_notepad/)
* [Notepads Discord Server](https://discord.gg/VqetCub)
