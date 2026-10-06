// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Presentation.Controls.Dialog;

public sealed partial class FileOpenErrorDialog : NotepadsDialog
{
    public FileOpenErrorDialog(string filePath, string errorMsg)
    {
        Title = ResourceLoader.GetString("FileOpenErrorDialog_Title");
        Content = string.IsNullOrEmpty(filePath) ? errorMsg : string.Format(ResourceLoader.GetString("FileOpenErrorDialog_Content"), filePath, errorMsg);
        PrimaryButtonText = ResourceLoader.GetString("FileOpenErrorDialog_PrimaryButtonText");
    }
}
