// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Text.Json;
using System.Threading.Tasks;
using Notepads.Controls;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Legacy;
using Notepads.Features.Sessions.Contracts.Recovery;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Transfer;
using Notepads.Presentation.Controls;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Workspace;
using Windows.Graphics.Printing;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Hosting;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class WorkspaceCommandFeatureTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        await TabCloseSelectionTests.RunAsync(log);
        await DocumentTitleFeatureTests.RunAsync(log);
        await IndentationFeatureTests.RunAsync(log);
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
        await TabDragContractsTests.RunAsync(host, log);
        await VerifyUnverifiedMoveAsync(host);
        await VerifyTransferCleanupAfterCloseAsync(host);
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
        var pending = CreatePendingTab();
        core.OpenTab(pending);
        var unopened = (DocumentLoadingPlaceholder)page.TestContentHost(pending).Content;
        Check(!unopened.IsLoaded && !unopened.TestHasResources,
            "A pending tab allocates no compositor resources before its first layout.");
        core.DeleteTab(pending);
        Check(!core.ContainsTab(pending) && !unopened.TestHasResources,
            "Closing before first layout removes the pending tab without creating animation resources.");
        pending = CreatePendingTab();
        core.OpenTab(pending);
        var placeholder = page.TestContentHost(pending).Content as DocumentLoadingPlaceholder;
        try
        {
            Check(placeholder != null, "Pending document uses a skeleton rather than a loading TextBlock.");
            await UntilAsync(() => placeholder.IsLoaded && placeholder.ActualWidth > 0,
                "The selected skeleton participates in the real workspace layout.");
            await VerifyLoadingPlaceholderAsync(placeholder);
            await VerifyLoadingSizeHintsAsync(host);
            await VerifyLoadingDirectionAsync(host);
            core.SwitchTo(pending);
            var positionButton = Named<Button>(page, "LineColumnIndicatorButton");
            await UntilAsync(() => positionButton.Visibility == Visibility.Collapsed,
                "Pending status collapses the empty position button and its padding.");
            page.TestRefreshMenu();
            Check(core.GetSelectedReadyEditor() == null && !page.TestFeatureStatusInteractive &&
                !Enabled("MenuSaveButton") && !Enabled("MenuSaveAsButton") && !Enabled("MenuFindButton") &&
                !Enabled("MenuReplaceButton") && !Enabled("MenuPrintButton") && Enabled("MenuSaveAllButton"),
                "Pending selection disables editor-only commands while Save All retains recovery admission.");
            Check(page.TestStatusPath == "pending.txt" && page.TestStatusModified &&
                string.IsNullOrEmpty(page.TestStatusPosition) &&
                Named<TextBlock>(page, "FontZoomIndicator").Visibility == Visibility.Collapsed,
                "Pending status keeps recovery metadata without loading text or ready-editor zoom.");
            foreach (var name in new[] { "PathIndicatorButton", "ModificationIndicatorButton", "LineColumnIndicatorButton",
                "FontZoomIndicatorButton", "LineEndingIndicatorButton", "EncodingIndicatorButton", "LanguageIndicatorButton", "IndentationIndicatorButton" })
                Check(!Named<Button>(page, name).IsEnabled,
                    "Pending status disables keyboard and UIA actions as well as pointer input: " + name);
            context = (TabContextFlyout)page.TestContainer(pending).ContextFlyout;
            context.TestFeatureRefresh();
            Check(!context.TestFeatureCommand("Rename").IsEnabled, "A pending tab cannot rename an absent editor.");
            pending.FailLoading(new InvalidOperationException("Loading presentation fixture"));
            await UntilAsync(() => positionButton.Visibility == Visibility.Visible &&
                AutomationProperties.GetName(positionButton) == page.TestStatusPosition,
                "Failed status restores the localized position message and its UIA name.");
            Check(page.TestContentHost(pending).Content is TextBlock failed && !string.IsNullOrWhiteSpace(failed.Text) &&
                !string.IsNullOrWhiteSpace(page.TestStatusPosition),
                "A failed load replaces the skeleton with a visible failure message and status.");
        }
        finally
        {
            core.DeleteTab(pending);
            page.TestController.ResumeRestoration();
        }
        Check(!core.ContainsTab(pending), "A failed tab closes without remaining in the roster.");
        await UntilAsync(() => !placeholder.IsLoaded && !placeholder.TestHasResources,
            "Closing the loading tab releases its animation and compositor resources.");
        core.SwitchTo(first);
        await UntilAsync(() => Named<Button>(page, "LineColumnIndicatorButton").Visibility == Visibility.Visible &&
            AutomationProperties.GetName(Named<Button>(page, "LineColumnIndicatorButton")) == page.TestStatusPosition &&
            !string.IsNullOrWhiteSpace(page.TestStatusPosition),
            "Returning to a ready tab restores the visible position button and its UIA name.");
        page.TestRefreshMenu();
        Check(Enabled("MenuFindButton") && page.TestFeatureStatusInteractive,
            "Returning to a ready tab restores search and status interaction.");
        log.AppendLine("PASS: WorkspaceCommands.Pending — real pending row has accurate menu, status and context guards.");
        log.AppendLine("PASS: WorkspaceCommands.Pending — themed accessible skeleton, RTL alignment, failure and animation cleanup.");

        bool Enabled(string name) => ((Control)page.TestFeatureControl(name)).IsEnabled;
    }

    private static async Task VerifyUnverifiedMoveAsync(FeatureTestHost host)
    {
        var tab = await NewTabAsync(host.Page);
        const string text = "A reported Move alone must not discard this draft Ω 😀";
        Editor(tab).TypeText(text);
        var core = (NotepadsCore)host.Core;
        var operation = Guid.NewGuid();
        var admission = core.TestBeginTabTransfer(operation, tab);
        Check(!admission.Cancel, "Unverified move fixture admits its transfer.");
        var serialized = (string)await admission.Data.GetView().GetDataAsync(TextEditorTransfer.DataFormat);
        var token = JsonSerializer.Deserialize(serialized, SessionJsonContext.Default.TransferToken);
        core.TestEndTabTransfer(operation, tab, Windows.ApplicationModel.DataTransfer.DataPackageOperation.Move);
        await UntilAsync(() => core.TestTabTransferOperation(tab) == null,
            "A published Move completes without depending on a clipboard callback.");
        Check(core.GetTabs().Contains(tab) && Editor(tab).GetText() == text,
            "A reported Move without a durable receiver receipt retains the source text and tab.");
        var offer = await RecoveryRootStore.ReadTransferAsync(token.TransferId, RecoveryRecordKind.TransferOffer);
        Check(offer != null, "An unverified move retains its published recovery offer.");
        await RecoveryRootStore.CreateStore().DeleteAsync(offer.Address); // Only this fixture's root.
        core.DeleteTab(tab);
    }

    private static async Task VerifyTransferCleanupAfterCloseAsync(FeatureTestHost host)
    {
        var tab = await NewTabAsync(host.Page);
        Editor(tab).TypeText("Retain my published recovery Ω 😀");
        var core = (NotepadsCore)host.Core;
        var operation = Guid.NewGuid();
        var admission = core.TestBeginTabTransfer(operation, tab);
        Check(!admission.Cancel, "Recovery retention fixture admits its transfer.");
        var data = (string)await admission.Data.GetView().GetDataAsync(TextEditorTransfer.DataFormat);
        var token = JsonSerializer.Deserialize(data, SessionJsonContext.Default.TransferToken);
        Check(await RecoveryRootStore.ReadTransferAsync(token.TransferId, RecoveryRecordKind.TransferOffer) != null,
            "The real data provider publishes a durable source offer before cancellation.");
        var gate = core.TestTabTransferGate(tab);
        await gate.WaitAsync();
        try
        {
            core.TestEndTabTransfer(operation, tab);
            core.DeleteTab(tab);
        }
        finally { gate.Release(); }
        await UntilAsync(() => core.TestTabTransferOperation(tab) == null,
            "Closing during paused cleanup still completes the admitted operation.");
        Check(await RecoveryRootStore.ReadTransferAsync(token.TransferId, RecoveryRecordKind.TransferOffer) != null,
            "Cancellation rechecks liveness after its gate and retains the closed source's recovery offer.");
        await TransferRecoveryStore.DeleteSourceTransferAsync(token);
    }

    private static WorkspaceTab CreatePendingTab(long? byteLength = null, bool rightToLeft = false)
    {
        var id = Guid.NewGuid();
        var metadata = new DocumentMetadata
        {
            FileNamePlaceholder = "pending.txt", IsModified = true, RightToLeftReadingOrder = rightToLeft
        };
        if (byteLength == null)
            return new(new RecoveryDocumentSource(null, new TextEditorSessionDataV1
            {
                Id = id, EditingFileName = "pending.txt", StateMetaData = metadata
            }, true, null, null, new RecoverySourceLifetime()));
        var descriptor = new RecoveryDescriptor(new TextEditorSessionDataV2
        {
            Id = id, EditingFileName = "pending.txt", StateMetaData = metadata,
            Journal = new DocumentJournalData { DocumentByteLength = byteLength.Value }
        });
        var decision = new RecoveryDocumentDecision(RecoveryMembership.Owned, descriptor, [], [], 0, null, null, false);
        return new(new RecoveryDocumentSource(decision, null, true, null, null, new RecoverySourceLifetime()));
    }

    private static async Task VerifyLoadingSizeHintsAsync(FeatureTestHost host)
    {
        var smallHeight = 0d;
        var largeHeight = 0d;
        foreach (var bytes in new long[] { 0, 120, 65536, int.MaxValue })
        {
            var tab = CreatePendingTab(bytes);
            host.Core.OpenTab(tab);
            var placeholder = (DocumentLoadingPlaceholder)host.Page.TestContentHost(tab).Content;
            try
            {
                await UntilAsync(() => placeholder.IsLoaded && placeholder.ActualHeight > 0,
                    "Size-based skeleton participates in workspace layout.");
                var lines = placeholder.TestLines;
                if (bytes == 0)
                    Check(lines.Length == 1, "An empty checkpoint shows only a minimal loading outline.");
                else if (bytes == 120)
                {
                    smallHeight = placeholder.ActualHeight;
                    Check(lines.Length is >= 1 and <= 3, "A tiny file does not imply a full document of text.");
                }
                else if (bytes == 65536)
                {
                    largeHeight = placeholder.ActualHeight;
                    var bottom = lines.Max(line => line.Offset.Y + line.Size.Y);
                    Check(lines.Length > 8 && largeHeight > smallHeight &&
                        bottom <= largeHeight && largeHeight - bottom < 28,
                        "A large file fills the available height instead of stopping after eight blocks.");
                }
                else
                    Check(placeholder.ActualHeight == largeHeight && lines.Length <= 128,
                        "Huge files use the same bounded visible outline without allocating per document line.");
            }
            finally { host.Core.DeleteTab(tab); }
            await UntilAsync(() => !placeholder.IsLoaded && !placeholder.TestHasResources,
                "Size-based skeleton releases resources when its tab closes.");
        }
    }

    private static async Task VerifyLoadingDirectionAsync(FeatureTestHost host)
    {
        var originalDirection = host.Page.FlowDirection;
        try
        {
            foreach (var chromeDirection in new[] { FlowDirection.LeftToRight, FlowDirection.RightToLeft })
            {
                host.Page.FlowDirection = chromeDirection;
                foreach (var byteLength in new long?[] { null, 65536 })
                foreach (var rightToLeft in new[] { false, true })
                {
                    var tab = CreatePendingTab(byteLength, rightToLeft);
                    host.Core.OpenTab(tab);
                    var placeholder = (DocumentLoadingPlaceholder)host.Page.TestContentHost(tab).Content;
                    try
                    {
                        await UntilAsync(() => placeholder.IsLoaded && placeholder.ActualWidth > 0,
                            "The saved document direction reaches a presented workspace skeleton.");
                        Check(placeholder.FlowDirection == FlowDirection.LeftToRight && placeholder.TestLines.Length > 0 &&
                            placeholder.TestLines.All(line => Math.Abs(line.Offset.X -
                                (rightToLeft ? placeholder.ActualWidth - line.Size.X : 0)) < 0.01),
                            "V1 and V2 skeletons follow saved document direction independently of application direction.");
                        var transform = placeholder.TransformToVisual(null);
                        var left = transform.TransformPoint(new Windows.Foundation.Point(0, 0));
                        var right = transform.TransformPoint(new Windows.Foundation.Point(placeholder.ActualWidth, 0));
                        Check(right.X > left.X, "RTL application chrome does not mirror the document skeleton canvas.");
                    }
                    finally { host.Core.DeleteTab(tab); }
                    await UntilAsync(() => !placeholder.IsLoaded && !placeholder.TestHasResources,
                        "Direction fixtures release their compositor resources.");
                }
            }
        }
        finally { host.Page.FlowDirection = originalDirection; }
    }

    private static async Task VerifyLoadingPlaceholderAsync(DocumentLoadingPlaceholder placeholder)
    {
        var peer = FrameworkElementAutomationPeer.CreatePeerForElement(placeholder);
        Check(peer?.GetAutomationControlType() == AutomationControlType.ProgressBar &&
            peer.GetName() == AutomationProperties.GetName(placeholder) && !string.IsNullOrWhiteSpace(peer.GetName()),
            "A localized loading indicator remains available to UIA without visible loading text.");
        var lines = placeholder.TestLines;
        Check(lines.Length >= 4 && lines.All(line => line.Size.X > 0 && line.Size.X <= placeholder.ActualWidth &&
            line.Offset.X == 0 && line.Offset.Y >= 0 && line.Offset.Y + line.Size.Y <= placeholder.ActualHeight) &&
            lines.Select(line => line.Size.X).Distinct().Count() > 1,
            "Skeleton has bounded, varied, leading-aligned document lines.");
        placeholder.Height = 42;
        await UntilAsync(() => placeholder.ActualHeight == 42, "The skeleton fits a short document viewport.");
        var visual = ElementCompositionPreview.GetElementChildVisual(placeholder);
        Check(visual.Clip != null && visual.Size.Y == 42, "Short viewports clip document blocks at their actual bounds.");
        placeholder.Height = double.NaN;
        await UntilAsync(() => placeholder.ActualHeight >= 200, "The skeleton restores its natural document height.");
        if (!new AccessibilitySettings().HighContrast)
        {
            placeholder.RequestedTheme = ElementTheme.Light;
            await UntilAsync(() => placeholder.ActualTheme == ElementTheme.Light && placeholder.TestBaseColor.R == 0,
                "The light theme uses dark gray blocks.");
            Check(placeholder.TestHighlightColor.A > placeholder.TestBaseColor.A &&
                placeholder.TestHighlightColor.A <= 24 &&
                placeholder.TestHighlightColor.A - placeholder.TestBaseColor.A <= 12,
                "The light theme keeps blocks and the shimmer highlight subdued.");
            placeholder.RequestedTheme = ElementTheme.Dark;
            await UntilAsync(() => placeholder.ActualTheme == ElementTheme.Dark && placeholder.TestBaseColor.R == 255,
                "The dark theme uses light gray blocks.");
            Check(placeholder.TestHighlightColor.A <= 24 &&
                placeholder.TestHighlightColor.A - placeholder.TestBaseColor.A <= 12,
                "The dark theme keeps blocks and the shimmer highlight subdued.");
        }
        Check(placeholder.TestHasAnimation == (Window.Current.Visible && new UISettings().AnimationsEnabled &&
            !new AccessibilitySettings().HighContrast), "Shimmer follows the live system motion and contrast preferences.");
        placeholder.Visibility = Visibility.Collapsed;
        Check(!placeholder.TestHasAnimation, "A collapsed skeleton stops its animation.");
        placeholder.Visibility = Visibility.Visible;
        placeholder.RequestedTheme = ElementTheme.Default;
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
        Check(sets.MoveSet(thirdContainer, 0), "The strip commits an unselected tab reorder.");
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
        Check(sets.MoveSet(thirdContainer, 2), "The strip restores an unselected tab's order.");
        Check(core.GetTabs().SequenceEqual(new[] { first, second, third }) && ReferenceEquals(core.GetSelectedTab(), first),
            "Restoring strip order preserves the selected native document.");
        await TabDragTestHost.FrameAsync();
        var selectedContainer = host.Page.TestContainer(first);
        var content = host.Page.TestContentHost(first).Content;
        var native = ((WinUIEditor.EditorBaseControl)((Grid)Editor(first).TestNativeCore.Content).Children[0]).Editor;
        var document = native.DocPointer;
        int selectionEvents = 0, unloads = 0;
        void Selected(object sender, WorkspaceTab tab) => selectionEvents++;
        void Unloaded(object sender, RoutedEventArgs args) => unloads++;
        core.TabSelected += Selected;
        ((FrameworkElement)firstEditor).Unloaded += Unloaded;
        try
        {
            Check(sets.MoveSet(selectedContainer, 2), "The selected header can move to the end.");
            await TabDragTestHost.FrameAsync();
            Check(unloads == 0, "The selected editor remains attached through the committed reorder's layout.");
            Check(sets.MoveSet(selectedContainer, 0), "The selected header can return to the beginning.");
            await TabDragTestHost.FrameAsync();
            Check(unloads == 0, "Returning the selected header keeps its editor loaded: " + unloads);
            Check(selectionEvents == 0 && ReferenceEquals(core.GetSelectedTab(), first),
                "Selected reorder preserves semantic selection: events=" + selectionEvents);
            Check(ReferenceEquals(content, host.Page.TestContentHost(first).Content) && native.DocPointer == document &&
                firstEditor.GetText() == text && Editor(first).TestNativeCore.CanUndo,
                "Selected reorder retains native document identity, text and undo availability.");
            Check(!sets.MoveSet(selectedContainer, -1) && !sets.MoveSet(selectedContainer, 3),
                "Invalid destinations leave the roster intact.");
        }
        finally
        {
            core.TabSelected -= Selected;
            ((FrameworkElement)firstEditor).Unloaded -= Unloaded;
        }
        var implementation = (NotepadsCore)core;
        var operation = Guid.NewGuid();
        Check(!implementation.TestBeginTabTransfer(operation, first).Cancel, "The first transfer is admitted.");
        var rejected = Guid.NewGuid();
        Check(implementation.TestBeginTabTransfer(rejected, first).Cancel, "Overlapping transfer admission is refused.");
        implementation.TestEndTabTransfer(rejected, first);
        Check(implementation.TestTabTransferOperation(first) == operation,
            "A rejected operation's completion cannot retire an earlier admitted transfer.");
        var gate = implementation.TestTabTransferGate(first);
        await gate.WaitAsync();
        try
        {
            implementation.TestEndTabTransfer(operation, first);
            Check(implementation.TestTabTransferOperation(first) == operation &&
                implementation.TestBeginTabTransfer(Guid.NewGuid(), first).Cancel,
                "Admission remains reserved while real asynchronous cleanup is paused.");
            implementation.TestEndTabTransfer(operation, first); // A duplicate callback must not start a second cleanup.
        }
        finally { gate.Release(); }
        await UntilAsync(() => implementation.TestTabTransferOperation(first) == null,
            "Only completed cleanup releases the admitted transfer reservation.");
        log.AppendLine("PASS: WorkspaceCommands.TabNavigation — navigation and transactional reorder preserve attachment, identity/text/undo; transfer admission/completion has operation identity.");
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
