// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Views.MainPage;
using NotepadsIntegrationTests;
using Windows.UI.Xaml.Controls;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static class EncodingMenuTests
{
    internal static async Task RunAsync(FeatureTestHost host, StringBuilder log)
    {
        var page = host.Page;
        page.ShowHideStatusBar(true);
        await UntilAsync(() => page.TestFeatureStatusVisible, "The status bar is loaded.");
        var original = page.TestFeatureEncodingMenu;
        Check(original.Items.Count == 0, "Startup does not construct the unused encoding menu.");
        await OpenAndCheckAsync(page, canReopen: false);
        var entries = original.Items.ToArray();

        var editor = await OpenAsync(host, "encoding-menu.txt", Encoding.UTF8.GetBytes("menu Ω"));
        await OpenAndCheckAsync(page, canReopen: true);
        Check(original.Items.SequenceEqual(entries), "Reopening reuses the menu without duplicate entries.");

        page.ShowHideStatusBar(false);
        await UntilAsync(() => !page.TestFeatureStatusVisible, "The status bar unloads.");
        page.ShowHideStatusBar(true);
        await UntilAsync(() => page.TestFeatureStatusVisible, "The status bar reloads.");
        Check(!ReferenceEquals(original, page.TestFeatureEncodingMenu) && page.TestFeatureEncodingMenu.Items.Count == 0,
            "Reloading the status bar recreates a deferred encoding menu.");
        await OpenAndCheckAsync(page, canReopen: true);

        File.Delete(editor.EditingFilePath);
        await editor.TestCheckFileStatusAsync();
        await OpenAndCheckAsync(page, canReopen: false);
        Check(editor.GetText() == "menu Ω", "Refreshing the encoding menu preserves document contents.");
        log.AppendLine("PASS: documents/encoding-menu loads on first open, reuses entries and survives status-bar recreation.");
        log.AppendLine("PASS: documents/encoding-menu refreshes reopen availability for untitled, saved and deleted files.");
    }

    private static async Task OpenAndCheckAsync(NotepadsMainPage page, bool canReopen)
    {
        var menu = page.TestFeatureEncodingMenu;
        var opened = false;
        var closed = false;
        void OnOpened(object sender, object args) => opened = true;
        void OnClosed(object sender, object args) => closed = true;
        menu.Opened += OnOpened;
        menu.Closed += OnClosed;
        try
        {
            page.TestFeatureOpenEncodingMenu();
            await UntilAsync(() => opened, "The real encoding flyout opens and invokes its XAML Opening handler.");
            Check(menu.Items.Count == 2, "The encoding flyout contains exactly its reopen and save groups.");
            var reopen = menu.Items.OfType<MenuFlyoutSubItem>().Single(item => item.Name == "ReopenWithEncoding");
            var save = menu.Items.OfType<MenuFlyoutSubItem>().Single(item => item.Name == "SaveWithEncoding");
            Check(reopen.IsEnabled == canReopen && save.IsEnabled && reopen.Items.Count >= 6 && save.Items.Count >= 4,
                "Encoding choices are populated and availability matches the current document.");
            menu.Hide();
            await UntilAsync(() => closed, "The encoding flyout closes before the next document transition.");
        }
        finally
        {
            menu.Opened -= OnOpened;
            menu.Closed -= OnClosed;
            menu.Hide();
        }
    }
}
