// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Preferences;
using Notepads.Features.WebSearch;
using Notepads.Infrastructure.Shell;
using Notepads.Presentation.Controls.Print;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Workspace.Activation;
using NotepadsIntegrationTests;
using Windows.ApplicationModel;
using Windows.Storage;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Documents;
using Windows.UI.Xaml.Media;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static class ShellWorkflowTests
{
    internal static async Task RunActivationAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var simple = await host.Folder.CreateFileAsync("simple.txt");
        var spaced = await host.Folder.CreateFileAsync("space name.txt");
        await FileIO.WriteTextAsync(simple, "simple");
        await FileIO.WriteTextAsync(spaced, "space");
        var child = await host.Folder.CreateFolderAsync("nested");
        var nested = await child.CreateFileAsync("child.txt");
        await FileIO.WriteTextAsync(nested, "child");
        var cases = new[]
        {
            (host.Folder.Path, "notepads simple.txt", simple),
            (host.Folder.Path, "NOTEPADS.EXE \".\\space name.txt\"", spaced),
            (host.Folder.Path, "\"C:\\Program Files\\Notepads\\Notepads.exe\" \"space name.txt\"", spaced),
            (host.Folder.Path, "notepads .\\nested/child.txt", nested),
            (child.Path, "notepads ..\\simple.txt", simple),
            (host.Folder.Path, "notepads \"" + spaced.Path + "\"", spaced)
        };
        foreach (var (directory, command, expected) in cases)
        {
            var resolved = await CommandLineFiles.OpenFileFromCommandLineAsync(directory, command, "Notepads");
            Check(resolved?.IsEqual(expected) == true, "Command-line parsing/opening must resolve the expected scoped file: " + command);
        }
        const string variable = "NOTEPADS_INTEGRATION_DOCUMENT_ROOT";
        var previous = Environment.GetEnvironmentVariable(variable);
        try
        {
            Environment.SetEnvironmentVariable(variable, host.Folder.Path);
            var resolved = await CommandLineFiles.OpenFileFromCommandLineAsync(child.Path,
                "notepads \"%" + variable + "%\\space name.txt\"", "Notepads");
            Check(resolved?.IsEqual(spaced) == true, "Command-line opening must expand process environment variables before resolving a quoted path.");
        }
        finally { Environment.SetEnvironmentVariable(variable, previous); }
        foreach (var command in new[] { "", "notepads", "notepads \"unterminated", "notepads missing.txt" })
            Check(await CommandLineFiles.OpenFileFromCommandLineAsync(host.Folder.Path, command, "Notepads") == null,
                "Missing or malformed command-line input must not produce a file: " + command);
        Check(CommandLineFiles.IsFullPath(simple.Path) && !CommandLineFiles.IsFullPath("simple.txt") &&
            !CommandLineFiles.IsFullPath("\\simple.txt"), "Command-line full-path recognition must distinguish absolute, relative and root-relative paths.");
        var selected = await CommandLineFiles.OpenFileFromCommandLineAsync(host.Folder.Path, "notepads simple.txt", "Notepads");
        Check(await host.Page.OpenFileAsync(selected) && host.Core.GetSelectedReadyEditor().GetText() == "simple",
            "A command-line resolved StorageFile must open through actual MainPage/Core admission.");
        var count = host.Core.GetTabCount();
        Check(!await host.Page.OpenFileAsync(selected) && host.Core.GetTabCount() == count,
            "Reopening the same command-line file must reuse its existing tab.");
        log.AppendLine("PASS: shell/command-line resolves real quoted, relative, absolute and environment paths; malformed/missing input and duplicate admission are preserved.");

        foreach (var (uri, expected) in new[]
        {
            ("notepads://newinstance", NotepadsOperationProtocol.OpenNewInstance),
            ("NOTEPADS://NEWINSTANCE/", NotepadsOperationProtocol.OpenNewInstance),
            ("notepads://other", NotepadsOperationProtocol.Unrecognized),
            ("notepads://newinstance/extra", NotepadsOperationProtocol.Unrecognized),
            ("notepads://newinstance?payload=1", NotepadsOperationProtocol.Unrecognized),
            ("https://newinstance/", NotepadsOperationProtocol.Unrecognized)
        })
            Check(NotepadsProtocolService.GetOperationProtocol(new Uri(uri)) == expected,
                "Protocol parsing must match the supported operation exactly: " + uri);
        TestLauncher.Reset();
        try
        {
            Check(!await NotepadsProtocolService.LaunchProtocolAsync(NotepadsOperationProtocol.Unrecognized) && TestLauncher.Requests.Count == 0,
                "Unsupported protocol operations must not reach the launch boundary.");
            Check(await NotepadsProtocolService.LaunchProtocolAsync(NotepadsOperationProtocol.OpenNewInstance),
                "Supported protocol launch must return the launcher result.");
            var request = TestLauncher.Requests.Single();
            Check(request.Uri.Scheme == "notepads" && request.Uri.Host == "newinstance" &&
                request.Options.TargetApplicationPackageFamilyName == Package.Current.Id.FamilyName,
                "New-instance launch must target the current test package family and carry only its operation URI.");
            TestLauncher.Result = false;
            Check(!await NotepadsProtocolService.LaunchProtocolAsync(NotepadsOperationProtocol.OpenNewInstance),
                "A declined OS launch must remain a declined protocol action.");
            TestLauncher.Failure = new IOException("Injected launch failure");
            Check(!await NotepadsProtocolService.LaunchProtocolAsync(NotepadsOperationProtocol.OpenNewInstance),
                "A launcher exception must become protocol failure without escaping to the UI.");
        }
        finally { TestLauncher.Reset(); }
        log.AppendLine("PASS: shell/protocol parsing and launch target/result/error policies execute against the explicit outbound boundary.");
    }

    internal static async Task RunOpenPickerAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        TestFileOpenPicker.Reset();
        try
        {
            var before = host.Core.GetTabCount();
            TestFileOpenPicker.Results.Enqueue(Array.Empty<StorageFile>());
            await host.Page.TestOpenPickerAsync();
            Check(host.Core.GetTabCount() == before, "Canceling an Open picker must not create or close tabs.");
            Check(TestFileOpenPicker.LastCreated.FileTypeFilter.First() == "*" &&
                TestFileOpenPicker.LastCreated.FileTypeFilter.Contains(".txt"),
                "The real open-picker factory must offer all files and supported text documents.");
            var first = await host.Folder.CreateFileAsync("picker-first.txt");
            var second = await host.Folder.CreateFileAsync("picker-second.txt");
            await FileIO.WriteTextAsync(first, "first");
            await FileIO.WriteTextAsync(second, "second");
            TestFileOpenPicker.Results.Enqueue(new[] { first, second, first });
            await host.Page.TestOpenPickerAsync();
            Check(host.Core.GetTabs().Count(tab => tab.ReadyEditor?.EditingFile?.IsEqual(first) == true) == 1 &&
                host.Core.GetTabs().Count(tab => tab.ReadyEditor?.EditingFile?.IsEqual(second) == true) == 1,
                "Opening multiple picker results must create one tab per distinct file and reuse repeats.");
            Check(host.Core.GetSelectedReadyEditor().EditingFile.IsEqual(first) && host.Core.GetSelectedReadyEditor().GetText() == "first",
                "The final repeated picker result must select its existing document without replacing its text.");
            var retained = host.Core.GetTabs();
            TestFileOpenPicker.Failure = new IOException("Injected picker failure");
            await ChooseDialogAsync(host.Page.TestOpenPickerAsync(), "PrimaryButton", "Injected picker failure");
            Check(host.Core.GetTabs().SequenceEqual(retained), "A picker failure must preserve all existing tabs and their order after the real error dialog closes.");
            await CheckOpenRecentAsync(host, first, second, log);
        }
        finally { TestFileOpenPicker.Reset(); }
        log.AppendLine("PASS: shell/open-picker cancel, multiple results, duplicate reuse and actual error dialog preserve workspace state.");
    }

    private static async Task CheckOpenRecentAsync(FeatureTestHost host, StorageFile first, StorageFile second, StringBuilder log)
    {
        try
        {
            MRUService.ClearAll();
            await host.Page.TestBuildOpenRecentAsync();
            Check(!host.Page.TestOpenRecentMenu.IsEnabled && host.Page.TestOpenRecentMenu.Items.Count == 0,
                "An empty recent-file list must not offer an enabled empty submenu.");
            var unopened = await host.Folder.CreateFileAsync("recent-unopened.txt");
            await FileIO.WriteTextAsync(unopened, "opened from recent Ω");
            MRUService.SeedForTest(second, host.Folder, unopened, first, first);
            await host.Page.TestBuildOpenRecentAsync();
            var recent = host.Page.TestOpenRecentMenu;
            var commands = recent.Items.OfType<MenuFlyoutItem>().ToArray();
            Check(recent.IsEnabled && commands.Length == 4 &&
                commands.Take(3).Select(item => item.Text).SequenceEqual(new[] { second.Path, unopened.Path, first.Path }),
                "Open Recent must retain file order, suppress duplicate paths and omit non-file entries.");
            var count = host.Core.GetTabCount();
            FeatureUI.Invoke(commands.Single(item => item.Text == unopened.Path));
            await UntilAsync(() => host.Core.GetSelectedReadyEditor()?.EditingFile?.IsEqual(unopened) == true &&
                host.Page.TestOpenRecentMenu.Items.OfType<MenuFlyoutItem>().FirstOrDefault()?.Text == unopened.Path,
                "The actual recent-file command must open its scoped file and refresh recent ordering.");
            Check(host.Core.GetTabCount() == count + 1 && host.Core.GetSelectedReadyEditor().GetText() == "opened from recent Ω",
                "Opening an unopened recent file must create one ready tab with the file's content.");
            FeatureUI.Invoke(host.Page.TestOpenRecentMenu.Items.OfType<MenuFlyoutItem>().Single(item => item.Text == first.Path));
            await UntilAsync(() => host.Core.GetSelectedReadyEditor()?.EditingFile?.IsEqual(first) == true,
                "A recent-file command for an existing document must select its existing tab.");
            Check(host.Core.GetTabCount() == count + 1 && host.Core.GetSelectedReadyEditor().GetText() == "first",
                "Reopening a recent document must preserve the workspace count and existing content.");
            FeatureUI.Invoke(host.Page.TestOpenRecentMenu.Items.OfType<MenuFlyoutItem>().Last());
            await UntilAsync(() => !host.Page.TestOpenRecentMenu.IsEnabled && host.Page.TestOpenRecentMenu.Items.Count == 0,
                "Clear Recently Opened must rebuild the actual menu as disabled and empty.");
            Check((await MRUService.GetMostRecentlyUsedListAsync()).Count == 0 && host.Core.GetTabCount() == count + 1,
                "Clearing recents must clear the declared list boundary without closing document tabs.");
            log.AppendLine("PASS: shell/open-recent filters ordered file entries, opens/reuses actual tabs and clears only the recent list.");
        }
        finally { MRUService.ClearAll(); }
    }

    internal static async Task RunOutboundAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var engine = ApplicationPreferences.EditorDefaultSearchEngine;
        var template = ApplicationPreferences.EditorCustomMadeSearchUrl;
        try
        {
            ApplicationPreferences.EditorDefaultSearchEngine = SearchEngine.Custom;
            ApplicationPreferences.EditorCustomMadeSearchUrl = "https://search.example.test/?q={0}";
            var editor = await OpenAsync(host, "web-search.txt", Encoding.UTF8.GetBytes("  https://example.test/path?q=one  \rΩ & two"));
            TestLauncher.Reset();
            editor.TestNativeCore.SetTextSelectionPosition(0, 0);
            await editor.TestNativeCore.SearchInWebAsync();
            Check(TestLauncher.Requests.Count == 0, "Web search without a selection must not dispatch an outbound request.");
            editor.TestNativeCore.SetTextSelectionPosition(0, 35);
            await editor.TestNativeCore.SearchInWebAsync();
            Check(TestLauncher.Requests.Single().Uri.AbsoluteUri == "https://example.test/path?q=one",
                "Selected HTTP(S) URLs must be trimmed and opened directly at the boundary.");
            TestLauncher.Reset();
            editor.TestNativeCore.SetTextSelectionPosition(36, 43);
            await editor.TestNativeCore.SearchInWebAsync();
            Check(TestLauncher.Requests.Single().Uri.AbsoluteUri == "https://search.example.test/?q=%CE%A9%20%26%20two",
                "Selected search text must be escaped once and placed in the configured query template.");

            var nonWeb = await OpenAsync(host, "web-search-scheme.txt", Encoding.UTF8.GetBytes("mailto:user@example.test"));
            nonWeb.TestNativeCore.SelectAll();
            TestLauncher.Reset();
            await nonWeb.TestNativeCore.SearchInWebAsync();
            Check(TestLauncher.Requests.Single().Uri.AbsoluteUri == "https://search.example.test/?q=mailto%3Auser%40example.test",
                "A non-HTTP(S) URI must become search text instead of dispatching its external scheme.");

            var longQuery = new string('x', 2001);
            var longEditor = await OpenAsync(host, "web-search-long.txt", Encoding.UTF8.GetBytes(longQuery));
            longEditor.TestNativeCore.SelectAll();
            TestLauncher.Reset();
            await longEditor.TestNativeCore.SearchInWebAsync();
            Check(TestLauncher.Requests.Single().Uri.Query == "?q=" + new string('x', 2000),
                "Web search must enforce its 2000-character outbound selection limit.");
            TestLauncher.Failure = new IOException("Injected browser failure");
            await longEditor.TestNativeCore.SearchInWebAsync();
            Check(longEditor.GetText() == longQuery && !longEditor.IsModified,
                "An outbound browser failure must preserve document content and clean state.");
            Selection(longEditor, 0, 2001, "An outbound browser failure must preserve selection");
        }
        finally
        {
            TestLauncher.Reset();
            ApplicationPreferences.EditorDefaultSearchEngine = engine;
            ApplicationPreferences.EditorCustomMadeSearchUrl = template;
        }
        log.AppendLine("PASS: shell/web-search selection admission, direct URLs, query escaping, length cap and failure preservation use the outbound launcher boundary.");
    }

    internal static async Task RunPrintingAsync(StringBuilder log)
    {
        TestPrintManager.ResetRequests();
        var subscribers = TestPrintManager.SubscriptionCount;
        Canvas canvas;
        try
        {
            await using (var host = await FeatureTestHost.CreateAsync())
            {
                canvas = host.Page.TestPrintCanvas;
                Check(TestPrintManager.SubscriptionCount == subscribers + 1,
                    "The actual MainPage must register exactly one print-task handler while loaded.");
                var first = await OpenAsync(host, "print-first.txt", Encoding.UTF8.GetBytes("first Ω\rnext"));
                first.TestNativeCore.SetTextSelectionPosition(0, 5);
                await host.Page.PrintAsync(first);
                Check(TestPrintManager.ShowCount == 1 && canvas.Children.Count == 1,
                    "Printing a nonempty attached editor must prepare content and request the print UI once.");
                var firstPage = (PrintPageFormat)canvas.Children[0];
                Check(PrintText(firstPage) == "first Ω\rnext", "Printing must use the whole document even when only a word is selected.");
                var block = (RichTextBlock)firstPage.FindName("TextContent");
                Check(block.FontFamily.Source == ApplicationPreferences.EditorFontFamily && block.FontSize == ApplicationPreferences.EditorFontSize,
                    "Prepared print content must use the actual configured editor font.");
                var second = await OpenAsync(host, "print-second.txt", Encoding.UTF8.GetBytes("second"));
                await host.Page.PrintAllAsync(host.Core.GetTabs());
                Check(TestPrintManager.ShowCount == 2 && canvas.Children.Cast<PrintPageFormat>().Select(PrintText)
                    .SequenceEqual(new[] { "first Ω\rnext", "second" }),
                    "Print All must prepare nonempty documents in tab order and dispatch a single print UI request.");

                var unattached = await host.Core.CreateNewTextEditorAsync("unattached-print");
                try
                {
                    unattached.TypeText("must not print");
                    await host.Page.PrintAsync(unattached);
                    Check(TestPrintManager.ShowCount == 2 && canvas.Children.Count == 2,
                        "Print admission must reject an editor that does not belong to a workspace tab.");
                }
                finally { await host.Core.DiscardUnattachedEditorAsync(unattached); }

                var empty = await OpenAsync(host, "print-empty.txt", Array.Empty<byte>());
                await host.Page.PrintAsync(empty);
                Check(TestPrintManager.ShowCount == 2 && canvas.Children.Count == 0,
                    "An empty document must clear stale print content and never dispatch the chooser.");
                var oversized = await OpenAsync(host, "print-over-limit.txt", Encoding.UTF8.GetBytes(new string('Ω', 524289)));
                await host.Page.PrintAsync(oversized);
                Check(TestPrintManager.ShowCount == 2 && canvas.Children.Count == 0,
                    "A document over the 1 MiB native-byte admission limit must not prepare print content or dispatch the chooser.");
                TestPrintManager.Supported = false;
                await host.Page.PrintAsync(second);
                Check(TestPrintManager.ShowCount == 2, "Unsupported printing must not request the OS print chooser.");
                TestPrintManager.Supported = true;
                TestPrintManager.ShowFailure = new IOException("Injected print chooser failure");
                await host.Page.PrintAsync(second);
                Check(TestPrintManager.ShowCount == 3 && second.GetText() == "second" && !second.IsModified,
                    "The real print error handler must contain chooser errors while preserving the document.");
                TestPrintManager.ShowFailure = null;
            }
            Check(TestPrintManager.SubscriptionCount == subscribers && canvas.Children.Count == 0,
                "Disposing MainPage must unregister its print-task handler and release every retained print page.");
            var formatted = new PrintPageFormat("body Ω", new FontFamily("Consolas"), 16, "Header", "Footer");
            Check(PrintText(formatted) == "body Ω" && ((TextBlock)formatted.FindName("HeaderTextBlock")).Text == "Header" &&
                ((TextBlock)formatted.FindName("FooterTextBlock")).Text == "Footer" &&
                ((StackPanel)formatted.FindName("Header")).Visibility == Visibility.Visible &&
                ((StackPanel)formatted.FindName("Footer")).Visibility == Visibility.Visible,
                "The actual print-page control must retain body, header and footer text.");
            log.AppendLine("PASS: shell/print uses actual content controls, document admission, tab ordering and registration cleanup; OS chooser/spooler is an external gate.");
        }
        finally { TestPrintManager.ResetRequests(); }
    }

    private static string PrintText(PrintPageFormat page) =>
        string.Concat(((Paragraph)page.FindName("TextEditorContent")).Inlines.OfType<Run>().Select(run => run.Text));
}
