// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Presentation.Helpers;
using Notepads.Presentation.Theming;
using Windows.ApplicationModel;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media.Imaging;
using Windows.UI.Xaml.Navigation;

namespace Notepads.Presentation.Views.Settings;

public sealed partial class AboutPage : Page
{
    public string AppName => _context?.ApplicationName ?? string.Empty;

    public string AppVersion => $"v{GetAppVersion()}";

    private SettingsContext _context;
    private bool _eventsBound;

    public AboutPage()
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

        Bindings.Update();
        SetAppIconBasedOnTheme(ThemeSettingsService.ThemeMode);

        Loaded += AboutPage_Loaded;
        Unloaded += AboutPage_Unloaded;
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        Initialize(e.Parameter as SettingsContext ?? throw new ArgumentException("Settings navigation requires a context."));
    }

    private void AboutPage_Loaded(object sender, RoutedEventArgs e)
    {
        if (_eventsBound) return;
        _eventsBound = true;
        ThemeSettingsService.OnThemeChanged += ThemeSettingsService_OnThemeChanged;
    }

    private void AboutPage_Unloaded(object sender, RoutedEventArgs e)
    {
        if (!_eventsBound) return;
        _eventsBound = false;
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
