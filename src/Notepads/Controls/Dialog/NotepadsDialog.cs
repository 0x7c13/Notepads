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
            // TODO: Uncomment to apply CornerRadius
            //CornerRadius = (CornerRadius)Application.Current.Resources["OverlayCornerRadius"];
            PrimaryButtonStyle = GetButtonStyle();
            SecondaryButtonStyle = GetButtonStyle();
            CloseButtonStyle = GetButtonStyle();

            RequestedTheme = ThemeSettingsService.ThemeMode;
            Background = ThemeSettingsService.ThemeMode == ElementTheme.Dark
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

        internal static Style GetButtonStyle()
        {
            var buttonStyle = new Style(typeof(Button));
            // TODO: Uncomment to apply CornerRadius
            //buttonStyle.Setters.Add(new Setter(Control.CornerRadiusProperty,
            //    (CornerRadius)Application.Current.Resources["ControlCornerRadius"]));
            return buttonStyle;
        }

        internal static Style GetButtonStyle(Color backgroundColor)
        {
            var buttonStyle = GetButtonStyle();
            buttonStyle.Setters.Add(new Setter(Control.BackgroundProperty, backgroundColor));
            buttonStyle.Setters.Add(new Setter(Control.ForegroundProperty, Colors.White));
            return buttonStyle;
        }
    }
}