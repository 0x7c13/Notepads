
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
            Loaded += AdvancedSettings_Loaded;
        }

        private void AdvancedSettings_Loaded(object sender, RoutedEventArgs e)
        {
            ShowStatusBarToggleSwitch.Toggled += ShowStatusBarToggleSwitch_Toggled;
        }

        private void ShowStatusBarToggleSwitch_Toggled(object sender, RoutedEventArgs e)
        {
            EditorSettingsService.ShowStatusBar = ShowStatusBarToggleSwitch.IsOn;
        }
    }
}
