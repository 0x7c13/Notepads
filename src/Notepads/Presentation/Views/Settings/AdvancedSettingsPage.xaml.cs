// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Helpers;
using Windows.Globalization;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Navigation;

namespace Notepads.Presentation.Views.Settings;

public sealed partial class AdvancedSettingsPage : Page
{
    private readonly IReadOnlyCollection<LanguageItem> SupportedLanguages = LanguageUtility.GetSupportedLanguageItems();

    private SettingsContext _context;
    private bool _eventsBound;
    private bool _refreshingSessionSnapshot;

    public AdvancedSettingsPage()
    {
        InitializeComponent();
    }

    internal void Initialize(SettingsContext context)
    {
        if (_context != null)
        {
            if (!ReferenceEquals(_context, context)) throw new InvalidOperationException("A settings page cannot change its window context.");
            return;
        }
        _context = context ?? throw new ArgumentNullException(nameof(context));

        ShowStatusBarToggleSwitch.IsOn = ApplicationPreferences.ShowStatusBar;
        EnableSmartCopyToggleSwitch.IsOn = ApplicationPreferences.IsSmartCopyEnabled;
        EnableSessionSnapshotToggleSwitch.IsOn = ApplicationPreferences.IsSessionSnapshotEnabled;
        ExitWhenLastTabClosedToggleSwitch.IsOn = ApplicationPreferences.ExitWhenLastTabClosed;
        AlwaysOpenNewWindowToggleSwitch.IsOn = ApplicationPreferences.AlwaysOpenNewWindow;

        LanguagePicker.SelectedItem = SupportedLanguages.FirstOrDefault(language => language.ID == ApplicationLanguages.PrimaryLanguageOverride);
        RestartPrompt.Visibility = LanguageUtility.CurrentLanguageID == ApplicationLanguages.PrimaryLanguageOverride ? Visibility.Collapsed : Visibility.Visible;

        Loaded += AdvancedSettings_Loaded;
        Unloaded += AdvancedSettings_Unloaded;
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        Initialize(e.Parameter as SettingsContext ?? throw new ArgumentException("Settings navigation requires a context."));
    }

    private void AdvancedSettings_Loaded(object sender, RoutedEventArgs e)
    {
        if (_eventsBound) return;
        _eventsBound = true;
        ApplicationPreferences.OnSessionBackupAndRestoreOptionChanged += OnSessionSnapshotPreferenceChanged;
        OnSessionSnapshotPreferenceChanged(null, ApplicationPreferences.IsSessionSnapshotEnabled);
        ShowStatusBarToggleSwitch.Toggled += ShowStatusBarToggleSwitch_Toggled;
        EnableSmartCopyToggleSwitch.Toggled += EnableSmartCopyToggleSwitch_Toggled;
        EnableSessionSnapshotToggleSwitch.Toggled += EnableSessionBackupAndRestoreToggleSwitch_Toggled;
        ExitWhenLastTabClosedToggleSwitch.Toggled += ExitWhenLastTabClosedToggleSwitch_Toggled;
        AlwaysOpenNewWindowToggleSwitch.Toggled += AlwaysOpenNewWindowToggleSwitch_Toggled;
        LanguagePicker.SelectionChanged += LanguagePicker_SelectionChanged;
    }

    private void AdvancedSettings_Unloaded(object sender, RoutedEventArgs e)
    {
        if (!_eventsBound) return;
        _eventsBound = false;
        ApplicationPreferences.OnSessionBackupAndRestoreOptionChanged -= OnSessionSnapshotPreferenceChanged;
        ShowStatusBarToggleSwitch.Toggled -= ShowStatusBarToggleSwitch_Toggled;
        EnableSmartCopyToggleSwitch.Toggled -= EnableSmartCopyToggleSwitch_Toggled;
        EnableSessionSnapshotToggleSwitch.Toggled -= EnableSessionBackupAndRestoreToggleSwitch_Toggled;
        ExitWhenLastTabClosedToggleSwitch.Toggled -= ExitWhenLastTabClosedToggleSwitch_Toggled;
        AlwaysOpenNewWindowToggleSwitch.Toggled -= AlwaysOpenNewWindowToggleSwitch_Toggled;
        LanguagePicker.SelectionChanged -= LanguagePicker_SelectionChanged;
    }

    private void EnableSmartCopyToggleSwitch_Toggled(object sender, RoutedEventArgs e)
    {
        ApplicationPreferences.IsSmartCopyEnabled = EnableSmartCopyToggleSwitch.IsOn;
    }

    private void EnableSessionBackupAndRestoreToggleSwitch_Toggled(object sender, RoutedEventArgs e)
    {
        if (!_refreshingSessionSnapshot && ApplicationPreferences.IsSessionSnapshotEnabled != EnableSessionSnapshotToggleSwitch.IsOn)
            ApplicationPreferences.IsSessionSnapshotEnabled = EnableSessionSnapshotToggleSwitch.IsOn;
    }

    private async void OnSessionSnapshotPreferenceChanged(object sender, bool enabled)
    {
        try
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                if (!_eventsBound) return;
                _refreshingSessionSnapshot = true;
                try { EnableSessionSnapshotToggleSwitch.IsOn = ApplicationPreferences.IsSessionSnapshotEnabled; }
                finally { _refreshingSessionSnapshot = false; }
            });
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(AdvancedSettingsPage)}] Failed to refresh session preference: {ex}");
        }
    }

    private void ShowStatusBarToggleSwitch_Toggled(object sender, RoutedEventArgs e)
    {
        ApplicationPreferences.ShowStatusBar = ShowStatusBarToggleSwitch.IsOn;
    }

    private void ExitWhenLastTabClosedToggleSwitch_Toggled(object sender, RoutedEventArgs e)
    {
        ApplicationPreferences.ExitWhenLastTabClosed = ExitWhenLastTabClosedToggleSwitch.IsOn;
    }

    private void AlwaysOpenNewWindowToggleSwitch_Toggled(object sender, RoutedEventArgs e)
    {
        ApplicationPreferences.AlwaysOpenNewWindow = AlwaysOpenNewWindowToggleSwitch.IsOn;
    }

    private void LanguagePicker_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        var languageId = ((LanguageItem)e.AddedItems.First()).ID;

        RestartPrompt.Visibility = languageId == LanguageUtility.CurrentLanguageID ? Visibility.Collapsed : Visibility.Visible;

        ApplicationLanguages.PrimaryLanguageOverride = languageId;
    }
}
