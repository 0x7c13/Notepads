
namespace Notepads.Controls.Settings
{
    using Notepads.Services;
    using System;
    using System.Collections.Generic;
    using System.IO;
    using System.Linq;
    using System.Runtime.InteropServices.WindowsRuntime;
    using Windows.Foundation;
    using Windows.Foundation.Collections;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Controls.Primitives;
    using Windows.UI.Xaml.Data;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;
    using Windows.UI.Xaml.Media.Animation;
    using Windows.UI.Xaml.Navigation;

    public sealed partial class SettingsPage : Page
    {
        public SettingsPage()
        {
            InitializeComponent();
            Loaded += SettingsPage_Loaded;
        }

        private void SettingsPage_Loaded(object sender, RoutedEventArgs e)
        {
            ((NavigationViewItem)SettingsNavigationView.MenuItems.First()).IsSelected = true;
        }

        private void SettingsPanel_OnItemInvoked(NavigationView sender, NavigationViewItemInvokedEventArgs args)
        {
            SettingsPanel.Show(args.InvokedItem as string);
        }
    }
}
