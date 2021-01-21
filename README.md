<p align="center">
  <img width="128" align="center" src="src/Notepads/Assets/appicon_ws.gif">
</p>
<h1 align="center">
  Notepads
</h1>
<p align="center">
  A modern, lightweight text editor with a minimalist design.
</p>
<p align="center">
  <a style="text-decoration:none" href="https://www.microsoft.com/store/apps/9nhl4nsc67wm">
    <img src="https://img.shields.io/badge/Microsoft%20Store-Download-orange.svg?style=flat-square" alt="Store link" />
  </a>
  <a style="text-decoration:none" href="https://github.com/JasonStein/Notepads/releases">
    <img src="https://img.shields.io/github/release/jasonstein/notepads.svg?label=latest%20version&style=flat-square" alt="Releases" />
  </a>
  <a style="text-decoration:none">
    <img src="https://img.shields.io/badge/platform-windows%2010%20%7C%20uwp-yellow.svg?style=flat-square" alt="Platform" />
  </a>
  <a style="text-decoration:none" href="https://discord.gg/VqetCub">
    <img src="https://img.shields.io/discord/588473626651787274.svg?style=flat-square" alt="Discord" />
  </a>
</p>

## What is Notepads and why do I care?

I have been waiting long enough for a modern Windows 10 notepad app to come before I decided to create one myself. Don’t get me wrong, Notepad++, VS Code, and Sublime are great text editors. I have used them all and I will continue to use them in the future. However, they are either too heavy or look less appealing. There are times that I just wanted to use Windows notepad for things like writing notes or editing config files. So I decided to create a win32 notepad replacement here and try to give it a modern look and feel. Most importantly, it has to be blazingly fast and appeal to everyone.

So here comes the “Notepads” 🎉 (s stands for Sets).

* Fluent design with a built-in tab system.
* Blazingly fast and lightweight.
* Launch from the command line or PowerShell by typing: `notepads` or `notepads %FilePath%`.
* Multi-line handwriting support.
* Built-in Markdown live preview.
* Built-in diff viewer (preview your changes).
* Session snapshot and multi-instances.

![Screenshot Dark](ScreenShots/1.png?raw=true "Dark")
![Screenshot Markdown](ScreenShots/2.png?raw=true "Markdown")
![Screenshot DiffViewer](ScreenShots/3.png?raw=true "DiffViewer")
![Screenshot Light](ScreenShots/4.png?raw=true "Light")

******* 📣 Notepads App is still under active development. *******

## Status update

