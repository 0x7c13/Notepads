// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Controls.Dialog
{
    using System;
    using Windows.UI;

    public sealed class SessionCorruptionErrorDialog : NotepadsDialog
    {
        public SessionCorruptionErrorDialog(Action recoveryAction)
        {
            Title = ResourceLoader.GetString("SessionCorruptionErrorDialog_Title");
            Content = ResourceLoader.GetString("SessionCorruptionErrorDialog_Content");
            PrimaryButtonText = ResourceLoader.GetString("SessionCorruptionErrorDialog_PrimaryButtonText");
            CloseButtonText = ResourceLoader.GetString("SessionCorruptionErrorDialog_CloseButtonText");
            PrimaryButtonStyle = GetButtonStyle(Color.FromArgb(255, 255, 69, 0));
            PrimaryButtonClick += (dialog, args) => { recoveryAction(); };
        }
    }
}