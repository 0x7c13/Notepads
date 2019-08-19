
namespace Notepads.Controls.Settings
{
    using Notepads.Services;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;

    public sealed partial class AdvancedSettings : Page
    {
        public AdvancedSettings()
        {
            InitializeComponent();
            ShowStatusBarToggleSwitch.IsOn = EditorSettingsService.ShowStatusBar;
            EnableSessionBackupAndRestoreToggleSwitch.IsOn = EditorSettingsService.IsSessionBackupAndRestoreEnabled;
            Loaded += AdvancedSettings_Loaded;
        }

        private void AdvancedSettings_Loaded(object sender, RoutedEventArgs e)
        {
            ShowStatusBarToggleSwitch.Toggled += ShowStatusBarToggleSwitch_Toggled;
            EnableSessionBackupAndRestoreToggleSwitch.Toggled += EnableSessionBackupAndRestoreToggleSwitch_Toggled;
        }

        private void EnableSessionBackupAndRestoreToggleSwitch_Toggled(object sender, RoutedEventArgs e)
        {
            EditorSettingsService.IsSessionBackupAndRestoreEnabled = EnableSessionBackupAndRestoreToggleSwitch.IsOn;
        }

        private void ShowStatusBarToggleSwitch_Toggled(object sender, RoutedEventArgs e)
        {
            EditorSettingsService.ShowStatusBar = ShowStatusBarToggleSwitch.IsOn;
        }
    }
}
