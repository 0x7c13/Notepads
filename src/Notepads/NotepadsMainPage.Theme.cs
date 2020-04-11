namespace Notepads
{
    using System;
    using Windows.UI;
    using Windows.UI.ViewManagement;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Media;
    using Notepads.Services;
    using Notepads.Utilities;

    public sealed partial class NotepadsMainPage
    {
        private void InitializeThemeSettings()
        {
            ThemeSettingsService.SetRequestedTheme(RootGrid, Window.Current.Content, ApplicationView.GetForCurrentView().TitleBar, Application.Current.RequestedTheme);
            ThemeSettingsService.OnBackgroundChanged += ThemeSettingsService_OnBackgroundChanged;
            ThemeSettingsService.OnThemeChanged += ThemeSettingsService_OnThemeChanged;
            ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;
        }

        private async void ThemeSettingsService_OnAccentColorChanged(object sender, Color color)
        {
            await ThreadUtility.CallOnUIThreadAsync(Dispatcher, ThemeSettingsService.SetRequestedAccentColor);
        }

        private async void ThemeSettingsService_OnThemeChanged(object sender, EventArgs args)
        {
            await ThreadUtility.CallOnUIThreadAsync(Dispatcher, () =>
            {
                ThemeSettingsService.SetRequestedTheme(RootGrid, Window.Current.Content, ApplicationView.GetForCurrentView().TitleBar, Application.Current.RequestedTheme);
            });
        }

        private async void ThemeSettingsService_OnBackgroundChanged(object sender, Brush backgroundBrush)
        {
            await ThreadUtility.CallOnUIThreadAsync(Dispatcher, () =>
            {
                RootGrid.Background = backgroundBrush;
            });
        }
    }
}