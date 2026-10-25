// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Preferences;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Transfer;
using Notepads.Features.Sessions.Validation;
using Notepads.Infrastructure.Settings;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Workspace;
using Notepads.Presentation.Workspace.Activation;
using Windows.ApplicationModel;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class WindowMoveFeatureTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        await VerifyFileMoveAsync("SecondaryButton");
        await VerifyFileMoveAsync("PrimaryButton");
        await VerifyFailureAsync();
        await VerifyCancellationAsync();
        foreach (var rightToLeft in new[] { false, true }) await VerifyMoveAsync(rightToLeft);
        await VerifyMoveAsync(false, diff: true);
        await VerifyFileMoveAsync();
        await VerifyPreferredColumnAsync();
        foreach (var rtl in new[] { false, true }) await VerifyRectangleAsync(rtl);
        log.AppendLine("PASS: window move menu admission, refused launch, background draft move, .LOG, undo/redo, selections, RTL and duplicate delivery.");
        log.AppendLine("PASS: first/repeated receiver snapshots and secondary Cancel/Save/Discard preserve content and durably retire closed tabs.");
    }

    private static async Task VerifyFailureAsync()
    {
        await using var source = await FeatureTestHost.CreateAsync();
        var tab = source.Core.GetSelectedTab();
        var editor = (TextEditor)tab.ReadyEditor;
        editor.TypeText("source 中文 remains");
        TransferToken token = null;
        TestLauncher.DataLaunch = (uri, options, data) =>
        {
            token = JsonSerializer.Deserialize((string)data[WindowMoveActivation.DataKey], SessionJsonContext.Default.TransferToken);
            Check(options.TargetApplicationPackageFamilyName == Package.Current.Id.FamilyName && uri.AbsolutePath == "/" + token.TransferId.ToString("N"),
                "The menu launches an identity-only request addressed to this package.");
            return Task.FromResult(false);
        };
        Check(!await source.Core.MoveTabToNewWindowAsync(tab), "A refused broker launch does not report a move.");
        Check(source.Core.ContainsTab(tab) && editor.GetText() == "source 中文 remains" && editor.TestNativeCore.IsEnabled &&
            !editor.TestNativeCore.TestFeatureNative.ReadOnly, "Refusal retains the source text and releases both editing guards.");
        using (await SessionRecoveryTransaction.EnterAsync())
        {
            bool refused = false;
            try { await WindowMoveStore.ReadPendingAsync(token, CancellationToken.None); }
            catch (FileNotFoundException) { refused = true; }
            catch (DirectoryNotFoundException) { refused = true; }
            Check(refused, "Failed launch fences late activation before source editing resumes.");
        }
        editor.TestNativeCore.TestFeatureNative.Undo();
        Check(editor.IsDocumentEmpty, "Refusal preserves the original undo history.");
    }

    private static async Task VerifyCancellationAsync()
    {
        await using var source = await FeatureTestHost.CreateAsync();
        var tab = source.Core.GetSelectedTab();
        var editor = (TextEditor)tab.ReadyEditor;
        editor.TypeText("cancelled move retains this text");
        var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var launch = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        TestLauncher.DataLaunch = (_, _, _) => { entered.TrySetResult(); return launch.Task; };
        try
        {
            var moving = source.Core.MoveTabToNewWindowAsync(tab);
            await Task.WhenAny(entered.Task, moving).WaitAsync(TimeSpan.FromSeconds(20));
            Check(entered.Task.IsCompleted, "The cancellation fixture must reach the broker before it can cancel the move.");
            editor.CancelWindowMove();
            Check(!await moving.WaitAsync(TimeSpan.FromSeconds(20)) && source.Core.ContainsTab(tab) &&
                editor.IsEnabled && !editor.TestNativeCore.TestFeatureNative.ReadOnly,
                "Cancellation fences and releases the source without waiting for the broker callback.");
        }
        finally { launch.TrySetResult(true); }
    }

    private static async Task VerifyMoveAsync(bool rtl, bool diff = false)
    {
        await using var source = await FeatureTestHost.CreateAsync();
        var original = source.Core.GetSelectedTab();
        var editor = (TextEditor)original.ReadyEditor;
        await UntilAsync(() => editor.IsLoaded && editor.TestNativeCore.IsLoaded,
            "The draft must finish its initial empty .LOG check before typing a header.");
        editor.TypeText(".LOG\rabc אבג 中文\rtail");
        var native = editor.TestNativeCore.TestFeatureNative;
        native.EmptyUndoBuffer();
        native.BeginUndoAction(); native.InsertText(5, "X"); native.EndUndoAction();
        native.BeginUndoAction(); native.InsertText(6, "Y"); native.EndUndoAction();
        native.Undo();
        native.SetSelection(8, 6); native.AddSelection(3, 1); native.MainSelection = 1;
        editor.TestNativeCore.RightToLeftReadingOrder = rtl;
        editor.TestNativeCore.TextWrapping = TextWrapping.NoWrap;
        editor.TestNativeCore.SetFontZoomFactor(150);
        var fontSize = editor.TestNativeCore.FontSize;
        if (diff)
        {
            await editor.OpenSideBySideDiffViewerAsync();
            Check(editor.Mode == TextEditorMode.DiffPreview, "The source diff preview was prepared.");
        }
        var expected = editor.GetText();
        var title = editor.DisplayTitle;
        var other = await source.Core.CreateNewTextEditorAsync("other");
        source.Core.OpenTextEditor(other);
        Check(native.Selections == 2 && native.MainSelection == 1 && native.GetSelectionNCaret(0) == 8 &&
            native.GetSelectionNAnchor(0) == 6 && native.GetSelectionNCaret(1) == 3 && native.GetSelectionNAnchor(1) == 1,
            $"Source selections before moving (diff={diff}, rtl={rtl}): {DescribeSelections(native)}");
        Check(source.Core.GetSelectedReadyEditor() == other, "The move fixture targets a background tab.");
        var menu = (TabContextFlyout)source.Page.TestContainer(original).ContextFlyout;
        menu.TestFeatureRefresh();
        Check(source.Core.CanMoveTabToNewWindow(original) && menu.TestFeatureCommand("MoveToNewWindow").IsEnabled,
            "The context menu admits a ready background tab.");

        FeatureTestHost receiver = null;
        TransferToken token = null;
        var launched = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        TestLauncher.DataLaunch = async (_, _, data) =>
        {
            token = JsonSerializer.Deserialize((string)data[WindowMoveActivation.DataKey], SessionJsonContext.Default.TransferToken);
            launched.TrySetResult();
            await release.Task;
            receiver = await FeatureTestHost.CreateAsync(token);
            return true;
        };
        try
        {
            var moving = source.Core.MoveTabToNewWindowAsync(original);
            try
            {
                await launched.Task.WaitAsync(TimeSpan.FromSeconds(20));
                Check(!source.Core.CanMoveTabToNewWindow(original) && !await source.Core.MoveTabToNewWindowAsync(original),
                    "The reserved tab refuses duplicate moves before launch completes.");
                editor.TypeText("must not insert");
                if (diff) { editor.CloseSideBySideDiffViewer(); await editor.ToggleDiffPreviewAsync(); }
                Check(!diff || editor.Mode == TextEditorMode.DiffPreview, "A reserved diff cannot be closed or toggled.");
                Check(editor.GetText() == expected && !editor.IsEnabled && !editor.TestNativeCore.IsEnabled && native.ReadOnly,
                    "The source remains protected while its target is being prepared.");
            }
            finally { release.TrySetResult(); }
            Check(await moving.WaitAsync(TimeSpan.FromSeconds(40)), "A durable receiver receipt commits the source move.");
            Check(!source.Core.ContainsTab(original) && source.Core.GetSelectedReadyEditor() == other && source.Core.GetTabCount() == 1,
                "Moving a background tab preserves the other tab and its selection.");
            Check(receiver.Core.GetTabCount() == 1, "Dedicated startup opens only the moved document.");
            var received = (TextEditor)receiver.Core.GetSelectedReadyEditor();
            Check(received.Mode == (diff ? TextEditorMode.DiffPreview : TextEditorMode.Editing),
                "The requested view mode is restored before the source is removed.");
            Check(received.GetText() == expected && received.DisplayTitle == title && received.IsModified,
                "The receiver retains draft text/title/dirty status without appending another .LOG timestamp.");
            Check(received.TestNativeCore.RightToLeftReadingOrder == rtl && received.TestNativeCore.TextWrapping == TextWrapping.NoWrap &&
                received.TestNativeCore.GetFontZoomFactor() == 150 && received.TestNativeCore.FontSize == fontSize,
                "Document direction, wrapping and zoom survive the move.");
            var movedNative = received.TestNativeCore.TestFeatureNative;
            Check(movedNative.Selections == 2 && movedNative.MainSelection == 1 && movedNative.GetSelectionNCaret(0) == 8 &&
                movedNative.GetSelectionNAnchor(0) == 6 && movedNative.GetSelectionNCaret(1) == 3 && movedNative.GetSelectionNAnchor(1) == 1,
                $"Both directed selections and the main selection survive target attachment (diff={diff}, rtl={rtl}): {DescribeSelections(movedNative)}");
            await receiver.Page.ActivateWindowMoveAsync(token);
            Check(receiver.Core.GetTabCount() == 1 && received.GetText() == expected, "Duplicate activation observes the original import task.");
            if (diff) received.CloseSideBySideDiffViewer();
            movedNative.Redo(); Check(received.GetText() == ".LOG\rXYabc אבג 中文\rtail", "The moved redo tail remains available.");
            movedNative.Undo(); movedNative.Undo();
            Check(received.GetText() == ".LOG\rabc אבג 中文\rtail", "Both moved undo groups restore their original text.");
        }
        finally { if (receiver != null) await receiver.DisposeAsync(); }
    }

    private static async Task VerifyFileMoveAsync(string closeButton = null)
    {
        await using var source = await FeatureTestHost.CreateAsync();
        const string saved = "saved 中文\r\nline";
        var encoding = new UTF8Encoding(true);
        var editor = await NotepadsWorkspaceTests.DocumentFeatureAssertions.OpenAsync(source, "move-file.txt",
            encoding.GetPreamble().Concat(encoding.GetBytes(saved)).ToArray());
        var file = editor.EditingFile;
        editor.TestNativeCore.TestFeatureNative.SetEmptySelection(editor.TestNativeCore.TestFeatureNative.Length);
        editor.TypeText("\r\nchanged");
        var expected = editor.GetText();
        var original = source.Core.GetSelectedTab();
        var remaining = await source.Core.CreateNewTextEditorAsync("remaining source tab");
        remaining.TypeText("Keep the source window alive; last-tab process exit is tested by the broker suite.");
        source.Core.OpenTextEditor(remaining);
        FeatureTestHost receiver = null;
        var snapshots = ApplicationPreferences.IsSessionSnapshotEnabled;
        Guid targetOwner = default;
        var launched = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        TestLauncher.DataLaunch = async (_, _, data) =>
        {
            var token = JsonSerializer.Deserialize((string)data[WindowMoveActivation.DataKey], SessionJsonContext.Default.TransferToken);
            targetOwner = token.TransferId;
            if (closeButton != null)
            {
                ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, true);
                ApplicationPreferences.RefreshSessionSnapshotSetting();
            }
            receiver = await FeatureTestHost.CreateAsync(token);
            launched.TrySetResult();
            return true;
        };
        try
        {
            var menu = (TabContextFlyout)source.Page.TestContainer(original).ContextFlyout;
            menu.TestFeatureRefresh();
            var item = menu.TestFeatureCommand("MoveToNewWindow");
            var peer = new MenuFlyoutItemAutomationPeer(item);
            ((IInvokeProvider)peer.GetPattern(PatternInterface.Invoke)).Invoke();
            await launched.Task.WaitAsync(TimeSpan.FromSeconds(30));
            await UntilAsync(() => !source.Core.ContainsTab(original), "The actual menu command did not remove the acknowledged source tab.");
            var received = (TextEditor)receiver.Core.GetSelectedReadyEditor();
            Check(received.EditingFile.IsEqual(file) && received.GetText() == expected && received.IsModified,
                "A dirty saved file retains its exact file association and dirty contents.");
            received.TestNativeCore.TestFeatureNative.Undo();
            Check(received.GetText() == saved.Replace("\r\n", "\r") && !received.IsModified,
                "Undo reaches the transferred saved baseline/save point.");
            received.TestNativeCore.TestFeatureNative.Redo();
            if (closeButton != null)
            {
                // The receiving process starts with snapshots enabled, but skips session restoration.
                // Its first discovery must not archive/close the just-received live document.
                Check(await receiver.Page.TestController.SaveSessionAsync(), "The first snapshot of a moved document succeeds.");
                Check(await receiver.Page.TestController.SaveSessionAsync(), "Repeated snapshots preserve the moved document's recovery authority.");
                var cancel = receiver.Page.TestCloseWindowAsync();
                await ChooseDialogAsync(cancel, "CloseButton");
                Check(!await cancel && receiver.Page.IsEnabled && !receiver.Core.IsClosing &&
                    receiver.Page.TestController.IsBackupEnabled && received.IsModified && received.GetText() == expected,
                    "Cancel preserves a secondary window, its unsaved document and snapshot protection.");
                var close = receiver.Page.TestCloseWindowAsync();
                await ChooseDialogAsync(close, closeButton);
                Check(await close && !receiver.Page.TestController.IsBackupEnabled,
                    "Save or discard approves secondary window closure and stops further backup while shutdown drains.");
                using (await SessionRecoveryTransaction.EnterAsync())
                {
                    var authority = await SessionRecoveryCatalog.ReadScopeAsync(targetOwner);
                    Check(SessionRecoveryAuthority.IsClosed(authority.Stamp, received.Id,
                        authority.Decisions.Select(read => read.Record)), "Explicit secondary closure retires the recovery row so it cannot reappear on restart.");
                }
                var written = closeButton == "PrimaryButton" ? expected.Replace("\r", "\r\n") : saved;
                await NotepadsWorkspaceTests.DocumentFeatureAssertions.BytesAsync(file,
                    encoding.GetPreamble().Concat(encoding.GetBytes(written)).ToArray(), "Secondary Save writes edits; Don't Save leaves the original file unchanged.");
                return;
            }
            Check(await receiver.Page.TestSaveDocumentAsync(received) && !received.IsModified,
                "The receiver can save through its transferred file grant.");
            await NotepadsWorkspaceTests.DocumentFeatureAssertions.BytesAsync(file,
                encoding.GetPreamble().Concat(encoding.GetBytes(expected.Replace("\r", "\r\n"))).ToArray(),
                "Save retains UTF-8 BOM and CRLF policy on the same file.");
        }
        finally
        {
            if (receiver != null) await receiver.DisposeAsync();
            ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, snapshots);
            ApplicationPreferences.RefreshSessionSnapshotSetting();
        }
    }

    private static string DescribeSelections(WinUIEditor.Editor native) => $"main={native.MainSelection}; " +
        string.Join(", ", Enumerable.Range(0, checked((int)native.Selections))
            .Select(index => $"{native.GetSelectionNCaret(index)}/{native.GetSelectionNAnchor(index)}"));

    private static async Task VerifyPreferredColumnAsync()
    {
        await using var source = await FeatureTestHost.CreateAsync();
        var tab = source.Core.GetSelectedTab();
        var editor = (TextEditor)tab.ReadyEditor;
        await UntilAsync(() => editor.TestNativeCore.IsLoaded, "The source must have its real display scale.");
        editor.TestNativeCore.TextWrapping = TextWrapping.NoWrap;
        editor.TypeText(new string('a', 60) + "\rx\r" + new string('a', 60));
        var native = editor.TestNativeCore.TestFeatureNative;
        native.SetEmptySelection(30);
        native.ChooseCaretX(); native.LineDown();
        Check(native.CurrentPos == 62, "The short line clamps the caret while retaining column 30.");
        var other = await source.Core.CreateNewTextEditorAsync("remaining");
        source.Core.OpenTextEditor(other);
        FeatureTestHost receiver = null;
        TestLauncher.DataLaunch = async (_, _, data) =>
        {
            var token = JsonSerializer.Deserialize((string)data[WindowMoveActivation.DataKey], SessionJsonContext.Default.TransferToken);
            receiver = await FeatureTestHost.CreateAsync(token);
            return true;
        };
        try
        {
            Check(await source.Core.MoveTabToNewWindowAsync(tab), "The preferred-column fixture moves successfully.");
            var received = (TextEditor)receiver.Core.GetSelectedReadyEditor();
            await UntilAsync(() => received.TestNativeCore.IsLoaded, "The target must attach at its real display scale.");
            var target = received.TestNativeCore.TestFeatureNative;
            target.LineDown();
            Check(target.CurrentPos == 93, $"First Down after attachment restores column 30, not its 96-DPI projection: {target.CurrentPos}.");
        }
        finally { if (receiver != null) await receiver.DisposeAsync(); }
    }

    private static async Task VerifyRectangleAsync(bool rtl)
    {
        await using var source = await FeatureTestHost.CreateAsync();
        var tab = source.Core.GetSelectedTab();
        var editor = (TextEditor)tab.ReadyEditor;
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        await UntilAsync(() => core.IsLoaded, "The rectangle source is attached.");
        await NotepadsEditorTests.EditorTestLayout.AfterPaintAsync(native, () =>
        {
            core.Width = 220;
            core.TextWrapping = TextWrapping.Wrap;
            core.RightToLeftReadingOrder = rtl;
            editor.TypeText(new string(rtl ? 'א' : 'a', 128));
            core.UpdateLayout();
        });
        var bytesPerCharacter = rtl ? 2 : 1;
        var secondRow = Enumerable.Range(1, 127).Select(i => (long)i * bytesPerCharacter)
            .First(p => native.PointYFromPosition(p) > native.PointYFromPosition(0));
        native.SelectionMode = WinUIEditor.SelectionMode.Rectangle;
        native.RectangularSelectionAnchor = bytesPerCharacter;
        native.RectangularSelectionCaret = secondRow + 2 * bytesPerCharacter;
        var expected = SelectionRanges(native);
        var main = native.MainSelection;
        Check(expected.Length == 2 && expected.All(r => r.Caret != r.Anchor), "Two wrapped rows have nonempty rectangular selections.");
        var other = await source.Core.CreateNewTextEditorAsync("remaining");
        source.Core.OpenTextEditor(other);
        FeatureTestHost receiver = null;
        TestLauncher.DataLaunch = async (_, _, data) =>
        {
            var token = JsonSerializer.Deserialize((string)data[WindowMoveActivation.DataKey], SessionJsonContext.Default.TransferToken);
            receiver = await FeatureTestHost.CreateAsync(token);
            return true;
        };
        try
        {
            Check(await source.Core.MoveTabToNewWindowAsync(tab), "The wrapped rectangle moves successfully.");
            var received = (TextEditor)receiver.Core.GetSelectedReadyEditor();
            await UntilAsync(() => received.TestNativeCore.IsLoaded && received.TestNativeCore.ActualWidth > 220,
                "The receiving editor is attached in a wider viewport.");
            var target = received.TestNativeCore.TestFeatureNative;
            // Resolve the actual target layout before inspecting selections; checking
            // only the detached import would miss style-refresh reprojection.
            _ = target.PointYFromPosition(target.Length);
            Check(target.SelectionMode == WinUIEditor.SelectionMode.Rectangle && target.MainSelection == main &&
                SelectionRanges(target).SequenceEqual(expected),
                $"First target layout preserves every selected byte and virtual column (rtl={rtl}): {DescribeSelections(target)}.");
            var originalText = received.GetText();
            var deleted = originalText;
            foreach (var range in expected.OrderByDescending(r => Math.Min(r.Caret, r.Anchor)))
                deleted = deleted.Remove(checked((int)Math.Min(range.Caret, range.Anchor) / bytesPerCharacter),
                    checked((int)Math.Abs(range.Caret - range.Anchor) / bytesPerCharacter));
            target.Clear();
            Check(received.GetText() == deleted, "Deleting after reflow removes exactly the originally selected characters.");
            target.Undo();
            Check(received.GetText() == originalText, "One undo restores the moved rectangular deletion.");
            target.SelectionMode = WinUIEditor.SelectionMode.Rectangle;
            target.RectangularSelectionAnchor = bytesPerCharacter;
            target.RectangularSelectionCaret = 2 * bytesPerCharacter;
            Check(target.Selections == 1 && target.GetSelectionNStart(0) == bytesPerCharacter &&
                target.GetSelectionNEnd(0) == 2 * bytesPerCharacter,
                "An explicit new rectangle still projects its corners against the current layout.");
        }
        finally { if (receiver != null) await receiver.DisposeAsync(); }
    }

    private static (long Caret, long Anchor, long CaretSpace, long AnchorSpace)[] SelectionRanges(WinUIEditor.Editor native) =>
        Enumerable.Range(0, checked((int)native.Selections)).Select(i => (native.GetSelectionNCaret(i), native.GetSelectionNAnchor(i),
            native.GetSelectionNCaretVirtualSpace(i), native.GetSelectionNAnchorVirtualSpace(i))).ToArray();
}
