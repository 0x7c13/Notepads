// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Presentation.Helpers;
using Notepads.Presentation.Theming;
using Windows.System.Power;
using Windows.UI;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Media;
using Windows.UI.Xaml.Navigation;

namespace Notepads.Presentation.Views.Settings;

public sealed partial class PersonalizationSettingsPage : Page
{
    private readonly UISettings UISettings = new();

    private SettingsContext _context;
    private bool _eventsBound;

    public PersonalizationSettingsPage()
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

        if (ThemeSettingsService.UseWindowsTheme)
        {
            ThemeModeDefaultButton.IsChecked = true;
        }
        else
        {
            switch (ThemeSettingsService.ThemeMode)
            {
                case ElementTheme.Light:
                    ThemeModeLightButton.IsChecked = true;
                    break;
                case ElementTheme.Dark:
                    ThemeModeDarkButton.IsChecked = true;
                    break;
            }
        }

        AccentColorToggle.IsOn = ThemeSettingsService.UseWindowsAccentColor;
        AccentColorPicker.IsEnabled = !ThemeSettingsService.UseWindowsAccentColor;
        BackgroundTintOpacitySlider.Value = ThemeSettingsService.AppBackgroundPanelTintOpacity * 100;
        AccentColorPicker.Color = ThemeSettingsService.AppAccentColor;

        BackgroundTintOpacitySlider.IsEnabled = UISettings.AdvancedEffectsEnabled &&
                                                PowerManager.EnergySaverStatus != EnergySaverStatus.On;

        Loaded += PersonalizationSettings_Loaded;
        Unloaded += PersonalizationSettings_Unloaded;
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        Initialize(e.Parameter as SettingsContext ?? throw new ArgumentException("Settings navigation requires a context."));
    }

    private async void ThemeSettingsService_OnAccentColorChanged(object sender, Color color)
    {
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (!_eventsBound) return;
            BackgroundTintOpacitySlider.Foreground = new SolidColorBrush(color);
            AccentColorPicker.ColorChanged -= AccentColorPicker_OnColorChanged;
            AccentColorPicker.Color = color;
            AccentColorPicker.ColorChanged += AccentColorPicker_OnColorChanged;
        });
    }

    private void PersonalizationSettings_Loaded(object sender, RoutedEventArgs e)
    {
        if (_eventsBound) return;
        _eventsBound = true;
        ThemeModeDefaultButton.Checked += ThemeRadioButton_OnChecked;
        ThemeModeLightButton.Checked += ThemeRadioButton_OnChecked;
        ThemeModeDarkButton.Checked += ThemeRadioButton_OnChecked;
        BackgroundTintOpacitySlider.ValueChanged += BackgroundTintOpacitySlider_OnValueChanged;
        AccentColorToggle.Toggled += WindowsAccentColorToggle_OnToggled;
        AccentColorPicker.ColorChanged += AccentColorPicker_OnColorChanged;
        ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;
        UISettings.AdvancedEffectsEnabledChanged += UISettings_AdvancedEffectsEnabledChanged;
        PowerManager.EnergySaverStatusChanged += PowerManager_EnergySaverStatusChanged;
    }

    private void PersonalizationSettings_Unloaded(object sender, RoutedEventArgs e)
    {
        if (!_eventsBound) return;
        _eventsBound = false;
        ThemeModeDefaultButton.Checked -= ThemeRadioButton_OnChecked;
        ThemeModeLightButton.Checked -= ThemeRadioButton_OnChecked;
        ThemeModeDarkButton.Checked -= ThemeRadioButton_OnChecked;
        BackgroundTintOpacitySlider.ValueChanged -= BackgroundTintOpacitySlider_OnValueChanged;
        AccentColorToggle.Toggled -= WindowsAccentColorToggle_OnToggled;
        AccentColorPicker.ColorChanged -= AccentColorPicker_OnColorChanged;
        ThemeSettingsService.OnAccentColorChanged -= ThemeSettingsService_OnAccentColorChanged;
        UISettings.AdvancedEffectsEnabledChanged -= UISettings_AdvancedEffectsEnabledChanged;
        PowerManager.EnergySaverStatusChanged -= PowerManager_EnergySaverStatusChanged;
    }

    private async void PowerManager_EnergySaverStatusChanged(object sender, object e)
    {
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            BackgroundTintOpacitySlider.IsEnabled = UISettings.AdvancedEffectsEnabled &&
                                                    PowerManager.EnergySaverStatus != EnergySaverStatus.On;
        });
    }

    private async void UISettings_AdvancedEffectsEnabledChanged(UISettings sender, object args)
    {
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            BackgroundTintOpacitySlider.IsEnabled = UISettings.AdvancedEffectsEnabled &&
                                                    PowerManager.EnergySaverStatus != EnergySaverStatus.On;
        });
    }

    private void ThemeRadioButton_OnChecked(object sender, RoutedEventArgs e)
    {
        if (sender is RadioButton radioButton)
        {
            switch (radioButton.Tag)
            {
                case "Light":
                    ThemeSettingsService.UseWindowsTheme = false;
                    ThemeSettingsService.SetTheme(ElementTheme.Light);
                    break;
                case "Dark":
                    ThemeSettingsService.UseWindowsTheme = false;
                    ThemeSettingsService.SetTheme(ElementTheme.Dark);
                    break;
                case "Default":
                    ThemeSettingsService.UseWindowsTheme = true;
                    break;
            }
        }
    }

    private void AccentColorPicker_OnColorChanged(ColorPicker sender, ColorChangedEventArgs args)
    {
        if (AccentColorPicker.IsEnabled)
        {
            ThemeSettingsService.AppAccentColor = args.NewColor;
            if (!AccentColorToggle.IsOn) ThemeSettingsService.CustomAccentColor = args.NewColor;
        }
    }

    private void BackgroundTintOpacitySlider_OnValueChanged(object sender, RangeBaseValueChangedEventArgs e)
    {
        ThemeSettingsService.AppBackgroundPanelTintOpacity = e.NewValue / 100;
    }

    private void WindowsAccentColorToggle_OnToggled(object sender, RoutedEventArgs e)
    {
        AccentColorPicker.IsEnabled = !AccentColorToggle.IsOn;
        ThemeSettingsService.UseWindowsAccentColor = AccentColorToggle.IsOn;
        AccentColorPicker.Color = AccentColorToggle.IsOn ? ThemeSettingsService.AppAccentColor : ThemeSettingsService.CustomAccentColor;
    }
}
