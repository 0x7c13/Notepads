// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Windows.Storage;

namespace Notepads.Presentation.Controls.Dialog;

public sealed partial class FileSaveErrorDialog : NotepadsDialog
{
    internal FileSaveErrorDialog(StorageFile file, string errorMsg, bool offerActions = false, bool allowRetry = true, bool committed = false)
        : this(string.IsNullOrEmpty(file?.Path) ? file?.Name : file.Path, errorMsg, offerActions, allowRetry, committed)
    {
    }

    public FileSaveErrorDialog(string filePath, string errorMsg, bool offerActions = false, bool allowRetry = true, bool committed = false)
    {
        Title = ResourceLoader.GetString(committed ? "FileSaveErrorDialog_Unverified" : "FileSaveErrorDialog_Title");
        Content = committed ? string.Format(errorMsg, filePath) : string.IsNullOrEmpty(filePath) ? errorMsg :
            string.Format(ResourceLoader.GetString("FileSaveErrorDialog_Content"), filePath, errorMsg);
        if (offerActions)
        {
            if (allowRetry) PrimaryButtonText = ResourceLoader.GetString(committed ? "FileSaveErrorDialog_CheckAgain" : "FileSaveErrorDialog_Retry");
            SecondaryButtonText = ResourceLoader.GetString("MainMenu_Button_SaveAs/Text");
            CloseButtonText = ResourceLoader.GetString("AppCloseSaveReminderDialog_CloseButtonText");
        }
        else PrimaryButtonText = ResourceLoader.GetString("FileSaveErrorDialog_PrimaryButtonText");
    }
}
