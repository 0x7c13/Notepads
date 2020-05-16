namespace Notepads.Views.Settings
{
    using Notepads.Services;
    using Notepads.Utilities;
    using System.Collections.Generic;
    using System.Linq;
    using Windows.Globalization;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;

    public sealed partial class AdvancedSettingsPage : Page
    {
        private IReadOnlyCollection<LanguageItem> SupportedLanguages;

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

#if DEBUG
            SupportedLanguages = LanguageUtility.GetSupportedLanguageItems();
            if (LanguagePreferenceSettingsPanel == null)
            {
                FindName("LanguagePreferenceSettingsPanel"); // Lazy loading
            }
            LanguagePicker.SelectedItem = SupportedLanguages.FirstOrDefault(language => language.ID == ApplicationLanguages.PrimaryLanguageOverride);
#endif
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
#if DEBUG
            LanguagePicker.SelectionChanged += LanguagePicker_SelectionChanged;
#endif
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
        }

        private void AlwaysOpenNewWindowToggleSwitch_Toggled(object sender, RoutedEventArgs e)
        {
            AppSettingsService.AlwaysOpenNewWindow = AlwaysOpenNewWindowToggleSwitch.IsOn;
        }

        private void LanguagePicker_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            var selectedID = ((LanguageItem)LanguagePicker.SelectedItem).ID;

            if (selectedID == LanguageUtility.CurrentLanguageID)
            {
                RestartPrompt.Visibility = Visibility.Collapsed;
            }
            else
            {
                RestartPrompt.Visibility = Visibility.Visible;
            }

            ApplicationLanguages.PrimaryLanguageOverride = selectedID;
        }
    }
}