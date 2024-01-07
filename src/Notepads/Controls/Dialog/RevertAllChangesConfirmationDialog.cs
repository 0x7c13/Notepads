// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Controls.Dialog
{
    using System;

    public sealed class RevertAllChangesConfirmationDialog : NotepadsDialog
    {
        public RevertAllChangesConfirmationDialog(string fileNameOrPath, Action confirmedAction)
        {
            Title = ResourceLoader.GetString("RevertAllChangesConfirmationDialog_Title");
            Content = string.Format(ResourceLoader.GetString("RevertAllChangesConfirmationDialog_Content"), fileNameOrPath);
            PrimaryButtonText = ResourceLoader.GetString("RevertAllChangesConfirmationDialog_PrimaryButtonText");
            CloseButtonText = ResourceLoader.GetString("RevertAllChangesConfirmationDialog_CloseButtonText");
            PrimaryButtonClick += (dialog, args) => { confirmedAction(); };
        }
    }
}