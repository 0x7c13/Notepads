namespace Notepads.Controls.Dialog
{
    using Windows.ApplicationModel.Resources;
    using Windows.UI;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media;
    using Notepads.Services;
    using Microsoft.Toolkit.Uwp.Helpers;

    public class NotepadsDialog : ContentDialog
    {
        public bool IsAborted = false;

        private readonly SolidColorBrush _darkModeBackgroundBrush = new SolidColorBrush("#101010".ToColor());
        private readonly SolidColorBrush _lightModeBackgroundBrush = new SolidColorBrush(Colors.White);

        public NotepadsDialog()
        {
            RequestedTheme = ThemeSettingsService.GetActualTheme(ThemeSettingsService.ThemeMode);
            Background = RequestedTheme == ElementTheme.Dark
                ? _darkModeBackgroundBrush
                : _lightModeBackgroundBrush;

            ActualThemeChanged += NotepadsDialog_ActualThemeChanged;
        }

        private void NotepadsDialog_ActualThemeChanged(FrameworkElement sender, object args)
        {
            Background = ActualTheme == ElementTheme.Dark
                ? _darkModeBackgroundBrush
                : _lightModeBackgroundBrush;
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