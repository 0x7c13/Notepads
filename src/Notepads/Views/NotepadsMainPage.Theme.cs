namespace Notepads.Views
{
    using Windows.UI;
    using Windows.UI.ViewManagement;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Media;
    using Notepads.Extensions;
    using Notepads.Services;

    public sealed partial class NotepadsMainPage
    {
        private void InitializeThemeSettings()
        {
            ThemeSettingsService.SetRequestedTheme(RootGrid, Window.Current.Content, ApplicationView.GetForCurrentView().TitleBar);
            ThemeSettingsService.OnBackgroundChanged += ThemeSettingsService_OnBackgroundChanged;
            ThemeSettingsService.OnThemeChanged += ThemeSettingsService_OnThemeChanged;
            ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;
        }

        private async void ThemeSettingsService_OnAccentColorChanged(object sender, Color color)
        {
            await Dispatcher.CallOnUIThreadAsync(ThemeSettingsService.SetRequestedAccentColor);
        }

        private async void ThemeSettingsService_OnThemeChanged(object sender, ElementTheme theme)
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                ThemeSettingsService.SetRequestedTheme(RootGrid, Window.Current.Content, ApplicationView.GetForCurrentView().TitleBar);
            });
        }

        private async void ThemeSettingsService_OnBackgroundChanged(object sender, Brush backgroundBrush)
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                RootGrid.Background = backgroundBrush;
            });
        }
    }
}