[[08-12-2019] Status update and KPI report](https://github.com/JasonStein/Notepads/issues/138)

![image](https://user-images.githubusercontent.com/1166162/62916469-75eb5800-bd4d-11e9-9f97-a632792400c0.png)

## Shortcuts:

* Ctrl+N/T to create new tab.
* Ctrl+(Shift)+Tab to switch between tabs.
* Ctrl+Num(1-9) to quickly switch to specified tab.
* Ctrl+"+"/"-" for zooming. Ctrl+"0" to reset zooming to default.
* Ctrl+L/R to change text flow direction. (LTR/RTL)
* Alt+P to toggle preview split view for Markdown file.
* Alt+D to toggle side-by-side diff viewer.

## Platform limitations (UWP):

* You won't be able to save files to system folders due to UWP restriction (windows, system32, etc.).
* You cannot associate potentially harmful file types (.cmd, .bat etc.) with Notepads.
* Notepads does not work well with large files; the file size limit is set to 1MB for now. I will add large file support later.

## Downloads:

Notepads is available in Microsoft Store. You can get the latest version of Notepads here for free: [Microsoft Store Link](https://www.microsoft.com/store/apps/9nhl4nsc67wm).

You can also use Windows Package Manager to install notepads:
```cmd
winget install notepads
```

## Roadmap:

* [Project Roadmap](ROADMAP.md)

## Changelog:

* [Notepads Releases](https://github.com/JasonStein/Notepads/releases)

## Disclaimer and Privacy statement:

To be 100% transparent:

* Notepads does not and will never collect user information in terms of user privacy.
* I will not track your IP. 
* I will not record your typings or read any of your files created in Notepads including file name and file path. 
* No typings or files will be sent to me or third parties. 

I am using analytics service "AppCenter" to collect basic usage data plus some minimum telemetry to help me debug runtime errors. Here is the thread I made clear on this topic: https://github.com/JasonStein/Notepads/issues/334 

Feel free to review the source code or build your own version of Notepads since it is 100% open sourced.

You might notice that I work for Microsoft but Notepads is my personal project that I accomplish during free time (to empower every person and every organization on the planet to achieve more😃). I do not work for the Windows team, nor do I work for a Microsoft UX/App team. I am not expert on creating Windows apps either. I learned how to code UWP as soon as I started this project, so don’t put too much hope on me or treat it as a project sponsored by Microsoft.

## Contributing:

* [How to contribute?](CONTRIBUTING.md)
* Notepads is free and open source, if you like my work, please consider:
   * Star this project on GitHub
   * Leave me a review [here](https://www.microsoft.com/store/apps/9nhl4nsc67wm)
   * [![ko-fi](https://www.ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/D1D6Y3C6)

## Dependencies and References:
* [Windows Community Toolkit](https://github.com/windows-toolkit/WindowsCommunityToolkit)
* [XAML Controls Gallery](https://github.com/microsoft/Xaml-Controls-Gallery)
* [Windows UI Library](https://github.com/Microsoft/microsoft-ui-xaml)
* [ColorCode Universal](https://github.com/WilliamABradley/ColorCode-Universal)
* [UTF Unknown](https://github.com/CharsetDetector/UTF-unknown)
* [DiffPlex](https://github.com/mmanela/diffplex)
* [Win2D](https://github.com/microsoft/Win2D)

## Special Thanks:

* [Yi Zhou](http://zhouyiwork.com/) - App icon designer, Notepads App Icon (old) is greatly inspired by the new icon for Windows Terminal.
* [Mahmoud Qurashy](https://github.com/mah-qurashy) - App icon and file icon(s) designer, creator of the new Notepads App Icon.

* Alexandru Sterpu - App Tester, who helped me a lot during preview/beta testing.
* Code Contributors: [DanverZ](https://github.com/chenghanzou), [BernhardWebstudio](https://github.com/BernhardWebstudio), [Csányi István](https://github.com/AmionSky), [Pavel Erokhin](https://github.com/MairwunNx), [Sergio Pedri](https://github.com/Sergio0694), [Lucas Pinho B. Santos](https://github.com/pinholucas), [Soumya Ranjan Mahunt](https://github.com/soumyamahunt), [Belleve Invis](https://github.com/be5invis), [Maickonn Richard](https://github.com/Maickonn), [Xam](https://github.com/XamDR)
* Documentation Contributors: [Craig S.](https://github.com/sercraig)
* Localization Contributors: 
    * [fr-FR][French (France)]: [François Rousselet](https://github.com/frousselet), [François-Joseph du Fou](https://github.com/FJduFou), [Armand Delessert](https://github.com/ArmandDelessert)
    * [es-ES][Spanish (Spain)]: [Jose Pinilla](https://github.com/joseppinilla)
    * [zh-CN][Chinese (S)]: [lindexi](https://github.com/lindexi), [walterlv](https://github.com/walterlv), [Jackie Liu](https://github.com/JasonStein)
    * [hu-HU][Hungarian (Hungary)]: [Csányi István](https://github.com/AmionSky), [Kristóf Kékesi](https://github.com/KristofKekesi)
    * [tr-TR][Turkish (Turkey)]: [Mert Can Demir](https://github.com/validatedev)
    * [ja-JP][Japanese (Japan)]: [Mamoru Satoh](https://github.com/pnp0a03)
    * [de-DE][German (Germany)]/[de-CH][German (Switzerland)]: [Walter Wolf](https://github.com/WalterWolf49)
    * [ru-RU][Russian (Russia)]: [Pavel Erokhin](https://github.com/MairwunNx), [krlvm](https://github.com/krlvm)
    * [fi-FI][Finnish (Finland)]: [Esa Elo](https://github.com/sauihdik)
    * [uk-UA][Ukrainian (Ukraine)]: [Taras Fomin aka Tarik02](https://github.com/Tarik02)
    * [it-IT][Italian (Italy)]: [Andrea Guarinoni](https://github.com/guari)
    * [cs-CZ][Czech (Czech Republic)]: [Jan Rajnoha](https://github.com/JanRajnoha)
    * [pt-BR][Portuguese (Brazil)]: [Lucas Pinho B. Santos](https://github.com/pinholucas)
    * [ko-KR][Korean (Korea)]: [Donghyeok Tak](https://github.com/tdh8316)
    * [hi-IN][Hindi (India)]/[or-IN][Odia (India)]: [Soumya Ranjan Mahunt](https://github.com/soumyamahunt)
    * [pl-PL][Polish (Poland)]: [Daxxxis](https://github.com/Daxxxis)
    * [ka-GE][Georgian (Georgia)]: [guram mazanashvili](https://github.com/gmaza)
    * [hr-HR][Croatian (Croatia)]: [milotype](https://github.com/milotype)
    * [zh-TW][Chinese (T)]: [Tony Yao](https://github.com/SeaBao)

[![](https://sourcerer.io/fame/JasonStein/JasonStein/Notepads/images/0)](https://sourcerer.io/fame/JasonStein/JasonStein/Notepads/links/0)[![](https://sourcerer.io/fame/JasonStein/JasonStein/Notepads/images/1)](https://sourcerer.io/fame/JasonStein/JasonStein/Notepads/links/1)[![](https://sourcerer.io/fame/JasonStein/JasonStein/Notepads/images/2)](https://sourcerer.io/fame/JasonStein/JasonStein/Notepads/links/2)[![](https://sourcerer.io/fame/JasonStein/JasonStein/Notepads/images/3)](https://sourcerer.io/fame/JasonStein/JasonStein/Notepads/links/3)[![](https://sourcerer.io/fame/JasonStein/JasonStein/Notepads/images/4)](https://sourcerer.io/fame/JasonStein/JasonStein/Notepads/links/4)[![](https://sourcerer.io/fame/JasonStein/JasonStein/Notepads/images/5)](https://sourcerer.io/fame/JasonStein/JasonStein/Notepads/links/5)[![](https://sourcerer.io/fame/JasonStein/JasonStein/Notepads/images/6)](https://sourcerer.io/fame/JasonStein/JasonStein/Notepads/links/6)[![](https://sourcerer.io/fame/JasonStein/JasonStein/Notepads/images/7)](https://sourcerer.io/fame/JasonStein/JasonStein/Notepads/links/7)

## Stay tuned 📢:
* [Notepads Discord Server](https://discord.gg/VqetCub)
