// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Navigation;

namespace Notepads.Presentation.Views.Settings;

public sealed partial class SettingsPage : Page
{
    private SettingsContext _context;

    public SettingsPage()
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
        SettingsPanel.Initialize(context);

        Loaded += SettingsPage_Loaded;
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        Initialize(e.Parameter as SettingsContext ?? throw new ArgumentException("Settings navigation requires a context."));
    }

    private void SettingsPage_Loaded(object sender, RoutedEventArgs e)
    {
        if (_context == null) return;
        Loaded -= SettingsPage_Loaded;
        SettingsNavigationView.SelectedItem = TextAndEditorSettingsItem;
    }

    private void SettingsNavigationView_OnSelectionChanged(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
    {
        if (_context == null || args.SelectedItem is not NavigationViewItem item) return;
        SettingsPanel.Show(item.Content as string, item.Tag as string);
    }
}
