﻿namespace Notepads.Views
{
    using Notepads.Extensions;
    using Notepads.Services;
    using System.Linq;
    using Windows.UI;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Navigation;

    public sealed partial class SettingsPage : Page
    {
        public SettingsPage()
        {
            InitializeComponent();
            Loaded += SettingsPage_Loaded;
            Unloaded += SettingsPage_Unloaded;

            if (App.IsGameBarWidget)
            {
                ThemeSettingsService.SetRequestedTheme(null, Window.Current.Content, null);
            }
        }

        private void SettingsPage_Loaded(object sender, RoutedEventArgs e)
        {
            if (App.IsGameBarWidget)
            {
                ThemeSettingsService.OnThemeChanged += ThemeSettingsService_OnThemeChanged;
                ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;
            }
            ((NavigationViewItem)SettingsNavigationView.MenuItems.First()).IsSelected = true;
        }

        private void SettingsPage_Unloaded(object sender, RoutedEventArgs e)
        {
            if (App.IsGameBarWidget)
            {
                ThemeSettingsService.OnThemeChanged -= ThemeSettingsService_OnThemeChanged;
                ThemeSettingsService.OnAccentColorChanged -= ThemeSettingsService_OnAccentColorChanged;
            }
        }

        private async void ThemeSettingsService_OnAccentColorChanged(object sender, Color color)
        {
            await Dispatcher.CallOnUIThreadAsync(ThemeSettingsService.SetRequestedAccentColor);
        }

        private async void ThemeSettingsService_OnThemeChanged(object sender, ElementTheme theme)
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                ThemeSettingsService.SetRequestedTheme(null, Window.Current.Content, null);
            });
        }

        protected override void OnNavigatedTo(NavigationEventArgs e)
        {
            base.OnNavigatedTo(e);

            switch (e.Parameter)
            {
                case null:
                    return;
            }
        }

        private void SettingsPanel_OnItemInvoked(NavigationView sender, NavigationViewItemInvokedEventArgs args)
        {
            SettingsPanel.Show((args.InvokedItem as string), (args.InvokedItemContainer as NavigationViewItem)?.Tag as string);
        }
    }
}