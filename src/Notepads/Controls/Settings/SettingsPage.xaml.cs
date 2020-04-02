namespace Notepads.Controls.Settings
{
    using Microsoft.Gaming.XboxGameBar;
    using Notepads.Services;
    using System;
    using System.Linq;
    using Windows.UI.ViewManagement;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media;
    using Windows.UI.Xaml.Navigation;

    public sealed partial class SettingsPage : Page
    {
        private XboxGameBarWidget _widget; // maintain throughout the lifetime of the settings widget

        public SettingsPage()
        {
            InitializeComponent();
            Loaded += SettingsPage_Loaded;

            if (App.IsGameBarWidget)
            {
                ThemeSettingsService.OnRequestThemeUpdate += ThemeSettingsService_OnRequestThemeUpdate;
                ThemeSettingsService.OnRequestAccentColorUpdate += ThemeSettingsService_OnRequestAccentColorUpdate;

                ThemeSettingsService.SetRequestedTheme(null, Window.Current.Content, null);
            }
        }

        private async void ThemeSettingsService_OnRequestAccentColorUpdate(object sender, EventArgs e)
        {
            await Dispatcher.RunAsync(Windows.UI.Core.CoreDispatcherPriority.Normal, () =>
            {
                ThemeSettingsService.SetRequestedAccentColor();
            });
        }

        private async void ThemeSettingsService_OnRequestThemeUpdate(object sender, EventArgs e)
        {
            await Dispatcher.RunAsync(Windows.UI.Core.CoreDispatcherPriority.Normal, () =>
            {
                ThemeSettingsService.SetRequestedTheme(null, Window.Current.Content, null);
            });
        }

        private void SettingsPage_Loaded(object sender, RoutedEventArgs e)
        {
            ((NavigationViewItem)SettingsNavigationView.MenuItems.First()).IsSelected = true;
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
            // Unregister events
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