// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Windows.UI;
using Windows.UI.Xaml;

namespace Notepads.Presentation.Controls.Dialog;

public sealed partial class AppCloseSaveReminderDialog : NotepadsDialog
{
    public AppCloseSaveReminderDialog(Action saveAndExitAction, Action discardAndExitAction, Action cancelAction)
    {
        Title = ResourceLoader.GetString("AppCloseSaveReminderDialog_Title");
        HorizontalAlignment = HorizontalAlignment.Center;
        Content = ResourceLoader.GetString("AppCloseSaveReminderDialog_Content");
        PrimaryButtonText = ResourceLoader.GetString("AppCloseSaveReminderDialog_PrimaryButtonText");
        SecondaryButtonText = ResourceLoader.GetString("AppCloseSaveReminderDialog_SecondaryButtonText");
        CloseButtonText = ResourceLoader.GetString("AppCloseSaveReminderDialog_CloseButtonText");
        PrimaryButtonStyle = GetButtonStyle(Color.FromArgb(255, 38, 114, 201));

        PrimaryButtonClick += (dialog, eventArgs) => saveAndExitAction();
        SecondaryButtonClick += (dialog, eventArgs) => discardAndExitAction();
        CloseButtonClick += (dialog, eventArgs) => cancelAction();
    }
}
