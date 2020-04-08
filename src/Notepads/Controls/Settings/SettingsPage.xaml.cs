namespace Notepads.Controls.Settings
{
    using Microsoft.Gaming.XboxGameBar;
    using Notepads.Services;
    using Notepads.Utilities;
    using System;
    using System.Linq;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Navigation;

    public sealed partial class SettingsPage : Page
    {
        private XboxGameBarWidget _widget; // maintain throughout the lifetime of the settings game bar widget

        public SettingsPage()
        {
            InitializeComponent();
            Loaded += SettingsPage_Loaded;
            Unloaded += SettingsPage_Unloaded;
        }

        private void SettingsPage_Loaded(object sender, RoutedEventArgs e)
        {
            if (App.IsGameBarWidget)
            {
                ThemeSettingsService.OnRequestThemeUpdate += ThemeSettingsService_OnRequestThemeUpdate;
                ThemeSettingsService.OnRequestAccentColorUpdate += ThemeSettingsService_OnRequestAccentColorUpdate;
                ThemeSettingsService.SetRequestedTheme(null, Window.Current.Content, null);
            }
            ((NavigationViewItem)SettingsNavigationView.MenuItems.First()).IsSelected = true;
        }

        private void SettingsPage_Unloaded(object sender, RoutedEventArgs e)
        {
            if (App.IsGameBarWidget)
            {
                ThemeSettingsService.OnRequestThemeUpdate -= ThemeSettingsService_OnRequestThemeUpdate;
                ThemeSettingsService.OnRequestAccentColorUpdate -= ThemeSettingsService_OnRequestAccentColorUpdate;
            }
        }

        private async void ThemeSettingsService_OnRequestAccentColorUpdate(object sender, EventArgs e)
        {
            await ThreadUtility.CallOnMainViewUIThreadAsync(ThemeSettingsService.SetRequestedAccentColor);
        }

        private async void ThemeSettingsService_OnRequestThemeUpdate(object sender, EventArgs e)
        {
            await ThreadUtility.CallOnMainViewUIThreadAsync(() =>
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
                case XboxGameBarWidget widget:
                    _widget = widget;
                    Window.Current.Closed += WidgetSettingsWindowClosed;
                    break;
            }
        }

        private void WidgetSettingsWindowClosed(object sender, Windows.UI.Core.CoreWindowEventArgs e)
        {
            // Un-registering events
            Window.Current.Closed -= WidgetSettingsWindowClosed;
            // Cleanup game bar objects
            _widget = null;
        }

        private void SettingsPanel_OnItemInvoked(NavigationView sender, NavigationViewItemInvokedEventArgs args)
        {
            SettingsPanel.Show((args.InvokedItem as string), (args.InvokedItemContainer as NavigationViewItem)?.Tag as string);
        }
    }
}