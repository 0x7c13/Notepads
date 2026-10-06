// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Infrastructure.Diagnostics;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media.Animation;

namespace Notepads.Presentation.Views.Settings;

public sealed partial class SettingsPanel : Page
{
    public SettingsPanel()
    {
        InitializeComponent();
    }

    private SettingsContext _context;

    internal void Initialize(SettingsContext context)
    {
        if (_context != null && !ReferenceEquals(_context, context))
            throw new InvalidOperationException("A settings panel cannot change its window context.");
        _context = context;
    }

    public void Show(string title, string tag)
    {
        if (_context == null) return;
        var pageType = tag switch
        {
            "TextAndEditor" => typeof(TextAndEditorSettingsPage),
            "Personalization" => typeof(PersonalizationSettingsPage),
            "Advanced" => typeof(AdvancedSettingsPage),
            "About" => typeof(AboutPage),
            _ => typeof(TextAndEditorSettingsPage),
        };
        LoggingService.LogInfo($"[{nameof(SettingsPanel)}] Navigating to: {tag} Page", consoleOnly: true);
        TitleTextBlock.Text = title;
        ContentFrame.Navigate(pageType, _context, new SuppressNavigationTransitionInfo());
    }
}
