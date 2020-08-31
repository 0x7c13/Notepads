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
        private readonly IReadOnlyCollection<LanguageItem> SupportedLanguages = LanguageUtility.GetSupportedLanguageItems();

        public AdvancedSettingsPage()
        {
            InitializeComponent();

            ShowStatusBarToggleSwitch.IsOn = AppSettingsService.ShowStatusBar;
            EnableSmartCopyToggleSwitch.IsOn = AppSettingsService.IsSmartCopyEnabled;

            // Disable session snapshot toggle for shadow windows
            if (!App.IsPrimaryInstance)
            {
                EnableSessionSnapshotToggleSwitch.IsOn = false;
                EnableSessionSnapshotToggleSwitch.IsEnabled = false;
            }
            else
            {
                EnableSessionSnapshotToggleSwitch.IsOn = AppSettingsService.IsSessionSnapshotEnabled;
            }

            ExitingLastTabClosesWindowToggleSwitch.IsOn = AppSettingsService.ExitingLastTabClosesWindow;
            AlwaysOpenNewWindowToggleSwitch.IsOn = AppSettingsService.AlwaysOpenNewWindow;

            if (App.IsGameBarWidget)
            {
                // these settings don't make sense for Game Bar, there can be only one
                SessionSnapshotSettingsTitle.Visibility = Visibility.Collapsed;
                SessionSnapshotSettingsControls.Visibility = Visibility.Collapsed;
                LaunchPreferenceSettingsTitle.Visibility = Visibility.Collapsed;
                LaunchPreferenceSettingsControls.Visibility = Visibility.Collapsed;
            }

            LanguagePicker.SelectedItem = SupportedLanguages.FirstOrDefault(language => language.ID == ApplicationLanguages.PrimaryLanguageOverride);
            RestartPrompt.Visibility = LanguageUtility.CurrentLanguageID == ApplicationLanguages.PrimaryLanguageOverride ? Visibility.Collapsed : Visibility.Visible;

            Loaded += AdvancedSettings_Loaded;
            Unloaded += AdvancedSettings_Unloaded;
        }

        private void AdvancedSettings_Loaded(object sender, RoutedEventArgs e)
        {
            ShowStatusBarToggleSwitch.Toggled += ShowStatusBarToggleSwitch_Toggled;
            EnableSmartCopyToggleSwitch.Toggled += EnableSmartCopyToggleSwitch_Toggled;
            EnableSessionSnapshotToggleSwitch.Toggled += EnableSessionBackupAndRestoreToggleSwitch_Toggled;
            ExitingLastTabClosesWindowToggleSwitch.Toggled += ExitingLastTabClosesWindowToggleSwitch_Toggled;
            AlwaysOpenNewWindowToggleSwitch.Toggled += AlwaysOpenNewWindowToggleSwitch_Toggled;
            LanguagePicker.SelectionChanged += LanguagePicker_SelectionChanged;
        }

        private void AdvancedSettings_Unloaded(object sender, RoutedEventArgs e)
        {
            ShowStatusBarToggleSwitch.Toggled -= ShowStatusBarToggleSwitch_Toggled;
            EnableSmartCopyToggleSwitch.Toggled -= EnableSmartCopyToggleSwitch_Toggled;
            EnableSessionSnapshotToggleSwitch.Toggled -= EnableSessionBackupAndRestoreToggleSwitch_Toggled;
            ExitingLastTabClosesWindowToggleSwitch.Toggled -= ExitingLastTabClosesWindowToggleSwitch_Toggled;
            AlwaysOpenNewWindowToggleSwitch.Toggled -= AlwaysOpenNewWindowToggleSwitch_Toggled;
            LanguagePicker.SelectionChanged -= LanguagePicker_SelectionChanged;
        }

        private void EnableSmartCopyToggleSwitch_Toggled(object sender, RoutedEventArgs e)
        {
            AppSettingsService.IsSmartCopyEnabled = EnableSmartCopyToggleSwitch.IsOn;
        }

        private void EnableSessionBackupAndRestoreToggleSwitch_Toggled(object sender, RoutedEventArgs e)
        {
            AppSettingsService.IsSessionSnapshotEnabled = EnableSessionSnapshotToggleSwitch.IsOn;
        }

        private void ShowStatusBarToggleSwitch_Toggled(object sender, RoutedEventArgs e)
        {
            AppSettingsService.ShowStatusBar = ShowStatusBarToggleSwitch.IsOn;
        }

        private void ExitingLastTabClosesWindowToggleSwitch_Toggled(object sender, RoutedEventArgs e)
        {
            AppSettingsService.ExitingLastTabClosesWindow = ExitingLastTabClosesWindowToggleSwitch.IsOn;
        }

        private void AlwaysOpenNewWindowToggleSwitch_Toggled(object sender, RoutedEventArgs e)
        {
            AppSettingsService.AlwaysOpenNewWindow = AlwaysOpenNewWindowToggleSwitch.IsOn;
        }

        private void LanguagePicker_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            var languageId = ((LanguageItem)e.AddedItems.First()).ID;

            RestartPrompt.Visibility = languageId == LanguageUtility.CurrentLanguageID ? Visibility.Collapsed : Visibility.Visible;

            ApplicationLanguages.PrimaryLanguageOverride = languageId;
        }
    }
}