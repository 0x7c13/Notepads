namespace Notepads
{
    using System;
    using Windows.UI.ViewManagement;
    using Windows.UI.Xaml;
    using Microsoft.AppCenter.Analytics;
    using Notepads.Services;

    public sealed partial class NotepadsMainPage
    {
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
                    if (EditorSettingsService.ShowStatusBar) ShowHideStatusBar(false);
                }
            }
            else // Default or FullScreen
            {
                if (ExitCompactOverlayButton.Visibility == Visibility.Visible)
                {
                    TitleBarReservedArea.Width = TitleBarReservedAreaDefaultWidth;
                    ExitCompactOverlayButton.Visibility = Visibility.Collapsed;
                    if (EditorSettingsService.ShowStatusBar) ShowHideStatusBar(true);
                }
            }
        }

        private async void EnterExitCompactOverlayMode()
        {
            if (App.IsGameBarWidget) return;

            if (ApplicationView.GetForCurrentView().ViewMode == ApplicationViewMode.Default)
            {
                var modeSwitched = await ApplicationView.GetForCurrentView()
                    .TryEnterViewModeAsync(ApplicationViewMode.CompactOverlay);
                if (!modeSwitched)
                {
                    LoggingService.LogError("Failed to enter CompactOverlay view mode.");
                    Analytics.TrackEvent("FailedToEnterCompactOverlayViewMode");
                }
            }
            else if (ApplicationView.GetForCurrentView().ViewMode == ApplicationViewMode.CompactOverlay)
            {
                var modeSwitched = await ApplicationView.GetForCurrentView()
                    .TryEnterViewModeAsync(ApplicationViewMode.Default);
                if (!modeSwitched)
                {
                    LoggingService.LogError("Failed to enter Default view mode.");
                    Analytics.TrackEvent("FailedToEnterDefaultViewMode");
                }
            }
        }

        private void EnterExitFullScreenMode()
        {
            if (App.IsGameBarWidget) return;

            if (ApplicationView.GetForCurrentView().IsFullScreenMode)
            {
                LoggingService.LogInfo("Existing full screen view mode.", consoleOnly: true);
                ApplicationView.GetForCurrentView().ExitFullScreenMode();
            }
            else
            {
                if (ApplicationView.GetForCurrentView().TryEnterFullScreenMode())
                {
                    LoggingService.LogInfo("Entered full screen view mode.", consoleOnly: true);
                    NotificationCenter.Instance.PostNotification(
                        _resourceLoader.GetString("TextEditor_NotificationMsg_ExitFullScreenHint"), 3000);
                }
                else
                {
                    LoggingService.LogError("Failed to enter full screen view mode.");
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