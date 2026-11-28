// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.TextEditor;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using WinUIEditor;
using static NotepadsEditorTests.EditorTestLayout;

namespace NotepadsIntegrationTests;

internal static class MarkdownSourceViewportPendingWrapTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        var previous = Window.Current.Content;
        using var core = new TextEditorCore
        {
            Width = 620, Height = 351, TextWrapping = TextWrapping.Wrap,
            HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
        };
        var view = (EditorBaseControl)((Grid)core.Content).Children[0];
        var native = view.Editor;
        const long targetLine = 5;
        var target = new string('M', 40);
        var text = new string('x', 0x200000 + 1) + "\ra\ra\ra\ra\r" + target + "\r" +
            string.Join('\r', Enumerable.Range(0, 200).Select(index => "tail " + index));
        Check(Encoding.UTF8.GetByteCount(text) < 4 * 1024 * 1024,
            "The explicit >visible-wrap-budget predecessor still fits the approved 4 MiB source limit.");
        var paints = 0;
        var layoutWakesAfterPaint = 0;
        var armedAtPaint = int.MaxValue;
        void Painted(Editor sender, PaintedEventArgs args) => paints++;
        EventHandler geometry = (_, _) =>
        {
            if (paints > armedAtPaint) layoutWakesAfterPaint++;
        };
        // Subscribe before Loaded attaches the bridge's Painted observer, so the
        // counter reflects the actual paint before that observer sends its wake.
        native.Painted += Painted;
        core.MarkdownViewportGeometryChanged += geometry;
        try
        {
            core.SetText(text);
            await AfterPaintAsync(native, () =>
            {
                Window.Current.Content = core;
                Window.Current.Activate();
                view.RestoreViewport(0, targetLine, 0, 0, true);
            }, TimeSpan.FromSeconds(60));
            await NotepadsWorkspaceTests.WorkspaceTestUI.UntilAsync(() =>
                !view.IsViewportRestorePending && native.WrapCount(targetLine) == 1 &&
                core.TryGetMarkdownViewport(out var sample) && sample.StartByte == native.PositionFromLine(targetLine),
                "The target must first be stable as one display row at the old actual width.", 60000);
            var revision = native.DocumentRevision;
            var length = native.Length;
            var oldRow = native.VisibleFromDocLine(targetLine);
            Check(native.LineCount > targetLine + native.LinesOnScreen * 2,
                "The target has enough following rows to exclude document-bottom clamping.");
            var paintsBeforeArrange = paints;
            core.Width = 180;
            core.UpdateLayout();
            // No await and no AfterPaint here: this is the first sample after
            // real ChangeSize/NeedWrapping at the new width, not a stable test.
            Check(paints == paintsBeforeArrange && view.ActualWidth < 200 &&
                native.TextWidth(32, target) > view.ActualWidth * view.XamlRoot.RasterizationScale &&
                native.WrapCount(targetLine) == 1,
                "The pending-width precondition must be observed before paint with the old one-row pcs height.");
            armedAtPaint = paints;
            var scalarAvailable = false;
            try
            {
                var scalar = view.ViewportSourceByte;
                scalarAvailable = true;
                Check(scalar == native.PositionFromLine(targetLine) && native.WrapCount(targetLine) > 1,
                    "The public byte getter must reject a mixed current layout/old pcs height before returning an anchor.");
            }
            catch (Exception error) when (error.HResult == unchecked((int)0x8000000E))
            {
                // E_ILLEGAL_METHOD_CALL is the scalar API's unavailable result.
            }
            var available = core.TryGetMarkdownViewport(out var first);
            var targetStart = native.PositionFromLine(targetLine);
            var followingStart = native.PositionFromLine(targetLine + 1);
            Check(!available || (first.StartByte == targetStart && first.EndByte > targetStart && first.EndByte < followingStart),
                "The first budget-limited getter must reject a mixed layout or return the target's true adjacent soft-wrap boundary.");
            log.AppendLine($"OBS: Markdown.SourceViewportPendingWrap scalarAvailable={scalarAvailable}, firstAvailable={available}, rowBefore={oldRow}, span={first.StartByte}..{first.EndByte}, target={targetStart}..{followingStart}, paintsBefore={paintsBeforeArrange}, paintsAtFirst={paints}.");

            MarkdownViewportSample settled = default;
            if (!available)
            {
                var consumedWake = 0;
                await NotepadsWorkspaceTests.WorkspaceTestUI.UntilAsync(() =>
                {
                    if (layoutWakesAfterPaint <= consumedWake) return false;
                    consumedWake = layoutWakesAfterPaint;
                    return core.TryGetMarkdownViewport(out settled) && settled.StartByte == targetStart &&
                        settled.EndByte > targetStart && settled.EndByte < followingStart;
                }, "An unavailable sample must be retried from a subsequent real native paint/layout wake, without another scroll input.", 60000);
                Check(layoutWakesAfterPaint > 0, "The bridge actually observed a post-unavailable native paint wake.");
            }
            else settled = first;
            Check(native.DocumentRevision == revision && native.Length == length && native.WrapCount(targetLine) > 1,
                "The settled target uses the new multi-row layout without changing source bytes.");
            await AfterPaintAsync(native, () => view.SetVerticalViewport(settled.DisplayRow + 1, 0));
            await NotepadsWorkspaceTests.WorkspaceTestUI.UntilAsync(() => core.TryGetMarkdownViewport(out var next) &&
                next.StartByte == settled.EndByte && next.Fraction == 0,
                "After actual wrap completion the adjacent display row shares exactly the sampled endpoint.");
            log.AppendLine($"PASS: Markdown.SourceViewportPendingWrap — >0x200000-byte L-5 predecessor, stable old width then synchronous pending-width first getter; available implies true subline, otherwise actual paint wake recovers; source unchanged and next endpoint shared; native paints={paints}, paint-layout wakes={layoutWakesAfterPaint}.");
        }
        finally
        {
            native.Painted -= Painted;
            core.MarkdownViewportGeometryChanged -= geometry;
            core.StopMarkdownViewportSampling();
            Window.Current.Content = previous;
        }
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
