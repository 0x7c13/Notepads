namespace Notepads.Views.MainPage
{
    using System;
    using Windows.UI.ViewManagement;
    using Windows.UI.Xaml;
    using Microsoft.AppCenter.Analytics;
    using Notepads.Services;

    public sealed partial class NotepadsMainPage
    {
        private const int TitleBarReservedAreaDefaultWidth = 180;

        private const int TitleBarReservedAreaCompactOverlayWidth = 100;

        // Show hide ExitCompactOverlayButton and status bar based on current ViewMode
        // Reset TitleBarReservedArea accordingly
        private void WindowSizeChanged(object sender, Windows.UI.Core.WindowSizeChangedEventArgs e)
        {
            if (ApplicationView.GetForCurrentView().ViewMode == ApplicationViewMode.CompactOverlay)
            {
                if (ExitCompactOverlayButton.Visibility == Visibility.Collapsed)
                {
                    TitleBarReservedArea.Width = TitleBarReservedAreaCompactOverlayWidth;
                    ExitCompactOverlayButton.Visibility = Visibility.Visible;
                    MainMenuButton.Visibility = Visibility.Collapsed;
                    if (AppSettingsService.ShowStatusBar) ShowHideStatusBar(false);
                }
            }
            else // Default or FullScreen
            {
                if (ExitCompactOverlayButton.Visibility == Visibility.Visible)
                {
                    TitleBarReservedArea.Width = TitleBarReservedAreaDefaultWidth;
                    ExitCompactOverlayButton.Visibility = Visibility.Collapsed;
                    MainMenuButton.Visibility = Visibility.Visible;
                    if (AppSettingsService.ShowStatusBar) ShowHideStatusBar(true);
                }
            }
        }

        private static async void EnterExitCompactOverlayMode()
        {
            if (App.IsGameBarWidget) return;

            if (ApplicationView.GetForCurrentView().ViewMode == ApplicationViewMode.Default)
            {
                var modeSwitched = await ApplicationView.GetForCurrentView()
                    .TryEnterViewModeAsync(ApplicationViewMode.CompactOverlay);
                if (!modeSwitched)
                {
                    LoggingService.LogError($"[{nameof(NotepadsMainPage)}] Failed to enter CompactOverlay view mode.");
                    Analytics.TrackEvent("FailedToEnterCompactOverlayViewMode");
                }
            }
            else if (ApplicationView.GetForCurrentView().ViewMode == ApplicationViewMode.CompactOverlay)
            {
                var modeSwitched = await ApplicationView.GetForCurrentView()
                    .TryEnterViewModeAsync(ApplicationViewMode.Default);
                if (!modeSwitched)
                {
                    LoggingService.LogError($"[{nameof(NotepadsMainPage)}] Failed to enter Default view mode.");
                    Analytics.TrackEvent("FailedToEnterDefaultViewMode");
                }
            }
        }

        private void EnterExitFullScreenMode()
        {
            if (App.IsGameBarWidget) return;

            if (ApplicationView.GetForCurrentView().IsFullScreenMode)
            {
                LoggingService.LogInfo($"[{nameof(NotepadsMainPage)}] Existing full screen view mode.", consoleOnly: true);
                ApplicationView.GetForCurrentView().ExitFullScreenMode();
            }
            else
            {
                if (ApplicationView.GetForCurrentView().TryEnterFullScreenMode())
                {
                    LoggingService.LogInfo($"[{nameof(NotepadsMainPage)}] Entered full screen view mode.", consoleOnly: true);
                    NotificationCenter.Instance.PostNotification(
                        _resourceLoader.GetString("TextEditor_NotificationMsg_ExitFullScreenHint"), 3000);
                }
                else
                {
                    LoggingService.LogError($"[{nameof(NotepadsMainPage)}] Failed to enter full screen view mode.");
                    Analytics.TrackEvent("FailedToEnterFullScreenViewMode");
                }
            }
        }

        private void ExitCompactOverlayButton_OnClick(object sender, RoutedEventArgs e)
        {
            if (ApplicationView.GetForCurrentView().ViewMode == ApplicationViewMode.CompactOverlay)
            {
                EnterExitCompactOverlayMode();
            }
        }
    }
}