// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text.Json;
using System.Threading.Tasks;
using Notepads.Markdown;
using WinUIEditor;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Media;
using static NotepadsEditorTests.EditorTestLayout;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal sealed partial class MarkdownKeyboardFixture
{
    private readonly record struct FollowOracle(SourceViewportSpan Source, ulong MainStart, ulong MainEnd);
    private readonly record struct FollowInverse(ulong Start, ulong End, double SourceStart, double SourceEnd);
    private readonly record struct FollowProbe(string Name, double Requested, double Expected, double Actual,
        double Tolerance, bool Admitted, bool Preserved);
    private readonly record struct FollowFrame(long Qpc, bool Available, long Row, double Fraction, long StartByte,
        long EndByte, ulong MainStart, ulong MainEnd, double Progress, double LogicalY, double NextY, long NextQpc,
        double OracleMain, double OracleSource, long Caret, long SelectionAnchor, ulong SelectionRevision, string Error);

    private readonly List<FollowOracle> _followOracle = new();
    private readonly List<FollowInverse> _followInverse = new();
    private readonly List<FollowProbe> _followProbes = new();
    private readonly List<FollowFrame> _followFrames = new();
    private bool _followPrepared, _followTracing, _followOverflow;
    private long _followPublications, _followTraceStart, _followNextQpc;
    private double _followNextY, _followMainTolerance, _followRowTolerance;

    internal async Task<string> PrepareFollowContinuityAsync()
    {
        RestoreFollowContinuity();
        await PrepareMotionAsync("text");
        _editor.TestSetMarkdownSplitWeights(1, 3);
        await MarkdownFeatureTests.CurrentAsync(_editor);
        await TabDragTestHost.FrameAsync();
        await TabDragTestHost.FrameAsync();
        await UntilAsync(() => _motionPreview.GetViewportAnchor().IsAvailable && _motionPreview.IsSurfaceReady,
            "The new split width has a current painted preview frame.");
        _editor.TestMarkdownPreview.FollowSource = false;
        _followPrepared = true;
        _followOracle.Clear(); _followInverse.Clear(); _followProbes.Clear();
        _motionPreview.ViewportChanged += FollowPublication;

        // A behavioral preferred-column oracle uses the existing long/empty/long
        // paragraph lines; it does not inspect Scintilla's private goal column.
        var core = _editor.TestNativeCore;
        var native = _source.Editor;
        core.TextWrapping = TextWrapping.NoWrap;
        await TabDragTestHost.FrameAsync();
        await UntilAsync(() => native.WrapCount(0) == 1 && _source.GetSourceViewport().IsAvailable,
            "The actual unwrapped source row is painted before measuring preferred-column preservation.");
        native.GotoPos(10); native.ChooseCaretX(); native.LineDown();
        var shortSelection = (native.Anchor, native.CurrentPos, native.SelectionRevision, native.XOffset);
        var preferredAccepted = _source.SetViewportBetweenSourcePositions(0, native.GetLineEndPosition(0), 0.5);
        var preferredPreserved = shortSelection == (native.Anchor, native.CurrentPos, native.SelectionRevision, native.XOffset);
        native.LineDown();
        _followProbes.Add(new("preferred-column", 10, native.PositionFromLine(2) + 10, native.CurrentPos,
            0, preferredAccepted, preferredPreserved));
        core.TextWrapping = TextWrapping.Wrap;
        await TabDragTestHost.FrameAsync();
        await UntilAsync(() => native.WrapCount(0) > 1 && _source.GetSourceViewport().IsAvailable,
            "The actual narrow source must wrap the fixed paragraph across multiple display rows.");
        native.SetSel(0, 0);

        // The oracle is measured from actual display rows through the old,
        // independent integer-row setter, not the new continuous setter.
        var oracleRows = (native.WrapCount(0) + 1) * 9;
        if (oracleRows > 64) throw new InvalidOperationException("The actual wrap geometry exceeds the finite row-oracle scope.");
        for (var row = 0; row < oracleRows; row++)
        {
            await AfterPaintAsync(native, () => _source.SetVerticalViewport(row, 0));
            var source = _source.GetSourceViewport();
            if (!source.IsAvailable || source.DisplayRow != row || source.Fraction != 0)
                throw new InvalidOperationException("The source row calibration did not actually paint its requested row.");
            var selection = _motionPreview.GetSelection();
            var mapped = await _motionPreview.MapSourceSpanAsync(selection.DocumentId, selection.Revision,
                selection.TextGeneration, checked((ulong)source.StartByte), checked((ulong)source.EndByte),
                MarkdownPositionAffinity.After).AsTask().WaitAsync(TimeSpan.FromSeconds(20));
            _followOracle.Add(new(source, mapped.Start.PreviewUtf16Offset, mapped.End.PreviewUtf16Offset));
        }

        _motionScroll.ChangeView(null, 0, null, true);
        await TabDragTestHost.FrameAsync();
        await TabDragTestHost.FrameAsync();
        var first = _motionText.DocumentRange.FindText("MMMM 0000", false, false);
        var second = _motionText.DocumentRange.FindText("MMMM 0001", false, false);
        first.GetBoundingRectangles(out var firstRect);
        second.GetBoundingRectangles(out var secondRect);
        if (firstRect.Length != 4 || secondRect.Length != 4 || secondRect[1] <= firstRect[1])
            throw new InvalidOperationException("The wide actual preview must show each fixed paragraph prefix on one line.");
        var main = _motionPreview.DisplayedText;
        var mainWidth = main.IndexOf("MMMM 0001", StringComparison.Ordinal) - main.IndexOf("MMMM 0000", StringComparison.Ordinal);
        _followMainTolerance = mainWidth / (secondRect[1] - firstRect[1]) + 0.000001;
        _followRowTolerance = 1.0 / native.TextHeight(0) + 0.000001;
        for (var paragraph = 0; paragraph < 8; paragraph++)
        {
            var start = (ulong)main.IndexOf("MMMM " + paragraph.ToString("D4", CultureInfo.InvariantCulture), StringComparison.Ordinal);
            var end = (ulong)main.IndexOf("MMMM " + (paragraph + 1).ToString("D4", CultureInfo.InvariantCulture), StringComparison.Ordinal);
            var selection = _motionPreview.GetSelection();
            var mapped = await _motionPreview.MapPreviewSpanAsync(selection.DocumentId, selection.Revision,
                selection.TextGeneration, start, end, MarkdownPositionAffinity.After).AsTask().WaitAsync(TimeSpan.FromSeconds(20));
            _followInverse.Add(new(start, end, FollowSourceCoordinate(mapped.Start.SourceByteOffset),
                FollowSourceCoordinate(mapped.End.SourceByteOffset)));
        }
        return FollowContinuityInfo();
    }

    internal async Task<string> ProbeFollowContinuityAsync()
    {
        RequireFollowContinuity();
        var native = _source.Editor;
        var identity = _motionPreview.GetSelection();
        var fixedSelection = (native.Anchor, native.CurrentPos, native.SelectionRevision, native.XOffset);
        var pair = _followOracle[0];
        var sourcePointStart = await _motionPreview.MapSourcePositionAsync(identity.DocumentId, identity.Revision,
            identity.TextGeneration, (ulong)pair.Source.StartByte, MarkdownPositionAffinity.After).AsTask();
        var sourcePointEnd = await _motionPreview.MapSourcePositionAsync(identity.DocumentId, identity.Revision,
            identity.TextGeneration, (ulong)pair.Source.EndByte, MarkdownPositionAffinity.After).AsTask();
        _followProbes.Add(new("source-batch-matches-points", 0, 0,
            pair.MainStart == sourcePointStart.PreviewUtf16Offset && pair.MainEnd == sourcePointEnd.PreviewUtf16Offset ? 0 : 1,
            0, true, true));
        foreach (var progress in new[] { 0.1, 0.3, 0.5, 0.7, 0.9, 0.7, 0.5, 0.3, 0.1 })
        {
            await AfterPaintAsync(native, () => _source.SetVerticalViewport(0,
                native.TextHeight(0) / _source.XamlRoot.RasterizationScale * progress));
            var source = _source.GetSourceViewport();
            var before = _followPublications;
            var anchor = _motionPreview.GetViewportAnchor();
            var admitted = _motionPreview.SetViewportBetweenPositions(identity.DocumentId, identity.Revision,
                identity.TextGeneration, anchor.LayoutGeneration, pair.MainStart, pair.MainEnd, source.Fraction);
            if (admitted) await WaitFollowPublicationAsync(before);
            anchor = _motionPreview.GetViewportAnchor();
            var expected = pair.MainStart + (pair.MainEnd - pair.MainStart) * source.Fraction;
            _followProbes.Add(new("source-fraction", progress, expected, FollowMainCoordinate(anchor), _followMainTolerance,
                admitted && source.DisplayRow == 0 && source.StartByte == pair.Source.StartByte && source.EndByte == pair.Source.EndByte,
                fixedSelection == (native.Anchor, native.CurrentPos, native.SelectionRevision, native.XOffset)));
        }
        var inverse = _followInverse[1];
        var mappedPair = await _motionPreview.MapPreviewSpanAsync(identity.DocumentId, identity.Revision,
            identity.TextGeneration, inverse.Start, inverse.End, MarkdownPositionAffinity.After).AsTask();
        var previewPointStart = await _motionPreview.MapPreviewPositionAsync(identity.DocumentId, identity.Revision,
            identity.TextGeneration, inverse.Start, MarkdownPositionAffinity.After).AsTask();
        var previewPointEnd = await _motionPreview.MapPreviewPositionAsync(identity.DocumentId, identity.Revision,
            identity.TextGeneration, inverse.End, MarkdownPositionAffinity.After).AsTask();
        _followProbes.Add(new("preview-batch-matches-points", 0, 0,
            mappedPair.Start.SourceByteOffset == previewPointStart.SourceByteOffset && mappedPair.End.SourceByteOffset == previewPointEnd.SourceByteOffset ? 0 : 1,
            0, true, true));
        foreach (var progress in new[] { 0.1, 0.3, 0.5, 0.7, 0.9, 0.7, 0.5, 0.3, 0.1 })
        {
            var before = _followPublications;
            var current = _motionPreview.GetViewportAnchor();
            var admitted = _motionPreview.SetViewportBetweenPositions(identity.DocumentId, identity.Revision,
                identity.TextGeneration, current.LayoutGeneration, inverse.Start, inverse.End, progress);
            if (admitted) await WaitFollowPublicationAsync(before);
            var anchor = _motionPreview.GetViewportAnchor();
            var accepted = _source.SetViewportBetweenSourcePositions((long)mappedPair.Start.SourceByteOffset,
                (long)mappedPair.End.SourceByteOffset, anchor.ScrollProgress);
            await TabDragTestHost.FrameAsync();
            await TabDragTestHost.FrameAsync();
            var actual = _source.GetSourceViewport();
            var expected = inverse.SourceStart + (inverse.SourceEnd - inverse.SourceStart) * anchor.ScrollProgress;
            _followProbes.Add(new("preview-fraction", progress, expected, actual.DisplayRow + actual.Fraction,
                _followRowTolerance, admitted && accepted && actual.IsAvailable && anchor.ScrollStartUtf16Offset == inverse.Start &&
                anchor.ScrollEndUtf16Offset == inverse.End,
                fixedSelection == (native.Anchor, native.CurrentPos, native.SelectionRevision, native.XOffset)));
        }
        return FollowContinuityInfo();
    }

    internal async Task<string> ConfigureFollowContinuityAsync()
    {
        RequireFollowContinuity();
        StopFollowContinuityTrace();
        _editor.TestMarkdownPreview.FollowSource = false;
        _motionPreview.CancelScrollAnimation(); _source.CancelScrollAnimation();
        _motionPreview.Focus(FocusState.Programmatic);
        var identity = _motionPreview.GetSelection();
        _motionPreview.SelectRange(identity.DocumentId, identity.Revision, identity.TextGeneration, 0, 0);
        _source.Editor.SetSel(0, 0);
        _motionScroll.ChangeView(null, 120, null, true);
        await TabDragTestHost.FrameAsync();
        await TabDragTestHost.FrameAsync();
        _editor.TestMarkdownPreview.FollowSource = true;
        // Startup mapping uses the normal finite coordinator; observe the real
        // source viewport before admitting a new external gesture.
        await Task.Delay(300);
        return FollowContinuityInfo();
    }

    internal string StartFollowContinuityTrace()
    {
        RequireFollowContinuity(); StopFollowContinuityTrace();
        _followFrames.Clear(); _followOverflow = false; _followTraceStart = Stopwatch.GetTimestamp();
        _followNextY = _motionScroll.VerticalOffset; _followNextQpc = 0;
        _followTracing = true;
        _motionScroll.ViewChanging += FollowChanging;
        CompositionTarget.Rendering += FollowRendering;
        return FollowContinuityInfo();
    }

    private void FollowChanging(object sender, ScrollViewerViewChangingEventArgs args)
    { _followNextQpc = Stopwatch.GetTimestamp(); _followNextY = args.NextView.VerticalOffset; }

    private void FollowRendering(object sender, object args)
    {
        if (!_followTracing) return;
        if (_followFrames.Count >= 512 || Stopwatch.GetElapsedTime(_followTraceStart).TotalSeconds > 6)
        { _followOverflow = true; StopFollowContinuityTrace(); return; }
        try
        {
            var source = _source.GetSourceViewport(); var anchor = _motionPreview.GetViewportAnchor();
            var forward = _followOracle.FirstOrDefault(item => item.Source.StartByte == source.StartByte && item.Source.EndByte == source.EndByte);
            var inverse = _followInverse.FirstOrDefault(item => item.Start == anchor.ScrollStartUtf16Offset && item.End == anchor.ScrollEndUtf16Offset);
            _followFrames.Add(new(Stopwatch.GetTimestamp(), source.IsAvailable && anchor.IsAvailable,
                source.DisplayRow, source.Fraction, source.StartByte, source.EndByte,
                anchor.ScrollStartUtf16Offset, anchor.ScrollEndUtf16Offset, anchor.ScrollProgress,
                _motionScroll.VerticalOffset, _followNextY, _followNextQpc,
                forward.Source.EndByte > forward.Source.StartByte ? forward.MainStart + (forward.MainEnd - forward.MainStart) * source.Fraction : double.NaN,
                inverse.End > inverse.Start ? inverse.SourceStart + (inverse.SourceEnd - inverse.SourceStart) * anchor.ScrollProgress : double.NaN,
                _source.Editor.CurrentPos, _source.Editor.Anchor, _source.Editor.SelectionRevision, ""));
        }
        catch (Exception error)
        { _followFrames.Add(new(Stopwatch.GetTimestamp(), false, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, double.NaN, double.NaN, 0, 0, 0, error.Message)); }
    }

    internal string FinishFollowContinuityTrace()
    {
        StopFollowContinuityTrace();
        return FollowContinuityInfo();
    }

    internal string FollowContinuityInfo()
    {
        RequireFollowContinuity();
        using var bytes = new MemoryStream();
        using (var json = new Utf8JsonWriter(bytes))
        {
            json.WriteStartObject(); json.WriteNumber("schema", 1); json.WriteNumber("frequency", Stopwatch.Frequency);
            json.WriteNumber("startQpc", _followTraceStart); json.WriteBoolean("overflow", _followOverflow);
            var currentSource = _source.GetSourceViewport(); var currentAnchor = _motionPreview.GetViewportAnchor();
            json.WriteString("document", currentAnchor.DocumentId.ToString()); json.WriteNumber("revision", currentAnchor.Revision);
            json.WriteNumber("textGeneration", currentAnchor.TextGeneration); json.WriteNumber("layoutGeneration", currentAnchor.LayoutGeneration);
            json.WriteNumber("sourceRow", currentSource.DisplayRow); Number(json, "sourceFraction", currentSource.Fraction);
            Number(json, "logicalY", _motionScroll.VerticalOffset);
            json.WriteBoolean("animationsEnabled", new Windows.UI.ViewManagement.UISettings().AnimationsEnabled);
            json.WriteNumber("sourceWraps", _source.Editor.WrapCount(0)); Number(json, "mainTolerance", _followMainTolerance);
            Number(json, "rowTolerance", _followRowTolerance); Number(json, "scale", _motionPreview.XamlRoot.RasterizationScale);
            WriteRect(json, "previewControl", Rect(_motionPreview)); WriteRect(json, "presenter", Rect(_motionViewport));
            WriteRect(json, "surface", Rect(_motionSurface)); WriteRect(json, "source", Rect(_source));
            var bar = Descendants(_motionScroll).OfType<ScrollBar>().Single(item => item.Name == "VerticalScrollBar");
            WriteRect(json, "bar", Rect(bar)); WriteRect(json, "thumb", ThumbRect(bar));
            json.WriteBoolean("barVisible", bar.Visibility == Visibility.Visible); json.WriteBoolean("barEnabled", bar.IsEnabled);
            Number(json, "barOpacity", bar.Opacity);
            json.WriteStartArray("oracleRows");
            foreach (var item in _followOracle)
            {
                json.WriteStartObject(); json.WriteNumber("row", item.Source.DisplayRow); json.WriteNumber("start", item.Source.StartByte);
                json.WriteNumber("end", item.Source.EndByte); json.WriteNumber("mainStart", item.MainStart); json.WriteNumber("mainEnd", item.MainEnd); json.WriteEndObject();
            }
            json.WriteEndArray(); json.WriteStartArray("probes");
            foreach (var item in _followProbes)
            {
                json.WriteStartObject(); json.WriteString("name", item.Name); Number(json, "requested", item.Requested);
                Number(json, "expected", item.Expected); Number(json, "actual", item.Actual); Number(json, "tolerance", item.Tolerance);
                json.WriteBoolean("admitted", item.Admitted); json.WriteBoolean("preserved", item.Preserved); json.WriteEndObject();
            }
            json.WriteEndArray(); json.WriteStartArray("frames");
            foreach (var frame in _followFrames)
            {
                json.WriteStartObject(); json.WriteNumber("qpc", frame.Qpc); json.WriteBoolean("available", frame.Available);
                json.WriteNumber("sourceRow", frame.Row); Number(json, "sourceFraction", frame.Fraction);
                json.WriteNumber("sourceStart", frame.StartByte); json.WriteNumber("sourceEnd", frame.EndByte);
                json.WriteNumber("mainStart", frame.MainStart); json.WriteNumber("mainEnd", frame.MainEnd); Number(json, "progress", frame.Progress);
                Number(json, "logicalY", frame.LogicalY); Number(json, "observedNextViewY", frame.NextY); json.WriteNumber("nextViewQpc", frame.NextQpc);
                Number(json, "oraclePreviewTargetMain", frame.OracleMain); Number(json, "oracleSourceTargetRow", frame.OracleSource);
                json.WriteNumber("caret", frame.Caret); json.WriteNumber("anchor", frame.SelectionAnchor);
                json.WriteNumber("selectionRevision", frame.SelectionRevision); json.WriteString("error", frame.Error); json.WriteEndObject();
            }
            json.WriteEndArray(); json.WriteEndObject(); json.Flush();
        }
        if (bytes.Length > 1024 * 1024) throw new InvalidOperationException("Continuity evidence exceeded 1MiB.");
        return "markdownFollowContinuity64=" + Convert.ToBase64String(bytes.ToArray()) + "\n";
    }

    internal void RestoreFollowContinuity()
    {
        StopFollowContinuityTrace();
        if (!_followPrepared) return;
        _followPrepared = false;
        _motionPreview.ViewportChanged -= FollowPublication;
        _editor.TestMarkdownPreview.FollowSource = false;
        _motionPreview.CancelScrollAnimation(); _source.CancelScrollAnimation();
    }

    private void StopFollowContinuityTrace()
    {
        if (!_followTracing) return;
        _followTracing = false; CompositionTarget.Rendering -= FollowRendering;
        _motionScroll.ViewChanging -= FollowChanging;
    }
    private void FollowPublication(MarkdownView sender, MarkdownViewportAnchor args) => ++_followPublications;
    private async Task WaitFollowPublicationAsync(long before)
    {
        await UntilAsync(() => _followPublications > before && _motionPreview.GetViewportAnchor().IsAvailable && _motionPreview.IsSurfaceReady,
            "The requested real continuous viewport publishes a new frame.");
    }
    private static double FollowMainCoordinate(MarkdownViewportAnchor anchor) =>
        anchor.ScrollStartUtf16Offset + (anchor.ScrollEndUtf16Offset - anchor.ScrollStartUtf16Offset) * anchor.ScrollProgress;
    private double FollowSourceCoordinate(ulong offset)
    {
        foreach (var item in _followOracle)
            if ((ulong)item.Source.StartByte <= offset && offset < (ulong)item.Source.EndByte)
                return item.Source.DisplayRow + (offset - (ulong)item.Source.StartByte) / (double)(item.Source.EndByte - item.Source.StartByte);
        if (_followOracle.Count > 0 && offset == (ulong)_followOracle[^1].Source.EndByte)
            return _followOracle[^1].Source.DisplayRow + 1;
        throw new InvalidOperationException("A mapped endpoint lies outside the independently painted source row oracle.");
    }
    private void RequireFollowContinuity()
    {
        if (!_followPrepared || !_editor.TestMarkdownPreview.SourceSession.IsCurrent)
            throw new InvalidOperationException("Prepare the current owned continuity fixture first.");
    }
}
