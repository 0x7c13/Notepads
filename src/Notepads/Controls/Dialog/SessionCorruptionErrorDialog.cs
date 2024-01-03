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