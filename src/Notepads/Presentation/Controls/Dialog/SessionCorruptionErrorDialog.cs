// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2024-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Windows.UI;

namespace Notepads.Presentation.Controls.Dialog;

public sealed partial class SessionCorruptionErrorDialog : NotepadsDialog
{
    public SessionCorruptionErrorDialog(Action recoveryAction, bool recoveryBlocked = false)
    {
        Title = ResourceLoader.GetString("SessionCorruptionErrorDialog_Title");
        Content = ResourceLoader.GetString(recoveryBlocked ? "SessionCorruptionErrorDialog_BlockedContent" : "SessionCorruptionErrorDialog_Content");
        PrimaryButtonText = ResourceLoader.GetString("SessionCorruptionErrorDialog_PrimaryButtonText");
        CloseButtonText = ResourceLoader.GetString("SessionCorruptionErrorDialog_CloseButtonText");
        PrimaryButtonStyle = GetButtonStyle(Color.FromArgb(255, 255, 69, 0));
        PrimaryButtonClick += (dialog, args) => { recoveryAction(); };
    }
}
