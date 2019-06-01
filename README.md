# Notepads App [Preview]

******* This is a preview release of Notepads for testing purposes only. *******

Notes: Please do not share it with others. Design and features are not final, same for the App icon.

## Things that suppose to work in this preview version:

* Launch from command line or PowerShell: "notepads", "notepads <file>.txt" or with absolute path: "notepads foo\bar\<file>.txt"
* Select either single or multiple text files and right click to launch with Notepads is supported.
* All basic functions including new tab, save, save as and open file should work without problem.
* Right click context menu is also there and it changes based on mode (Insertion or Selection).
* Line Ending is based on existing context of your open file. CRLF will be used as default for New Document.txt.
* Encoding is based on existing context of your open file. UTF-8 will be used as default for New Document.txt.
* Save reminder on tab closing or app exiting when there is unsaved content.
* Ctrl+Tab / Ctrl+Shift+Tab to switch tab back and forward. Ctrl+N to create new tab and Ctrl+W to close active tab.

## Things are not working or not implemented in this preview version:

* Print, Search and Replace.
* Settings and any customization related.
* You won't be able to save files to system folders due to UWP restriction (I am waiting for the upcoming .Net Core / Win Apps / Xaml Island meta).

## Known issues:

* Only works on Windows 10 1809 and 1903 for now. Please check your Windows 10 version before installation.
* All tooltips hover are shown as CTRL+N.
* If you drag a file into Notepads, file picker will be shown when saving due to UWP restriction.
* Editor view resizing glitch is caused by system control and won't fix for now.
* Save as window does not show files, only folders.

## Author Notes:

The preview is intended to collect feedback, report bugs and glitches. As well as helping me come up with a list of items that you want me to put into settings (like what part of the UI you want to customize etc. Let me know if you have any. For more info and issue reporting, please use Github issues.

## Downloads:

Please head over to GitHub release section for package downloading.
