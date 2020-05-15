namespace Notepads.Controls.Dialog
{
    using Windows.ApplicationModel.Resources;
    using Windows.UI;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Notepads.Services;

    public class NotepadsDialog : ContentDialog
    {
        public bool IsAborted = false;

        public NotepadsDialog()
        {
            RequestedTheme = ThemeSettingsService.ThemeMode;
        }

        internal readonly ResourceLoader ResourceLoader = ResourceLoader.GetForCurrentView();

        internal static Style GetButtonStyle(Color backgroundColor)
        {
            var buttonStyle = new Windows.UI.Xaml.Style(typeof(Button));
            buttonStyle.Setters.Add(new Setter(Control.BackgroundProperty, backgroundColor));
            buttonStyle.Setters.Add(new Setter(Control.ForegroundProperty, Colors.White));
            return buttonStyle;
        }
    }
}