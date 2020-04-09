namespace Notepads.Controls.Settings
{
    using System;
    using Notepads.Services;
    using Notepads.Utilities;
    using Windows.ApplicationModel;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media.Imaging;

    public sealed partial class AboutPage : Page
    {
        public string AppVersion => $"v{GetAppVersion()}";

        public AboutPage()
        {
            InitializeComponent();
            SetAppIconBasedOnTheme(ThemeSettingsService.ThemeMode);

            Loaded += AboutPage_Loaded;
            Unloaded += AboutPage_Unloaded;
        }

        private void AboutPage_Loaded(object sender, RoutedEventArgs e)
        {
            ThemeSettingsService.OnThemeChanged += ThemeSettingsService_OnThemeChanged;
        }

        private void AboutPage_Unloaded(object sender, RoutedEventArgs e)
        {
            ThemeSettingsService.OnThemeChanged -= ThemeSettingsService_OnThemeChanged;
        }

        private async void ThemeSettingsService_OnThemeChanged(object sender, EventArgs args)
        {
            await ThreadUtility.CallOnUIThreadAsync(Dispatcher, () =>
            {
                SetAppIconBasedOnTheme(ThemeSettingsService.UseWindowsTheme
                    ? ThemeSettingsService.ApplicationThemeToElementTheme(Application.Current.RequestedTheme)
                    : ThemeSettingsService.ThemeMode);
            });
        }

        private void SetAppIconBasedOnTheme(ElementTheme theme)
        {
            if (theme == ElementTheme.Dark || theme == ElementTheme.Default)
            {
                AppIconImage.Source = new BitmapImage(new Uri("ms-appx:///Assets/appicon_bs.png"));
            }
            else
            {
                AppIconImage.Source = new BitmapImage(new Uri("ms-appx:///Assets/appicon_ws.png"));
            }
        }

        private static string GetAppVersion()
        {
            PackageVersion version = Package.Current.Id.Version;
            return $"{version.Major}.{version.Minor}.{version.Build}.{version.Revision}";
        }
    }
}