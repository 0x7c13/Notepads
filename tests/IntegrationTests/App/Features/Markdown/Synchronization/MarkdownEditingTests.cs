// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Presentation.Controls.Markdown;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsWorkspaceTests;
using Windows.Storage;
using Windows.Storage.Streams;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Markup;
using Windows.UI.Xaml.Media;
using Windows.UI.ViewManagement;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    private static async Task EditingAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string disk = "# Unicode\r\n\r\ninitial Ω 😀 e\u0301\0tail\r\n\r\nlast line";
        var editor = await OpenAsync(host, "source.md", Encoding.UTF8.GetBytes(disk));
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        var canonical = disk.Replace("\r\n", "\r");
        Check(editor.GetText() == canonical, "Markdown opens real UTF-8/CRLF/emoji/combining/NUL bytes as canonical native CR text.");
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var session = extension.SourceSession;
        Check(session.FullSubmissions == 1 && session.DeltaSubmissions == 0,
            "Initial Markdown activation submits exactly one complete native source.");
        await CompareFullSyncAsync(host, editor, "initial Unicode source", log);

        var fullBefore = session.FullSubmissions;
        var deltaBefore = session.DeltaSubmissions;
        var copiedBefore = session.CopiedBytes;
        native.BeginUndoAction();
        try
        {
            native.SetSel(native.Length, native.Length);
            editor.TypeText("\rappended Ω😀\0after-NUL");
            native.SetSel(0, 0);
            editor.TypeText("prefix ");
            native.SetSel(7, 9);
            editor.TypeText("##");
        }
        finally { native.EndUndoAction(); }
        var edited = editor.GetText();
        Check(edited.Contains("😀\0after-NUL", StringComparison.Ordinal), "Counted real native insertion preserves text after NUL.");
        await CurrentAsync(editor);
        Check(session.DeltaSubmissions > deltaBefore && session.FullSubmissions == fullBefore && session.CopiedBytes > copiedBefore,
            "A batch of actual multi-point native notifications uses ApplyEdits without a complete resubmission.");
        await CompareFullSyncAsync(host, editor, "batched remote insertion and replacement", log);

        core.Undo();
        Check(editor.GetText() == canonical, "One real native Undo restores the grouped original source including NUL.");
        await CurrentAsync(editor);
        await CompareFullSyncAsync(host, editor, "Undo", log);
        core.Redo();
        Check(editor.GetText() == edited, "Native Redo restores the grouped Unicode edits.");
        await CurrentAsync(editor);
        await CompareFullSyncAsync(host, editor, "Redo", log);

        // This edit is deliberately small relative to the already accepted source.
        core.SetText("# Byte budget\r\r" + new string('a', 180000));
        session.RequestFullSync();
        await CurrentAsync(editor);
        var tinyFull = session.FullSubmissions;
        var tinyDelta = session.DeltaSubmissions;
        var tinyCopied = session.CopiedBytes;
        native.SetSel(native.Length, native.Length);
        editor.TypeText("Ω😀");
        await CurrentAsync(editor);
        Check(session.FullSubmissions == tinyFull && session.DeltaSubmissions > tinyDelta &&
            session.CopiedBytes - tinyCopied < 1024,
            "A six-byte ordinary insertion copies a bounded delta instead of the 180KB document.");
        await CompareFullSyncAsync(host, editor, "bounded ordinary insertion", log);

        var oldSelection = extension.PreviewView.GetSelection();
        editor.Focus();
        var focused = FocusManager.GetFocusedElement();
        native.SetSel(native.Length, native.Length);
        editor.TypeText(" dirty");
        var caret = native.CurrentPos;
        var anchor = native.Anchor;
        Check(!session.IsCurrent, "A synchronous real source modification immediately invalidates the accepted mapping stamp.");
        extension.PreviewView.SelectRange(oldSelection.DocumentId, oldSelection.Revision, oldSelection.TextGeneration, 0, 1);
        await Task.Delay(35);
        Check(native.CurrentPos == caret && native.Anchor == anchor,
            "A selection on the old displayed revision cannot move the dirty source caret.");
        await CurrentAsync(editor);
        Check(ReferenceEquals(focused, FocusManager.GetFocusedElement()), "Asynchronous Markdown completion preserves source focus.");

        const string replacement = "# Reloaded\n\nnew Ω😀\0tail\n";
        await FileIO.WriteBytesAsync(editor.EditingFile, Encoding.UTF8.GetBytes(replacement));
        await editor.ReloadFromEditingFileAsync();
        Check(editor.GetText() == replacement.Replace("\n", "\r"), "Reload commits the real file's LF/Unicode/NUL source.");
        await CurrentAsync(editor);
        await CompareFullSyncAsync(host, editor, "real file reload", log);
        Check(extension.SourceSession.AcceptedStamp.ContentVersion == core.ContentVersion &&
            extension.SourceSession.AcceptedStamp.NativeRevision == native.DocumentRevision &&
            extension.SourceSession.AcceptedStamp.Length == native.Length,
            "The accepted Markdown source stamp names the current native revision, byte length and editor version.");
        log.AppendLine($"PASS: Markdown.Source — real UTF8/CR/NUL edits, batch/delta/copy budget, native Undo/Redo, stale mapping guard, focus and file reload; full={session.FullSubmissions}, delta={session.DeltaSubmissions}, copied={session.CopiedBytes}.");
    }

    private static async Task WrappedViewportAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var paragraph = string.Join(" ", Enumerable.Range(0, 2400).Select(index => "word" + index.ToString("D4")));
        var editor = await OpenAsync(host, "wrapped-viewport.md", Encoding.UTF8.GetBytes(paragraph));
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        var nativeView = (WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0];
        core.TextWrapping = TextWrapping.Wrap;
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        extension.FollowSource = true;
        await UntilAsync(() => !nativeView.IsViewportRestorePending && native.LinesOnScreen > 0 &&
            native.WrapCount(0) > native.LinesOnScreen * 3,
            "One real logical source paragraph wraps across more than three display screens.");
        Check(native.LineCount == 1, "The viewport counterexample has no second logical source line to conceal lost wrap positions.");

        editor.Focus();
        native.SetEmptySelection(0);
        native.FirstVisibleLine = 0;
        await Task.Delay(180); // Drain the real 45ms focus/caret-follow timers before scrolling alone.
        var focused = FocusManager.GetFocusedElement();
        var selection = (native.Anchor, native.CurrentPos, native.SelectionRevision);
        var initial = preview.GetViewportAnchor();
        Check(initial.IsAvailable && extension.SourceSession.IsCurrent, "The source-scroll test starts from a published current preview anchor.");
        var screenRows = native.LinesOnScreen;

        native.FirstVisibleLine = screenRows;
        await UntilAsync(() =>
        {
            var anchor = preview.GetViewportAnchor();
            return !nativeView.IsViewportRestorePending && extension.SourceSession.IsCurrent &&
                native.DocLineFromVisible(native.FirstVisibleLine) == 0 && native.FirstVisibleLine >= screenRows &&
                anchor.IsAvailable && anchor.DocumentId == extension.SourceSession.DocumentId &&
                anchor.Revision == extension.SourceSession.AcceptedRevision &&
                anchor.PreviewUtf16Offset > initial.PreviewUtf16Offset;
        }, "Scrolling to the second wrapped source screen advances the actual published preview anchor within the same paragraph.");
        var secondScreen = preview.GetViewportAnchor();

        native.FirstVisibleLine = screenRows * 2;
        await UntilAsync(() =>
        {
            var anchor = preview.GetViewportAnchor();
            return !nativeView.IsViewportRestorePending && extension.SourceSession.IsCurrent &&
                native.DocLineFromVisible(native.FirstVisibleLine) == 0 && native.FirstVisibleLine >= screenRows * 2 &&
                anchor.IsAvailable && anchor.PreviewUtf16Offset > secondScreen.PreviewUtf16Offset;
        }, "Scrolling to the third source screen advances again although the logical source line is still zero.");
        Check((native.Anchor, native.CurrentPos, native.SelectionRevision) == selection &&
            ReferenceEquals(focused, FocusManager.GetFocusedElement()),
            "Source viewport following preserves the real caret, selection revision and keyboard focus.");

        var scroll = Descendants(preview).OfType<ScrollViewer>().Single(element => element.Name == "PreviewScrollViewer");
        Check(preview.Focus(FocusState.Programmatic), "The real preview accepts focus before its own viewport is scrolled.");
        await UntilAsync(() => preview.FocusState != FocusState.Unfocused && extension.SourceSession.IsCurrent,
            "The preview pane is the actual focused/current source-link owner.");
        await Task.Delay(180); // Settle the focus transition; the next action changes only a viewport.
        focused = FocusManager.GetFocusedElement();
        selection = (native.Anchor, native.CurrentPos, native.SelectionRevision);
        var beforeReverse = preview.GetViewportAnchor();
        var desiredOffset = Math.Min(scroll.ScrollableHeight, scroll.VerticalOffset + scroll.ViewportHeight);
        Check(desiredOffset > scroll.VerticalOffset && scroll.ChangeView(null, desiredOffset, null, true),
            "The real native preview ScrollViewer admits an additional display screen.");
        await UntilAsync(() =>
        {
            var anchor = preview.GetViewportAnchor();
            return extension.SourceSession.IsCurrent && anchor.IsAvailable &&
                anchor.PreviewUtf16Offset > beforeReverse.PreviewUtf16Offset;
        }, "Scrolling the preview publishes a newer actual paragraph viewport anchor.");
        var reverse = preview.GetViewportAnchor();
        var mapped = await preview.MapPreviewPositionAsync(reverse.DocumentId, reverse.Revision,
            reverse.TextGeneration, reverse.PreviewUtf16Offset, MarkdownPositionAffinity.After);
        Check(mapped.SourceByteOffset > 0 && mapped.SourceByteOffset < (ulong)native.Length,
            "The published reverse anchor maps to a real byte inside the single source paragraph.");
        await UntilAsync(() =>
        {
            if (!extension.SourceSession.IsCurrent || nativeView.IsViewportRestorePending ||
                native.DocLineFromVisible(native.FirstVisibleLine) != 0 || native.FirstVisibleLine <= 0) return false;
            var topHit = native.PositionFromPointClose(native.PointXFromPosition(0) + 1, native.TextHeight(0) / 2);
            return topHit > 0 && Math.Abs(native.PointYFromPosition((long)mapped.SourceByteOffset) -
                native.PointYFromPosition(topHit)) <= native.TextHeight(0);
        }, "Reverse viewport following restores the mapped source display row within the paragraph instead of its logical line start.");
        Check((native.Anchor, native.CurrentPos, native.SelectionRevision) == selection &&
            ReferenceEquals(focused, FocusManager.GetFocusedElement()),
            "Reverse scrolling preserves the source caret/selection revision and leaves keyboard focus in the preview.");
        log.AppendLine($"PASS: Markdown.WrappedViewport — one logical paragraph, source screens 2/3 advance preview anchors {initial.PreviewUtf16Offset}/{secondScreen.PreviewUtf16Offset}; reverse anchor maps to source byte {mapped.SourceByteOffset} and its actual wrapped display row without moving caret/focus.");
    }

    private static async Task WindowMoveMappingAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(host, "preview-move.md", Encoding.UTF8.GetBytes(new string('a', 6000)));
        var tab = host.Core.GetSelectedTab();
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        extension.FollowSource = false;
        native.SetEmptySelection(12);
        await UntilAsync(() =>
        {
            var selection = preview.GetSelection();
            var anchor = preview.GetViewportAnchor();
            return selection.IsAvailable && anchor.IsAvailable &&
                selection.DocumentId == preview.DisplayedDocumentId && selection.Revision == preview.DisplayedRevision &&
                anchor.DocumentId == selection.DocumentId && anchor.Revision == selection.Revision &&
                anchor.TextGeneration == selection.TextGeneration;
        }, "The move fixture acquires a current actual selection and viewport identity before issuing a selection command.");
        LogViewportDiagnostics(log, preview, "move before native Focus");
        bool focused;
        try { focused = preview.Focus(FocusState.Programmatic); }
        catch
        {
            LogViewportDiagnostics(log, preview, "move native Focus threw", includeParts: true);
            throw;
        }
        LogViewportDiagnostics(log, preview, "move after native Focus");
        Check(focused, "The move fixture focuses the real preview before queueing its selection mapping.");
        var frame = preview.GetSelection();
        LogViewportDiagnostics(log, preview, "move after GetSelection", frame);
        preview.SelectRange(frame.DocumentId, frame.Revision, frame.TextGeneration, 60, 0);
        extension.FollowSource = true; // Leave a real caret-follow intent queued when the move begins.
        var entered = false;
        (long Anchor, long Caret, ulong Revision) frozen = default;
        TestLauncher.DataLaunch = async (_, _, _) =>
        {
            entered = true;
            frozen = (native.Anchor, native.CurrentPos, native.SelectionRevision);
            Check(editor.IsMovingToWindow && !core.MarkdownCanRead && core.IsWindowMoveStateUnchanged,
                "The existing window-move launch boundary holds an unchanged native capture and disables Markdown mapping.");
            preview.SelectRange(frame.DocumentId, frame.Revision, frame.TextGeneration, 120, 0);
            await Task.Delay(180);
            Check((native.Anchor, native.CurrentPos, native.SelectionRevision) == frozen && core.IsWindowMoveStateUnchanged,
                "An older queued intent and a preview selection during real window transfer cannot change native SelectionRevision.");
            extension.FollowSource = false;
            return false; // Exercise the production refused-launch retirement and rollback path.
        };
        Check(!await host.Core.MoveTabToNewWindowAsync(tab).WaitAsync(TimeSpan.FromSeconds(30)) && entered,
            "The enabled-preview fixture reaches the existing move launcher and performs a real refused-launch rollback.");
        Check(host.Core.ContainsTab(tab) && !editor.IsMovingToWindow && core.IsEnabled && !native.ReadOnly &&
            (native.Anchor, native.CurrentPos, native.SelectionRevision) == frozen,
            "Rollback restores source editing without a background Markdown selection change.");
        await CurrentAsync(editor);
        log.AppendLine("PASS: Markdown.WindowMove — enabled preview, queued/during-transfer caret intents, unchanged native SelectionRevision/frozen capture and existing refused-launch rollback.");
    }

}
