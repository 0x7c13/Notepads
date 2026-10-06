// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Presentation.Helpers;
using Notepads.Presentation.Theming;
using Windows.UI;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Media;

namespace Notepads.Presentation.Views.MainPage;

public sealed partial class NotepadsMainPage
{
    private void InitializeThemeSettings()
    {
        ThemeSettingsService.SetRequestedTheme(RootGrid, Window.Current.Content, ApplicationView.GetForCurrentView().TitleBar);
    }

    private async void ThemeSettingsService_OnAccentColorChanged(object sender, Color color)
    {
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (_viewEventsBound) ThemeSettingsService.SetRequestedAccentColor();
        });
    }

    private async void ThemeSettingsService_OnThemeChanged(object sender, ElementTheme theme)
    {
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (!_viewEventsBound) return;
            ThemeSettingsService.SetRequestedTheme(RootGrid, Window.Current.Content, ApplicationView.GetForCurrentView().TitleBar);
        });
    }

    private async void ThemeSettingsService_OnBackgroundChanged(object sender, Brush backgroundBrush)
    {
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (!_viewEventsBound) return;
            RootGrid.Background = backgroundBrush;
        });
    }
}
