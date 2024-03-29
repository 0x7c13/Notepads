﻿// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Views.Settings
{
    using System;
    using Notepads.Extensions;
    using Notepads.Services;
    using Windows.ApplicationModel;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media.Imaging;

    public sealed partial class AboutPage : Page
    {
        public string AppName => App.ApplicationName;

        public string AppVersion => $"v{GetAppVersion()}";

        public AboutPage()
        {
            InitializeComponent();
            SetAppIconBasedOnTheme(ThemeSettingsService.ThemeMode);

            Loaded += AboutPage_Loaded;
            Unloaded += AboutPage_Unloaded;
        }

        private void AboutPage_Loaded(object sender, RoutedEventArgs e)
        {
            ThemeSettingsService.OnThemeChanged += ThemeSettingsService_OnThemeChanged;
        }

        private void AboutPage_Unloaded(object sender, RoutedEventArgs e)
        {
            ThemeSettingsService.OnThemeChanged -= ThemeSettingsService_OnThemeChanged;
        }

        private async void ThemeSettingsService_OnThemeChanged(object sender, ElementTheme theme)
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                SetAppIconBasedOnTheme(theme);
            });
        }

        private void SetAppIconBasedOnTheme(ElementTheme theme)
        {
            if (theme == ElementTheme.Dark || theme == ElementTheme.Default)
            {
                AppIconImage.Source = new BitmapImage(new Uri("ms-appx:///Assets/appicon_w.png"));
            }
            else
            {
                AppIconImage.Source = new BitmapImage(new Uri("ms-appx:///Assets/appicon_b.png"));
            }
        }

        private static string GetAppVersion()
        {
            PackageVersion version = Package.Current.Id.Version;
            return $"{version.Major}.{version.Minor}.{version.Build}.{version.Revision}";
        }
    }
}