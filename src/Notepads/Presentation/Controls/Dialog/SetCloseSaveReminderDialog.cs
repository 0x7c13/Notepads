// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;

namespace Notepads.Presentation.Controls.Dialog;

public sealed partial class SetCloseSaveReminderDialog : NotepadsDialog
{
    public SetCloseSaveReminderDialog(string fileNameOrPath, Action saveAction, Action skipSavingAction)
    {
        Title = ResourceLoader.GetString("SetCloseSaveReminderDialog_Title");
        Content = string.Format(ResourceLoader.GetString("SetCloseSaveReminderDialog_Content"), fileNameOrPath);
        PrimaryButtonText = ResourceLoader.GetString("SetCloseSaveReminderDialog_PrimaryButtonText");
        SecondaryButtonText = ResourceLoader.GetString("SetCloseSaveReminderDialog_SecondaryButtonText");
        CloseButtonText = ResourceLoader.GetString("SetCloseSaveReminderDialog_CloseButtonText");

        PrimaryButtonClick += (dialog, args) => { saveAction(); };
        SecondaryButtonClick += (dialog, args) => { skipSavingAction(); };
    }
}
