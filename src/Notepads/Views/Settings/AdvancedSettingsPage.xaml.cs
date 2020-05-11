namespace Notepads.Views.Settings
{
    using Notepads.Services;
    using Notepads.Settings;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;

    public sealed partial class AdvancedSettingsPage : Page
    {
        public AdvancedSettingsPage()
        {
            InitializeComponent();

            ShowStatusBarToggleSwitch.IsOn = AppSettingsService.ShowStatusBar;

            // Disable session snapshot toggle for shadow windows
            if (!App.IsFirstInstance)
            {
                EnableSessionSnapshotToggleSwitch.IsOn = false;
                EnableSessionSnapshotToggleSwitch.IsEnabled = false;
            }
            else
            {
                EnableSessionSnapshotToggleSwitch.IsOn = AppSettingsService.IsSessionSnapshotEnabled;
            }

            AlwaysOpenNewWindowToggleSwitch.IsOn = AppSettingsService.AlwaysOpenNewWindow;

            if (App.IsGameBarWidget)
            {
                // these settings don't make sense for Game Bar, there can be only one
                SessionSnapshotSettingsTitle.Visibility = Visibility.Collapsed;
                SessionSnapshotSettingsControls.Visibility = Visibility.Collapsed;
                LaunchPreferenceSettingsTitle.Visibility = Visibility.Collapsed;
                LaunchPreferenceSettingsControls.Visibility = Visibility.Collapsed;
            }

            Loaded += AdvancedSettings_Loaded;
            Unloaded += AdvancedSettings_Unloaded;
        }

        private void AdvancedSettings_Loaded(object sender, RoutedEventArgs e)
        {
            ShowStatusBarToggleSwitch.Toggled += ShowStatusBarToggleSwitch_Toggled;
            EnableSessionSnapshotToggleSwitch.Toggled += EnableSessionBackupAndRestoreToggleSwitch_Toggled;
            AlwaysOpenNewWindowToggleSwitch.Toggled += AlwaysOpenNewWindowToggleSwitch_Toggled;
        }

        private void AdvancedSettings_Unloaded(object sender, RoutedEventArgs e)
        {
            ShowStatusBarToggleSwitch.Toggled -= ShowStatusBarToggleSwitch_Toggled;
            EnableSessionSnapshotToggleSwitch.Toggled -= EnableSessionBackupAndRestoreToggleSwitch_Toggled;
            AlwaysOpenNewWindowToggleSwitch.Toggled -= AlwaysOpenNewWindowToggleSwitch_Toggled;
        }

        private void EnableSessionBackupAndRestoreToggleSwitch_Toggled(object sender, RoutedEventArgs e)
        {
            AppSettingsService.IsSessionSnapshotEnabled = EnableSessionSnapshotToggleSwitch.IsOn;
        }

        private void ShowStatusBarToggleSwitch_Toggled(object sender, RoutedEventArgs e)
        {
            AppSettingsService.ShowStatusBar = ShowStatusBarToggleSwitch.IsOn;
            InteropService.SyncSettings(SettingsKey.EditorShowStatusBarBool, AppSettingsService.ShowStatusBar);
        }

        private void AlwaysOpenNewWindowToggleSwitch_Toggled(object sender, RoutedEventArgs e)
        {
            AppSettingsService.AlwaysOpenNewWindow = AlwaysOpenNewWindowToggleSwitch.IsOn;
            InteropService.SyncSettings(SettingsKey.AlwaysOpenNewWindowBool, AppSettingsService.AlwaysOpenNewWindow);
        }
    }
}