// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Controls;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Sessions.Contracts.Legacy;
using Notepads.Features.Sessions.Recovery;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Workspace;
using Windows.Graphics.Printing;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using static NotepadsWorkspaceTests.WorkspaceTestUI;
using static NotepadsIntegrationTests.FeatureUI;

namespace NotepadsIntegrationTests;

internal static class WorkspaceCommandFeatureTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var page = host.Page;
        var core = host.Core;
        var first = core.GetSelectedTab();
        Check(first?.ReadyEditor != null && core.GetTabs().Length == 1, "Commands start with one ready blank tab.");
        page.TestRefreshMenu();
        Check(!Enabled("MenuSaveButton") && Enabled("MenuSaveAsButton") && Enabled("MenuFindButton") &&
            !Enabled("MenuSaveAllButton"), "A blank ready tab has Save As and Find, without spurious dirty commands.");

        var second = await NewTabAsync(page);
        var third = await NewTabAsync(page, toolbar: true);
        Editor(first).TypeText("first Ω\rsecond 😀");
        await VerifyTabNavigationAndOrderAsync(host, first, second, third, log);
        core.SwitchTo(first);
        await UntilAsync(() => page.TestStatusModified, "Editing updates the selected status modification marker.");
        page.TestRefreshMenu();
        Check(Enabled("MenuSaveButton") && Enabled("MenuSaveAllButton"), "Typing enables both real save commands.");

        var editor = Editor(first);
        var nativeCore = editor.TestNativeCore;
        var menu = (TextEditorContextFlyout)editor.GetContextFlyout();
        nativeCore.SetTextSelectionPosition(0, 0);
        menu.TestFeatureRefresh();
        Check(menu.Undo.IsEnabled && !menu.Redo.IsEnabled && !menu.Cut.IsEnabled && !menu.Copy.IsEnabled &&
            menu.WebSearch.Visibility == Visibility.Collapsed, "Insertion context exposes undo and hides selection-only commands.");
        Invoke(menu.SelectAll);
        menu.TestFeatureRefresh();
        Check(nativeCore.GetSelectedText() == "first Ω\rsecond 😀" && menu.Cut.IsEnabled && menu.Copy.IsEnabled &&
            menu.WebSearch.Visibility == Visibility.Visible, "Actual Select All updates native selection and context admission.");
        Invoke(menu.Undo);
        await UntilAsync(() => editor.IsDocumentEmpty && !editor.IsModified, "Context Undo restores the initial save point.");
        menu.TestFeatureRefresh();
        Check(!menu.Undo.IsEnabled && menu.Redo.IsEnabled, "Undo refresh exposes Redo.");
        Invoke(menu.Redo);
        await UntilAsync(() => editor.GetText() == "first Ω\rsecond 😀" && editor.IsModified,
            "Context Redo restores Unicode text and modified state.");
        var initialWrap = nativeCore.TextWrapping;
        Invoke(menu.WordWrap);
        Check(nativeCore.TestFeatureNative.WrapMode == (initialWrap == TextWrapping.Wrap ? WinUIEditor.Wrap.None : WinUIEditor.Wrap.Word),
            "Context Word Wrap changes the native layout mode.");
        Invoke(menu.WordWrap);
        Check(nativeCore.TextWrapping == initialWrap, "A second context command restores wrapping.");
        nativeCore.IsEnabled = false;
        menu.TestFeatureRefresh();
        page.TestRefreshMenu();
        Check(!menu.Undo.IsEnabled && !menu.Redo.IsEnabled && !menu.Cut.IsEnabled && !menu.Paste.IsEnabled &&
            !Enabled("MenuFindButton") && !Enabled("MenuReplaceButton"), "Disabled editors reject editing/search commands.");
        nativeCore.IsEnabled = true;
        editor.SetLanguageOverride("markdown");
        menu.TestFeatureRefresh();
        Check(menu.PreviewToggle.Visibility == Visibility.Collapsed, "Unavailable Markdown preview stays hidden for Markdown documents.");
        editor.ShowHideContentPreview();
        Check(editor.Mode == TextEditorMode.Editing && editor.GetText() == "first Ω\rsecond 😀",
            "The unavailable preview route preserves document text and edit mode.");
        log.AppendLine("PASS: WorkspaceCommands.NativeContext — real new-tab, selection, undo/redo, wrap and disabled/Markdown guards.");

        core.SwitchTo(second);
        var context = (TabContextFlyout)page.TestContainer(second).ContextFlyout;
        context.TestFeatureRefresh();
        Check(!context.TestFeatureCommand("CopyFullPath").IsEnabled &&
            !context.TestFeatureCommand("OpenContainingFolder").IsEnabled && context.TestFeatureCommand("Rename").IsEnabled,
            "Untitled tab context offers rename without file-only actions.");
        Invoke(context.TestFeatureCommand("CloseRight"));
        await UntilAsync(() => !core.ContainsTab(third), "Close Right removes the ready tab to the right.");
        Check(core.GetTabs().SequenceEqual(new[] { first, second }) && ReferenceEquals(core.GetSelectedTab(), second),
            "Close Right preserves left tab order, dirty content and the selected source tab.");
        var fourth = await NewTabAsync(page);
        context = (TabContextFlyout)page.TestContainer(first).ContextFlyout;
        context.TestFeatureRefresh();
        Invoke(context.TestFeatureCommand("CloseSaved"));
        await UntilAsync(() => core.GetTabs().Length == 1, "Close Saved finishes the clean-tab close requests.");
        Check(ReferenceEquals(core.GetSelectedTab(), first) && !core.ContainsTab(second) && !core.ContainsTab(fourth) &&
            editor.GetText() == "first Ω\rsecond 😀", "Close Saved retains the dirty document and selects it.");
        var fifth = await NewTabAsync(page);
        core.SwitchTo(first);
        context.TestFeatureRefresh();
        Invoke(context.TestFeatureCommand("CloseOthers"));
        await UntilAsync(() => !core.ContainsTab(fifth), "Close Others removes the other clean tab.");
        Check(core.GetTabs().Single() == first, "Close Others retains its context document.");
        log.AppendLine("PASS: WorkspaceCommands.TabContext — Close Right, Close Saved and Close Others preserve dirty text and selection.");

        // Pause the real scheduler while inspecting a logical recovery row. This
        // isolates presentation admission without loading the descriptor fixture.
        page.TestController.PauseRestoration();
        var pending = new WorkspaceTab(new RecoveryDocumentSource(null, new TextEditorSessionDataV1
        {
            Id = Guid.NewGuid(), EditingFileName = "pending.txt",
            StateMetaData = new DocumentMetadata { FileNamePlaceholder = "pending.txt", IsModified = true }
        }, true, null, null, new RecoverySourceLifetime()));
        core.OpenTab(pending);
        try
        {
            page.TestRefreshMenu();
            Check(core.GetSelectedReadyEditor() == null && !page.TestFeatureStatusInteractive &&
                !Enabled("MenuSaveButton") && !Enabled("MenuSaveAsButton") && !Enabled("MenuFindButton") &&
                !Enabled("MenuReplaceButton") && !Enabled("MenuPrintButton") && Enabled("MenuSaveAllButton"),
                "Pending selection disables editor-only commands while Save All retains recovery admission.");
            Check(page.TestStatusPath == "pending.txt" && page.TestStatusModified &&
                Named<TextBlock>(page, "FontZoomIndicator").Visibility == Visibility.Collapsed,
                "Pending status shows its recovery title/dirty marker and hides ready-editor zoom.");
            context = (TabContextFlyout)page.TestContainer(pending).ContextFlyout;
            context.TestFeatureRefresh();
            Check(!context.TestFeatureCommand("Rename").IsEnabled, "A pending tab cannot rename an absent editor.");
        }
        finally
        {
            core.DeleteTab(pending);
            page.TestController.ResumeRestoration();
        }
        Check(!core.ContainsTab(pending), "A pending tab can close before its first layout without remaining in the roster.");
        core.SwitchTo(first);
        page.TestRefreshMenu();
        Check(Enabled("MenuFindButton") && page.TestFeatureStatusInteractive,
            "Returning to a ready tab restores search and status interaction.");
        log.AppendLine("PASS: WorkspaceCommands.Pending — real pending row has accurate menu, status and context guards.");
        log.AppendLine("PASS: WorkspaceCommands.Pending — closing before first layout removes the pending tab safely.");

        bool Enabled(string name) => ((Control)page.TestFeatureControl(name)).IsEnabled;
    }

    private static async Task VerifyTabNavigationAndOrderAsync(FeatureTestHost host,
        WorkspaceTab first, WorkspaceTab second, WorkspaceTab third, StringBuilder log)
    {
        var core = host.Core;
        var firstEditor = first.ReadyEditor;
        var secondEditor = second.ReadyEditor;
        var thirdEditor = third.ReadyEditor;
        var text = firstEditor.GetText();
        core.SwitchTo(true);
        await UntilAsync(() => ReferenceEquals(core.GetSelectedTab(), first), "Next-tab command wraps the last tab to the first.");
        core.SwitchTo(false);
        await UntilAsync(() => ReferenceEquals(core.GetSelectedTab(), third), "Previous-tab command wraps the first tab to the last.");
        core.SwitchTo(1);
        await UntilAsync(() => ReferenceEquals(core.GetSelectedTab(), second), "Direct-index command selects the middle tab.");
        core.SwitchTo(-1);
        core.SwitchTo(3);
        Check(ReferenceEquals(core.GetSelectedTab(), second), "Out-of-range tab shortcuts preserve the current selection.");

        var sets = Named<SetsView>(host.Page, "Sets");
        Check(sets.CanDragItems && sets.CanReorderItems, "The production tab strip enables same-window reordering.");
        var thirdContainer = host.Page.TestContainer(third);
        // UWP owns the drag gesture. Exercise its actual item-collection boundary;
        // this assertion covers workspace reaction, not physical pointer delivery.
        sets.Items.RemoveAt(2);
        sets.Items.Insert(0, thirdContainer);
        Check(core.GetTabs().SequenceEqual(new[] { third, first, second }) &&
            ReferenceEquals(core.GetSelectedTab(), second) && ReferenceEquals(core.GetSelectedReadyEditor(), secondEditor),
            "Reordering a tab around the selected document preserves selected identity and exposes the new order.");
        core.SwitchTo(true);
        await UntilAsync(() => ReferenceEquals(core.GetSelectedTab(), third), "Next-tab wrap follows the reordered strip.");
        core.SwitchTo(1);
        await UntilAsync(() => ReferenceEquals(core.GetSelectedTab(), first), "Direct-index command follows the reordered strip.");
        Check(ReferenceEquals(first.ReadyEditor, firstEditor) && ReferenceEquals(third.ReadyEditor, thirdEditor) &&
            firstEditor.GetText() == text && firstEditor.IsModified && Editor(first).TestNativeCore.CanUndo,
            "Collection reordering preserves native editor ownership, Unicode edits and undo availability.");
        sets.Items.RemoveAt(0);
        sets.Items.Insert(2, thirdContainer);
        Check(core.GetTabs().SequenceEqual(new[] { first, second, third }) && ReferenceEquals(core.GetSelectedTab(), first),
            "Restoring strip order preserves the selected native document.");
        log.AppendLine("PASS: WorkspaceCommands.TabNavigation — actual next/previous/index commands, wrap and invalid-index guards; collection-boundary reorder preserves identity/text/undo (physical drag not exercised).");
    }

    internal static async Task RunPrintStateAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var page = host.Page;
        var content = host.Core.GetSelectedTab();
        Editor(content).TypeText("printable text");
        page.TestRefreshMenu();
        var print = (MenuFlyoutItem)page.TestFeatureControl("MenuPrintButton");
        Check(print.Visibility == (PrintManager.IsSupported() ? Visibility.Visible : Visibility.Collapsed),
            "Print capability determines menu visibility without opening the OS print UI.");
        if (!PrintManager.IsSupported())
        {
            log.AppendLine("PASS: PrintMenu.Capability — unsupported printing hides the command.");
            return;
        }
        Check(print.IsEnabled, "Nonempty editable text enables Print.");
        Editor(content).TestNativeCore.IsEnabled = false;
        page.TestRefreshMenu();
        var fromNonempty = print.IsEnabled;
        var blank = await NewTabAsync(page);
        page.TestRefreshMenu();
        Check(!print.IsEnabled, "Empty text disables Print.");
        host.Core.SwitchTo(content);
        page.TestRefreshMenu();
        Check(print.IsEnabled == fromNonempty,
            "Print admission for the same disabled, nonempty editor must not depend on the previously selected tab.");
        Check(print.IsEnabled, "Disabled editing still permits printing the original nonempty document (for example Diff Preview).");
        Editor(content).TestNativeCore.IsEnabled = true;
        log.AppendLine("PASS: PrintMenu.HistoryIndependent — disabled nonempty document remains printable after an empty selection.");
    }
}
