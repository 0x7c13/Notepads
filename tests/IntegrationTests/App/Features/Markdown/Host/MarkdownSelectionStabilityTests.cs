// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    private static async Task PendingSourceCaretSelectionAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string source = "# Selection stability\r\rBase <sup>Raised</sup> end.";
        var editor = await OpenAsync(host, "selection-stability.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        var native = editor.TestNativeCore.TestFeatureNative;
        editor.Focus();
        native.SetEmptySelection(0);
        await UntilAsync(() => !extension.TestSourceFindBusy,
            "The source's initial real mapping retires before the held-result case.");
        Check(preview.FocusState == FocusState.Unfocused, "The test preserves source focus and source viewport authority.");
        var identity = preview.GetSelection();
        var start = (ulong)preview.DisplayedText.IndexOf("Raised", StringComparison.Ordinal);
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var text = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        var range = text.DocumentRange.FindText("Raised", false, false);
        range.GetBoundingRectangles(out var beforeBounds);
        var beforeAnchor = preview.GetViewportAnchor();
        var sourceSelection = (native.Anchor, native.CurrentPos, native.SelectionRevision);
        var focused = Windows.UI.Xaml.Input.FocusManager.GetFocusedElement();
        var held = extension.TestHoldNextSourceCaret(out var release);
        try
        {
            extension.TestQueueSourceCaret();
            await held.WaitAsync(TimeSpan.FromSeconds(15));
            preview.SelectRange(identity.DocumentId, identity.Revision, identity.TextGeneration, start, 6);
            var immediate = preview.GetSelection();
            Check(immediate.Start == start && immediate.Length == 6, "An external nonempty preview selection is immediately current.");
            release();
            await UntilAsync(() => !extension.TestSourceFindBusy,
                "The real older source caret mapping completes after the external selection.");
            await TabDragTestHost.FrameAsync();
            var after = preview.GetSelection();
            range.GetBoundingRectangles(out var afterBounds);
            var afterAnchor = preview.GetViewportAnchor();
            var selected = string.Join("|", text.GetSelection().Select(value => value.GetText(-1)));
            log.AppendLine($"OBS: HeldSourceCaret selected={immediate.Start}+{immediate.Length}->{after.Start}+{after.Length}, text='{selected}', layout={beforeAnchor.LayoutGeneration}->{afterAnchor.LayoutGeneration}, anchor={beforeAnchor.PreviewUtf16Offset}:{beforeAnchor.OffsetWithinLine:R}->{afterAnchor.PreviewUtf16Offset}:{afterAnchor.OffsetWithinLine:R}.");
            Check(after.IsAvailable && after.Start == start && after.Length == 6 && selected == "Raised" &&
                beforeBounds.SequenceEqual(afterBounds) && beforeAnchor.LayoutGeneration == afterAnchor.LayoutGeneration &&
                beforeAnchor.PreviewUtf16Offset == afterAnchor.PreviewUtf16Offset &&
                beforeAnchor.OffsetWithinLine == afterAnchor.OffsetWithinLine &&
                sourceSelection == (native.Anchor, native.CurrentPos, native.SelectionRevision) &&
                ReferenceEquals(focused, Windows.UI.Xaml.Input.FocusManager.GetFocusedElement()),
                "A later external preview selection survives an older real source caret result without moving geometry, viewport, source selection or focus.");

            var sourceByte = Encoding.UTF8.GetByteCount(source[..(source.IndexOf("end", StringComparison.Ordinal) + 1)]);
            var mapped = await preview.MapSourcePositionAsync(identity.DocumentId, identity.Revision,
                identity.TextGeneration, (ulong)sourceByte, Notepads.Markdown.MarkdownPositionAffinity.Before);
            native.SetEmptySelection(sourceByte);
            await UntilAsync(() => preview.GetSelection().Length == 0 &&
                preview.GetSelection().Start == mapped.PreviewUtf16Offset && !extension.TestSourceFindBusy,
                "A genuinely newer source caret movement can synchronize again after the preview selection.");
            log.AppendLine("PASS: Markdown.PendingSourceCaretSelection — real held mapping, unfocused external selection, exact text/geometry/viewport/focus and subsequent source navigation.");
        }
        finally { release(); }
    }
}
