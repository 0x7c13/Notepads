
namespace Notepads.Services
{
    using Microsoft.Toolkit.Uwp.Helpers;
    using Microsoft.Toolkit.Uwp.UI.Helpers;
    using Notepads.Settings;
    using System;
    using System.Collections.Generic;
    using System.Globalization;
    using System.Linq;
    using System.Runtime.CompilerServices;
    using System.Text;
    using System.Threading.Tasks;
    using Windows.ApplicationModel.Core;
    using Windows.Storage;
    using Windows.UI;
    using Windows.UI.Core;
    using Windows.UI.ViewManagement;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media;

    public static class ThemeSettingsService
    {
        private static ThemeListener _themeListener;

        private static UISettings _uiSettings;

        public static event EventHandler<ElementTheme> OnThemeChanged;

        public static event EventHandler<Color> OnAccentColorChanged;

        public static ElementTheme ThemeMode { get; set; }

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
                        ThemeMode = ApplicationThemeToElementTheme(Application.Current.RequestedTheme);
                        SetRequestedTheme();
                    }
                    ApplicationSettings.WriteAsync(SettingsKey.UseWindowsThemeBool, _useWindowsTheme, true);
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
                    AppAccentColor = new UISettings().GetColorValue(UIColorType.Accent);
                }
                ApplicationSettings.WriteAsync(SettingsKey.UseWindowsAccentColorBool, _useWindowsAccentColor, true);
            }
        }

        public static Panel AppBackground { get; set; }

        private static double _appBackgroundPanelTintOpacity;

        public static double AppBackgroundPanelTintOpacity
        {
            get => _appBackgroundPanelTintOpacity;
            set
            {
                _appBackgroundPanelTintOpacity = value;
                if (AppBackground != null)
                {
                    ((AcrylicBrush)AppBackground.Background).TintOpacity = value;
                    ApplicationSettings.WriteAsync(SettingsKey.AppBackgroundTintOpacityDouble, value, true);
                }
            }
        }

        private static Color _appAccentColor;

        public static Color AppAccentColor
        {
            get => _appAccentColor;
            set
            {
                _appAccentColor = value;
                UpdateSystemAccentColorAndBrushes(value);
                ApplicationSettings.WriteAsync(SettingsKey.AppAccentColorHexStr, value.ToHex(), true);
                OnAccentColorChanged?.Invoke(null, value);
            }
        }

        public static void Initialize()
        {
            InitializeThemeMode();

            InitializeAppAccentColor();

            InitializeAppBackgroundPanelTintOpacity();
        }

        private static void InitializeAppAccentColor()
        {
            if (ApplicationSettings.ReadAsync(SettingsKey.UseWindowsAccentColorBool) is bool useWindowsAccentColor)
            {
                _useWindowsAccentColor = useWindowsAccentColor;
            }
            else
            {
                _useWindowsAccentColor = true;
            }

            _uiSettings = new UISettings();
            _uiSettings.ColorValuesChanged += UiSettings_ColorValuesChanged;

            _appAccentColor = new Windows.UI.ViewManagement.UISettings().GetColorValue(Windows.UI.ViewManagement.UIColorType.Accent);

            if (!UseWindowsAccentColor)
            {
                if (ApplicationSettings.ReadAsync(SettingsKey.AppAccentColorHexStr) is string accentColorHexStr)
                {
                    _appAccentColor = GetColor(accentColorHexStr);
                }
            }
        }

        private static void UiSettings_ColorValuesChanged(UISettings sender, object args)
        {
            _ = CoreApplication.MainView.CoreWindow.Dispatcher.RunAsync(CoreDispatcherPriority.Normal, () =>
              {
                  if (UseWindowsAccentColor)
                  {
                      AppAccentColor = sender.GetColorValue(UIColorType.Accent);
                  }
              });
        }

        private static void InitializeAppBackgroundPanelTintOpacity()
        {
            if (ApplicationSettings.ReadAsync(SettingsKey.AppBackgroundTintOpacityDouble) is double tintOpacity)
            {
                _appBackgroundPanelTintOpacity = tintOpacity;
            }
            else
            {
                _appBackgroundPanelTintOpacity = 0.7;
            }
        }

        private static void InitializeThemeMode()
        {
            if (ApplicationSettings.ReadAsync(SettingsKey.UseWindowsThemeBool) is bool useWindowsTheme)
            {
                _useWindowsTheme = useWindowsTheme;
            }
            else
            {
                _useWindowsTheme = true;
            }

            _themeListener = new ThemeListener();
            _themeListener.ThemeChanged += ThemeListener_ThemeChanged;

            ThemeMode = ApplicationThemeToElementTheme(Application.Current.RequestedTheme);

            if (!UseWindowsTheme)
            {
                if (ApplicationSettings.ReadAsync(SettingsKey.RequestedThemeStr) is string themeModeStr)
                {
                    Enum.TryParse(typeof(ElementTheme), themeModeStr, out var theme);
                    ThemeMode = (ElementTheme)theme;
                }
            }
        }

        private static void ThemeListener_ThemeChanged(ThemeListener sender)
        {
            _ = CoreApplication.MainView.CoreWindow.Dispatcher.RunAsync(CoreDispatcherPriority.Normal, () =>
              {
                  if (UseWindowsTheme)
                  {
                      SetTheme(sender.CurrentTheme);
                  }
              });
        }

        public static void SetTheme(ApplicationTheme theme)
        {
            SetTheme(ApplicationThemeToElementTheme(theme));
        }

        public static void SetTheme(ElementTheme theme)
        {
            ThemeMode = theme;
            SetRequestedTheme();
            ApplicationSettings.WriteAsync(SettingsKey.RequestedThemeStr, ThemeMode.ToString(), true);
        }

        public static void SetRequestedTheme()
        {
            // Set requested theme for app background
            if (AppBackground != null)
            {
                ApplyAcrylicBrush(ThemeMode, AppBackground);
            }

            if (Window.Current.Content is FrameworkElement frameworkElement)
            {
                frameworkElement.RequestedTheme = ThemeMode;
            }

            // Set requested theme for app title bar
            ApplyThemeForTitleBarButtons(ThemeMode);

            // Set ContentDialog background dimming color
            ((SolidColorBrush)Application.Current.Resources["SystemControlPageBackgroundMediumAltMediumBrush"]).Color = ThemeMode == ElementTheme.Dark ? Color.FromArgb(153, 0, 0, 0) : Color.FromArgb(153, 255, 255, 255);

            // Set accent color
            UpdateSystemAccentColorAndBrushes(AppAccentColor);

            OnThemeChanged?.Invoke(null, ThemeMode);
        }

        private static ElementTheme ApplicationThemeToElementTheme(ApplicationTheme theme)
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

        public static void ApplyAcrylicBrush(ElementTheme theme, Panel panel)
        {
            // Apply Acrylic background
            if (theme == ElementTheme.Dark)
            {
                if (Windows.Foundation.Metadata.ApiInformation.IsTypePresent("Windows.UI.Xaml.Media.XamlCompositionBrushBase"))
                {
                    var myBrush = new Windows.UI.Xaml.Media.AcrylicBrush
                    {
                        BackgroundSource = Windows.UI.Xaml.Media.AcrylicBackgroundSource.HostBackdrop,
                        TintColor = Color.FromArgb(255, 45, 45, 45),
                        FallbackColor = Color.FromArgb(255, 45, 45, 45),
                        TintOpacity = AppBackgroundPanelTintOpacity
                    };
                    panel.Background = myBrush;
                }
                else
                {
                    SolidColorBrush myBrush = new SolidColorBrush(Color.FromArgb(255, 32, 32, 32));
                    panel.Background = myBrush;
                }
            }
            else if (theme == ElementTheme.Light)
            {
                if (Windows.Foundation.Metadata.ApiInformation.IsTypePresent("Windows.UI.Xaml.Media.XamlCompositionBrushBase"))
                {
                    var myBrush = new Windows.UI.Xaml.Media.AcrylicBrush
                    {
                        BackgroundSource = Windows.UI.Xaml.Media.AcrylicBackgroundSource.HostBackdrop,
                        TintColor = Color.FromArgb(255, 210, 210, 210),
                        FallbackColor = Color.FromArgb(255, 210, 210, 210),
                        TintOpacity = AppBackgroundPanelTintOpacity
                    };
                    panel.Background = myBrush;
                }
                else
                {
                    SolidColorBrush myBrush = new SolidColorBrush(Color.FromArgb(255, 230, 230, 230));
                    panel.Background = myBrush;
                }
            }
        }

        public static void ApplyThemeForTitleBarButtons(ElementTheme theme)
        {
            var titleBar = ApplicationView.GetForCurrentView().TitleBar;

            if (theme == ElementTheme.Dark)
            {
                // Set active window colors
                titleBar.ButtonForegroundColor = Windows.UI.Colors.White;
                titleBar.ButtonBackgroundColor = Windows.UI.Colors.Transparent;
                titleBar.ButtonHoverForegroundColor = Windows.UI.Colors.White;
                titleBar.ButtonHoverBackgroundColor = Color.FromArgb(255, 55, 55, 55);
                titleBar.ButtonPressedForegroundColor = Windows.UI.Colors.White;
                titleBar.ButtonPressedBackgroundColor = Color.FromArgb(255, 70, 70, 70);

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
                titleBar.ButtonHoverBackgroundColor = Color.FromArgb(255, 200, 200, 200);
                titleBar.ButtonPressedForegroundColor = Windows.UI.Colors.Black;
                titleBar.ButtonPressedBackgroundColor = Color.FromArgb(255, 185, 185, 185);

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
                catch (Exception)
                {
                    // ignore
                }
            }
        }

        private static Color GetColor(string hex)
        {
            hex = hex.Replace("#", string.Empty);
            byte a = (byte)(Convert.ToUInt32(hex.Substring(0, 2), 16));
            byte r = (byte)(Convert.ToUInt32(hex.Substring(2, 2), 16));
            byte g = (byte)(Convert.ToUInt32(hex.Substring(4, 2), 16));
            byte b = (byte)(Convert.ToUInt32(hex.Substring(6, 2), 16));
            return Windows.UI.Color.FromArgb(a, r, g, b);
        }
    }
}
