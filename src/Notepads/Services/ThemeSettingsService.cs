namespace Notepads.Services
{
    using System;
    using Microsoft.Toolkit.Uwp.Helpers;
    using Notepads.Brushes;
    using Notepads.Controls.Helpers;
    using Notepads.Settings;
    using Notepads.Utilities;
    using Windows.UI;
    using Windows.UI.ViewManagement;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media;

    public static class ThemeSettingsService
    {
        public static event EventHandler<ElementTheme> OnThemeChanged;
        public static event EventHandler<Brush> OnBackgroundChanged;
        public static event EventHandler<Color> OnAccentColorChanged;

        public static ElementTheme ThemeMode { get; set; }

        private static readonly UISettings UISettings = new UISettings();
        private static readonly ThemeListener ThemeListener = new ThemeListener();
        private static Brush _currentAppBackgroundBrush;

        private static bool _useWindowsTheme;

        public static bool UseWindowsTheme
        {
            get => _useWindowsTheme;
            set
            {
                if (value != _useWindowsTheme)
                {
                    _useWindowsTheme = value;
                    if (value)
                    {
                        var currentWindowsTheme = Application.Current.RequestedTheme.ToElementTheme();
                        if (ThemeMode != currentWindowsTheme)
                        {
                            ThemeMode = currentWindowsTheme;
                            OnThemeChanged?.Invoke(null, ThemeMode);
                        }
                    }
                    ApplicationSettingsStore.Write(SettingsKey.UseWindowsThemeBool, _useWindowsTheme);
                }
            }
        }

        private static bool _useWindowsAccentColor;

        public static bool UseWindowsAccentColor
        {
            get => _useWindowsAccentColor;
            set
            {
                _useWindowsAccentColor = value;
                if (value)
                {
                    AppAccentColor = UISettings.GetColorValue(UIColorType.Accent);
                }
                ApplicationSettingsStore.Write(SettingsKey.UseWindowsAccentColorBool, _useWindowsAccentColor);
            }
        }

        private static double _appBackgroundPanelTintOpacity;

        public static double AppBackgroundPanelTintOpacity
        {
            get => _appBackgroundPanelTintOpacity;
            set
            {
                _appBackgroundPanelTintOpacity = value;
                OnBackgroundChanged?.Invoke(null, GetAppBackgroundBrush(ThemeMode));
                ApplicationSettingsStore.Write(SettingsKey.AppBackgroundTintOpacityDouble, value);
            }
        }

        private static Color _appAccentColor;

        public static Color AppAccentColor
        {
            get => _appAccentColor;
            set
            {
                _appAccentColor = value;
                OnAccentColorChanged?.Invoke(null, _appAccentColor);
                ApplicationSettingsStore.Write(SettingsKey.AppAccentColorHexStr, value.ToHex());
            }
        }

        private static Color _customAccentColor;

        public static Color CustomAccentColor
        {
            get => _customAccentColor;
            set
            {
                _customAccentColor = value;
                ApplicationSettingsStore.Write(SettingsKey.CustomAccentColorHexStr, value.ToHex());
            }
        }

        public static void Initialize()
        {
            InitializeThemeMode();

            InitializeAppAccentColor();

            InitializeCustomAccentColor();

            InitializeAppBackgroundPanelTintOpacity();
        }

        private static void InitializeAppAccentColor()
        {
            if (ApplicationSettingsStore.Read(SettingsKey.UseWindowsAccentColorBool) is bool useWindowsAccentColor)
            {
                _useWindowsAccentColor = useWindowsAccentColor;
            }
            else
            {
                _useWindowsAccentColor = true;
            }

            UISettings.ColorValuesChanged += UiSettings_ColorValuesChanged;

            _appAccentColor = UISettings.GetColorValue(Windows.UI.ViewManagement.UIColorType.Accent);

            if (!UseWindowsAccentColor)
            {
                if (ApplicationSettingsStore.Read(SettingsKey.AppAccentColorHexStr) is string accentColorHexStr)
                {
                    _appAccentColor = accentColorHexStr.ToColor();
                }
            }
        }

        private static void InitializeCustomAccentColor()
        {
            if (ApplicationSettingsStore.Read(SettingsKey.CustomAccentColorHexStr) is string customAccentColorHexStr)
            {
                _customAccentColor = customAccentColorHexStr.ToColor();
            }
            else
            {
                _customAccentColor = _appAccentColor;
            }
        }

        private static void UiSettings_ColorValuesChanged(UISettings sender, object args)
        {
            if (UseWindowsAccentColor)
            {
                AppAccentColor = sender.GetColorValue(UIColorType.Accent);
            }
        }

        private static void InitializeAppBackgroundPanelTintOpacity()
        {
            if (ApplicationSettingsStore.Read(SettingsKey.AppBackgroundTintOpacityDouble) is double tintOpacity)
            {
                _appBackgroundPanelTintOpacity = tintOpacity;
            }
            else
            {
                _appBackgroundPanelTintOpacity = 0.75;
            }
        }

        private static void InitializeThemeMode()
        {
            if (ApplicationSettingsStore.Read(SettingsKey.UseWindowsThemeBool) is bool useWindowsTheme)
            {
                _useWindowsTheme = useWindowsTheme;
            }
            else
            {
                _useWindowsTheme = true;
            }

            ThemeListener.ThemeChanged += ThemeListener_ThemeChanged;

            ThemeMode = Application.Current.RequestedTheme.ToElementTheme();

            if (!UseWindowsTheme)
            {
                if (ApplicationSettingsStore.Read(SettingsKey.RequestedThemeStr) is string themeModeStr)
                {
                    if (Enum.TryParse(typeof(ElementTheme), themeModeStr, out var theme))
                    {
                        ThemeMode = (ElementTheme)theme;
                    }
                }
            }
        }

        private static void ThemeListener_ThemeChanged(ThemeListener sender)
        {
            if (UseWindowsTheme)
            {
                SetTheme(sender.CurrentTheme.ToElementTheme());
            }
        }

        public static void SetTheme(ElementTheme theme)
        {
            if (ThemeMode != theme)
            {
                ThemeMode = theme;
                ApplicationSettingsStore.Write(SettingsKey.RequestedThemeStr, ThemeMode.ToString());
                OnThemeChanged?.Invoke(null, theme);
            }
        }

        public static void SetRequestedTheme(Panel backgroundPanel, UIElement currentContent, ApplicationViewTitleBar titleBar)
        {
            // Set requested theme for app background
            if (backgroundPanel != null)
            {
                backgroundPanel.Background = GetAppBackgroundBrush(ThemeMode);
            }

            if (currentContent is FrameworkElement frameworkElement)
            {
                frameworkElement.RequestedTheme = ThemeMode;
            }

            // Set requested theme for app title bar
            if (titleBar != null)
            {
                ApplyThemeForTitleBarButtons(titleBar, ThemeMode);
            }

            // Set ContentDialog background dimming color
            ((SolidColorBrush)Application.Current.Resources["SystemControlPageBackgroundMediumAltMediumBrush"]).Color =
                ThemeMode == ElementTheme.Dark ? Color.FromArgb(153, 0, 0, 0) : Color.FromArgb(153, 255, 255, 255);

            if (DialogManager.ActiveDialog != null)
            {
                DialogManager.ActiveDialog.RequestedTheme = ThemeMode;
            }

            // Set accent color
            UpdateSystemAccentColorAndBrushes(AppAccentColor);
        }

        public static void SetRequestedAccentColor()
        {
            UpdateSystemAccentColorAndBrushes(AppAccentColor);
        }

        public static ElementTheme ToElementTheme(this ApplicationTheme theme)
        {
            switch (theme)
            {
                case ApplicationTheme.Light:
                    return ElementTheme.Light;
                case ApplicationTheme.Dark:
                    return ElementTheme.Dark;
                default:
                    return ElementTheme.Default;
            }
        }

        private static Brush GetAppBackgroundBrush(ElementTheme theme)
        {
            var darkModeBaseColor = Color.FromArgb(255, 50, 50, 50);
            var lightModeBaseColor = Color.FromArgb(255, 240, 240, 240);

            var baseColor = theme == ElementTheme.Light ? lightModeBaseColor : darkModeBaseColor;

            if (AppBackgroundPanelTintOpacity > 0.99f ||
                !Windows.Foundation.Metadata.ApiInformation.IsTypePresent("Windows.UI.Xaml.Media.AcrylicBrush") ||
                App.IsGameBarWidget)
            {
                return new SolidColorBrush(baseColor);
            }
            else
            {
                if (_currentAppBackgroundBrush is HostBackdropAcrylicBrush hostBackdropAcrylicBrush)
                {
                    hostBackdropAcrylicBrush.LuminosityColor = baseColor;
                    hostBackdropAcrylicBrush.TintOpacity = (float)AppBackgroundPanelTintOpacity;
                    return _currentAppBackgroundBrush;
                }
                return _currentAppBackgroundBrush = BrushUtility.GetHostBackdropAcrylicBrush(baseColor, (float)AppBackgroundPanelTintOpacity).Result;
            }
        }

        public static void ApplyThemeForTitleBarButtons(ApplicationViewTitleBar titleBar, ElementTheme theme)
        {
            if (theme == ElementTheme.Dark)
            {
                // Set active window colors
                titleBar.ButtonForegroundColor = Windows.UI.Colors.White;
                titleBar.ButtonBackgroundColor = Windows.UI.Colors.Transparent;
                titleBar.ButtonHoverForegroundColor = Windows.UI.Colors.White;
                titleBar.ButtonHoverBackgroundColor = Color.FromArgb(255, 90, 90, 90);
                titleBar.ButtonPressedForegroundColor = Windows.UI.Colors.White;
                titleBar.ButtonPressedBackgroundColor = Color.FromArgb(255, 120, 120, 120);

                // Set inactive window colors
                titleBar.InactiveForegroundColor = Windows.UI.Colors.Gray;
                titleBar.InactiveBackgroundColor = Windows.UI.Colors.Transparent;
                titleBar.ButtonInactiveForegroundColor = Windows.UI.Colors.Gray;
                titleBar.ButtonInactiveBackgroundColor = Windows.UI.Colors.Transparent;

                titleBar.BackgroundColor = Color.FromArgb(255, 45, 45, 45);
            }
            else if (theme == ElementTheme.Light)
            {
                // Set active window colors
                titleBar.ButtonForegroundColor = Windows.UI.Colors.Black;
                titleBar.ButtonBackgroundColor = Windows.UI.Colors.Transparent;
                titleBar.ButtonHoverForegroundColor = Windows.UI.Colors.Black;
                titleBar.ButtonHoverBackgroundColor = Color.FromArgb(255, 180, 180, 180);
                titleBar.ButtonPressedForegroundColor = Windows.UI.Colors.Black;
                titleBar.ButtonPressedBackgroundColor = Color.FromArgb(255, 150, 150, 150);

                // Set inactive window colors
                titleBar.InactiveForegroundColor = Windows.UI.Colors.DimGray;
                titleBar.InactiveBackgroundColor = Windows.UI.Colors.Transparent;
                titleBar.ButtonInactiveForegroundColor = Windows.UI.Colors.DimGray;
                titleBar.ButtonInactiveBackgroundColor = Windows.UI.Colors.Transparent;

                titleBar.BackgroundColor = Color.FromArgb(255, 210, 210, 210);
            }
        }

        private static void UpdateSystemAccentColorAndBrushes(Color color)
        {
            if ((Color)Application.Current.Resources["SystemAccentColor"] == color)
            {
                return;
            }

            Application.Current.Resources["SystemAccentColor"] = color;

            // Took from: https://stackoverflow.com/questions/31831917/change-accent-color-in-windows-10-uwp/31844773
            var brushes = new string[]
            {
                "SystemControlBackgroundAccentBrush",
                "SystemControlDisabledAccentBrush",
                "SystemControlForegroundAccentBrush",
                "SystemControlHighlightAccentBrush",
                "SystemControlHighlightAltAccentBrush",
                "SystemControlHighlightAltListAccentHighBrush",
                "SystemControlHighlightAltListAccentLowBrush",
                "SystemControlHighlightAltListAccentMediumBrush",
                "SystemControlHighlightListAccentHighBrush",
                "SystemControlHighlightListAccentLowBrush",
                "SystemControlHighlightListAccentMediumBrush",
                "SystemControlHyperlinkTextBrush",
                "ContentDialogBorderThemeBrush",
                "JumpListDefaultEnabledBackground"
            };

            foreach (var brush in brushes)
            {
                try
                {
                    ((SolidColorBrush)Application.Current.Resources[brush]).Color = color;
                }
                catch (Exception ex)
                {
                    LoggingService.LogError($"[{nameof(ThemeSettingsService)}] Failed to apply color change for Brush: [{brush}]: {ex.Message}");
                }
            }

            try
            {
                // Overwrite MenuFlyoutSubItemRevealBackgroundSubMenuOpened resource color
                ((RevealBackgroundBrush)Application.Current.Resources["SystemControlHighlightAccent3RevealBackgroundBrush"]).Color = color;
            }
            catch (Exception)
            {
                // ignore
            }
        }
    }
}