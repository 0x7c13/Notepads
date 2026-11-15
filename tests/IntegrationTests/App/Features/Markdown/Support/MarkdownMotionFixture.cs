// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Presentation.Theming;
using Windows.Graphics.Imaging;
using Windows.Storage;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Media;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal sealed partial class MarkdownKeyboardFixture
{
    private sealed class MotionRow
    {
        internal long Begin, End, NextAt, ViewAt, SourceByte = -1, SourceFirst, SourceLine, SourceSubline;
        internal ulong Revision, Text, Layout, Offset, SelectionRevision;
        internal ulong AcceptedRevision, AcceptedNativeRevision, SourceRevision;
        internal long AcceptedLength, AcceptedContentVersion, AcceptedGeneration;
        internal string Document = "", SessionDocument = "", SourceDocument = "";
        internal long Caret, Anchor;
        internal double Within, Desired, Next, SourceY, Scale, SourceCaretY, SourceHeight;
        internal double[] Viewport, Surface, Glyph;
        internal bool Current, Available, Intermediate, Follow, PreviewFocus, SourceFocus, SourceKnown, HasSource, NeedsResync;
        internal string Error = "";
    }
    private readonly List<MotionRow> _motionRows = new();
    private ScrollViewer _motionScroll;
    private FrameworkElement _motionViewport, _motionSurface;
    private MarkdownView _motionPreview;
    private ITextProvider _motionText;
    private string _motionMode, _motionRun, _motionSourceSha, _motionPngSha;
    private long _motionStart, _motionNextAt, _motionViewAt;
    private int _motionLimitMs;
    private bool _motionTracing, _motionOverflow, _motionIntermediate;
    private double _motionNext;
    private uint _motionDecodedWidth, _motionDecodedHeight;

    internal async Task<string> PrepareMotionAsync(string mode)
    {
        if (mode is not ("marker" or "text")) throw new ArgumentException("Motion fixture must be marker or text.");
        StopMotionTrace();
        if (_editor.TestMarkdownPanelOpen)
        {
            var previous = _editor.TestMarkdownPreview;
            _editor.ShowHideContentPreview();
            await previous.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
        }
        TestClipboard.UseSystemClipboard = false; // This probe never reads or writes the OS clipboard.
        ThemeSettingsService.SetTheme(ElementTheme.Light);
        _motionMode = mode;
        var marker = CreateMotionPng();
        _motionPngSha = Convert.ToHexString(SHA256.HashData(marker));
        var file = await _host.Folder.CreateFileAsync("motion-rows.png", CreationCollisionOption.ReplaceExisting);
        await FileIO.WriteBytesAsync(file, marker);
        using (var stream = await file.OpenReadAsync())
        {
            var decoder = await BitmapDecoder.CreateAsync(stream);
            _motionDecodedWidth = decoder.PixelWidth;
            _motionDecodedHeight = decoder.PixelHeight;
        }
        var text = mode == "marker" ? "![motion rows](motion-rows.png)" :
            string.Join("\r\r", Enumerable.Range(0, 220).Select(index =>
                "MMMM " + index.ToString("D4", CultureInfo.InvariantCulture) + " 0123456789 viewport follow paragraph."));
        var utf8 = Encoding.UTF8.GetBytes(text);
        _motionSourceSha = Convert.ToHexString(SHA256.HashData(utf8));
        await FileIO.WriteBytesAsync(_editor.EditingFile, utf8);
        await _editor.ReloadFromEditingFileAsync();
        var core = _editor.TestNativeCore;
        core.SetFontZoomFactor(100);
        core.TextWrapping = TextWrapping.Wrap;
        _editor.ShowHideContentPreview();
        var extension = _editor.TestMarkdownPreview;
        extension.FollowSource = false;
        await MarkdownFeatureTests.CurrentAsync(_editor);
        _motionPreview = extension.PreviewView;
        _motionScroll = Descendants(_motionPreview).OfType<ScrollViewer>().Single(item => item.Name == "PreviewScrollViewer");
        _motionViewport = Descendants(_motionScroll).OfType<ScrollContentPresenter>().Single();
        _motionSurface = Descendants(_motionPreview).OfType<FrameworkElement>().Single(item => item.Name == "SurfaceTarget");
        var peer = FrameworkElementAutomationPeer.FromElement(_motionPreview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(_motionPreview);
        _motionText = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        if (mode == "marker")
            await UntilAsync(() => peer.GetChildren()?.Any(item => item.GetAutomationControlType() == AutomationControlType.Image &&
                item.GetName().StartsWith("motion rows, image loaded", StringComparison.Ordinal)) == true,
                "The real same-surface motion marker is decoded by the product.");
        _motionPreview.Focus(FocusState.Programmatic);
        var selection = _motionPreview.GetSelection();
        _motionPreview.SelectRange(selection.DocumentId, selection.Revision, selection.TextGeneration,
            _motionPreview.DisplayedTextLength, 0);
        _source.Editor.SetEmptySelection(_source.Editor.Length);
        _source.Editor.FirstVisibleLine = 0;
        await TabDragTestHost.FrameAsync();
        return MotionInfo();
    }

    internal string MotionInfo()
    {
        using var stream = new MemoryStream();
        using (var json = new Utf8JsonWriter(stream))
        {
            json.WriteStartObject(); json.WriteNumber("schema", 1);
            json.WriteString("method", "viewport-motion-v3");
            json.WriteString("mode", _motionMode); json.WriteString("sourceSha256", _motionSourceSha);
            json.WriteString("markerSha256", _motionPngSha);
            json.WriteNumber("intrinsicWidth", 32); json.WriteNumber("intrinsicHeight", 2048);
            json.WriteNumber("productDecodeMaximumSide", 2048);
            json.WriteNumber("platformDecoderWidth", _motionDecodedWidth); json.WriteNumber("platformDecoderHeight", _motionDecodedHeight);
            json.WriteString("decodedFactScope", "Platform BitmapDecoder on the identical PNG stream; Mark private decoded buffer is not a public getter. Native display ratio requires independent screen row calibration.");
            json.WriteNull("markPrivateDecodedWidth"); json.WriteNull("markPrivateDecodedHeight");
            json.WriteNumber("sourceRectangleWidth", 32); json.WriteNumber("sourceRectangleHeight", 2048);
            json.WriteNumber("naturalWidthDip", 32); json.WriteNumber("naturalHeightDip", 2048);
            Number(json, "scale", _source.XamlRoot.RasterizationScale);
            WriteRect(json, "viewport", Rect(_motionViewport));
            WriteRect(json, "surface", Rect(_motionSurface));
            WriteRect(json, "source", Rect(_source));
            var previewBar = Descendants(_motionScroll).OfType<ScrollBar>().FirstOrDefault(bar => bar.Orientation == Orientation.Vertical && bar.ActualWidth > 0);
            var sourceBar = Descendants(_source).OfType<ScrollBar>().FirstOrDefault(bar => bar.Orientation == Orientation.Vertical && bar.ActualWidth > 0);
            WriteRect(json, "previewThumb", ThumbRect(previewBar)); WriteRect(json, "sourceThumb", ThumbRect(sourceBar));
            Number(json, "desiredY", _motionScroll?.VerticalOffset ?? 0);
            Number(json, "scrollableHeight", _motionScroll?.ScrollableHeight ?? 0);
            json.WriteEndObject(); json.Flush();
        }
        return "markdownMotionInfo64=" + Convert.ToBase64String(stream.ToArray()) + "\n" + Snapshot();
    }

    internal string StartMotionTrace(string run, int maximumMilliseconds)
    {
        if (!Guid.TryParseExact(run, "N", out _) || maximumMilliseconds < 500 || maximumMilliseconds > 6000)
            throw new ArgumentException("Motion trace requires a run GUID and a 500–6000ms bound.");
        StopMotionTrace();
        if (_motionPreview == null || !_editor.TestMarkdownPreview.SourceSession.IsCurrent)
            throw new InvalidOperationException("Prepare a current motion fixture first.");
        _motionRows.Clear(); _motionOverflow = false; _motionRun = run; _motionLimitMs = maximumMilliseconds;
        _motionNextAt = _motionViewAt = 0; _motionNext = _motionScroll.VerticalOffset;
        _motionStart = Stopwatch.GetTimestamp(); _motionTracing = true;
        _motionScroll.ViewChanging += MotionViewChanging; _motionScroll.ViewChanged += MotionViewChanged;
        CompositionTarget.Rendering += MotionRendering;
        return "markdownMotionStarted=" + run + "\nmarkdownMotionQpc=" + _motionStart + "\nmarkdownMotionFrequency=" + Stopwatch.Frequency + "\n";
    }

    private void MotionViewChanging(object sender, ScrollViewerViewChangingEventArgs args)
    { _motionNext = args.NextView.VerticalOffset; _motionNextAt = Stopwatch.GetTimestamp(); }
    private void MotionViewChanged(object sender, ScrollViewerViewChangedEventArgs args)
    { _motionIntermediate = args.IsIntermediate; _motionViewAt = Stopwatch.GetTimestamp(); }
    private void MotionRendering(object sender, object args)
    {
        if (!_motionTracing) return;
        if (_motionRows.Count >= 1024) { _motionOverflow = true; StopMotionTrace(); return; }
        if (Stopwatch.GetElapsedTime(_motionStart).TotalMilliseconds > _motionLimitMs) { StopMotionTrace(); return; }
        var row = new MotionRow { Begin = Stopwatch.GetTimestamp() };
        try
        {
            var session = _editor.TestMarkdownPreview.SourceSession;
            var native = _source.Editor;
            var source = _motionPreview.GetSourceState();
            var anchor = _motionPreview.GetViewportAnchor();
            row.Viewport = Rect(_motionViewport); row.Surface = Rect(_motionSurface);
            row.Scale = _source.XamlRoot.RasterizationScale;
            row.Desired = _motionScroll.VerticalOffset; row.Next = _motionNext; row.NextAt = _motionNextAt;
            row.ViewAt = _motionViewAt; row.Intermediate = _motionIntermediate;
            row.Available = anchor.IsAvailable; row.Revision = anchor.Revision; row.Text = anchor.TextGeneration;
            row.Document = anchor.DocumentId.ToString(); row.AcceptedRevision = session.AcceptedRevision;
            row.SessionDocument = session.DocumentId.ToString(); row.SourceDocument = source.DocumentId.ToString();
            row.SourceRevision = source.Revision; row.SourceKnown = source.StateKnown;
            row.HasSource = source.HasSource; row.NeedsResync = source.NeedsResync;
            row.AcceptedNativeRevision = session.AcceptedStamp.NativeRevision; row.AcceptedLength = session.AcceptedStamp.Length;
            row.AcceptedContentVersion = session.AcceptedStamp.ContentVersion; row.AcceptedGeneration = session.AcceptedStamp.Generation;
            row.Layout = anchor.LayoutGeneration; row.Offset = anchor.PreviewUtf16Offset; row.Within = anchor.OffsetWithinLine;
            row.Current = session.IsCurrent; row.Follow = _editor.TestMarkdownPreview.FollowSource;
            row.SourceFirst = native.FirstVisibleLine; row.SourceLine = native.DocLineFromVisible(row.SourceFirst);
            row.SourceSubline = row.SourceFirst - native.VisibleFromDocLine(row.SourceLine);
            row.SourceY = _source.DisplayVerticalOffset; row.Caret = native.CurrentPos; row.Anchor = native.Anchor;
            row.SourceCaretY = native.PointYFromPosition(native.CurrentPos); row.SourceHeight = _source.ActualHeight * row.Scale;
            row.SelectionRevision = native.SelectionRevision;
            row.PreviewFocus = _motionPreview.FocusState != FocusState.Unfocused; row.SourceFocus = _source.FocusState != FocusState.Unfocused;
            if (!_source.IsViewportRestorePending) row.SourceByte = _source.ViewportSourceByte;
            if (_motionMode is "text" or "mixed")
            {
                var visible = _motionText.GetVisibleRanges();
                if (visible.Length > 0)
                {
                    visible[0].GetBoundingRectangles(out var rectangles);
                    if (rectangles.Length >= 4) row.Glyph = rectangles.Take(4).ToArray();
                }
            }
        }
        catch (Exception error) { row.Error = error.GetType().Name + ": " + error.Message; }
        row.End = Stopwatch.GetTimestamp(); _motionRows.Add(row);
    }

    private void StopMotionTrace()
    {
        if (!_motionTracing) return;
        _motionTracing = false;
        CompositionTarget.Rendering -= MotionRendering;
        _motionScroll.ViewChanging -= MotionViewChanging; _motionScroll.ViewChanged -= MotionViewChanged;
    }

    internal string FinishMotionTrace()
    {
        StopMotionTrace();
        using var stream = new MemoryStream();
        using (var json = new Utf8JsonWriter(stream))
        {
            json.WriteStartObject(); json.WriteNumber("schema", 1); json.WriteString("run", _motionRun);
            json.WriteString("method", "viewport-motion-v3");
            json.WriteString("mode", _motionMode); json.WriteNumber("frequency", Stopwatch.Frequency);
            json.WriteNumber("startQpc", _motionStart); json.WriteBoolean("overflow", _motionOverflow);
            json.WriteStartArray("frames");
            foreach (var row in _motionRows)
            {
                json.WriteStartObject(); json.WriteNumber("qpc", row.Begin); json.WriteNumber("endQpc", row.End);
                WriteRect(json, "viewport", row.Viewport); WriteRect(json, "surface", row.Surface); WriteRect(json, "glyphScreen", row.Glyph);
                Number(json, "scale", row.Scale); Number(json, "desiredY", row.Desired); Number(json, "nextY", row.Next);
                json.WriteNumber("nextQpc", row.NextAt); json.WriteNumber("viewQpc", row.ViewAt); json.WriteBoolean("intermediate", row.Intermediate);
                json.WriteBoolean("anchorAvailable", row.Available); json.WriteNumber("revision", row.Revision);
                json.WriteString("document", row.Document); json.WriteNumber("acceptedRevision", row.AcceptedRevision);
                json.WriteString("sessionDocument", row.SessionDocument); json.WriteString("sourceDocument", row.SourceDocument);
                json.WriteNumber("sourceRevision", row.SourceRevision); json.WriteBoolean("sourceKnown", row.SourceKnown);
                json.WriteBoolean("hasSource", row.HasSource); json.WriteBoolean("needsResync", row.NeedsResync);
                json.WriteNumber("acceptedNativeRevision", row.AcceptedNativeRevision); json.WriteNumber("acceptedLength", row.AcceptedLength);
                json.WriteNumber("acceptedContentVersion", row.AcceptedContentVersion); json.WriteNumber("acceptedGeneration", row.AcceptedGeneration);
                json.WriteNumber("textGeneration", row.Text); json.WriteNumber("layoutGeneration", row.Layout);
                json.WriteNumber("mainOffset", row.Offset); Number(json, "offsetWithinLine", row.Within);
                json.WriteBoolean("current", row.Current); json.WriteBoolean("follow", row.Follow);
                json.WriteNumber("sourceByte", row.SourceByte); json.WriteNumber("sourceFirst", row.SourceFirst);
                json.WriteNumber("sourceLine", row.SourceLine); json.WriteNumber("sourceSubline", row.SourceSubline);
                Number(json, "sourceY", row.SourceY); json.WriteNumber("caret", row.Caret); json.WriteNumber("anchor", row.Anchor);
                Number(json, "sourceCaretY", row.SourceCaretY); Number(json, "sourceHeight", row.SourceHeight);
                json.WriteNumber("selectionRevision", row.SelectionRevision); json.WriteBoolean("previewFocus", row.PreviewFocus);
                json.WriteBoolean("sourceFocus", row.SourceFocus); json.WriteString("error", row.Error);
                json.WriteEndObject();
            }
            json.WriteEndArray(); json.WriteEndObject(); json.Flush();
        }
        if (stream.Length > 2 * 1024 * 1024) throw new InvalidOperationException("Motion UI trace exceeded its 2MiB evidence bound.");
        return "markdownMotionTrace64=" + Convert.ToBase64String(stream.ToArray()) + "\n";
    }

    private static double[] ThumbRect(ScrollBar bar)
    {
        if (bar == null) return null;
        var thumb = Descendants(bar).OfType<Thumb>().Where(item => item.IsLoaded && item.ActualWidth > 0 && item.ActualHeight > 0)
            .OrderByDescending(item => item.ActualHeight).FirstOrDefault();
        return Rect(thumb ?? (FrameworkElement)bar);
    }
    private static double[] Rect(FrameworkElement element)
    {
        if (element == null) return null;
        var origin = element.TransformToVisual(null).TransformPoint(default);
        var transform = element.TransformToVisual(null);
        var unitX = transform.TransformPoint(new Windows.Foundation.Point(1, 0));
        var unitY = transform.TransformPoint(new Windows.Foundation.Point(0, 1));
        var brush = (element as Border)?.Background as ImageBrush;
        var relative = brush?.RelativeTransform?.TransformPoint(default) ?? default;
        return new[] { origin.X, origin.Y, element.ActualWidth, element.ActualHeight,
            element is Border ? Canvas.GetTop(element) : 0, element is Border ? Canvas.GetLeft(element) : 0,
            unitX.X - origin.X, unitX.Y - origin.Y, unitY.X - origin.X, unitY.Y - origin.Y, relative.X, relative.Y };
    }
    private static void WriteRect(Utf8JsonWriter json, string name, double[] values)
    {
        if (values == null) { json.WriteNull(name); return; }
        json.WriteStartArray(name);
        foreach (var value in values) if (double.IsFinite(value)) json.WriteNumberValue(value); else json.WriteNullValue();
        json.WriteEndArray();
    }

    private static byte[] CreateMotionPng()
    {
        using var image = new MemoryStream();
        image.Write(new byte[] { 137, 80, 78, 71, 13, 10, 26, 10 });
        var header = new byte[13]; Put32(header, 0, 32); Put32(header, 4, 2048); header[8] = 8; header[9] = 6;
        Chunk(image, "IHDR", header);
        using var compressed = new MemoryStream();
        using (var zip = new ZLibStream(compressed, CompressionLevel.SmallestSize, true))
        {
            var row = new byte[1 + 32 * 4];
            for (var y = 0; y < 2048; y++)
            {
                var gray = y ^ (y >> 1);
                for (var x = 0; x < 32; x++)
                {
                    var offset = 1 + x * 4; row[offset + 3] = 255;
                    if (x < 4) { row[offset] = 0; row[offset + 1] = 128; row[offset + 2] = 255; }
                    else if (x >= 28) { row[offset] = 255; row[offset + 1] = 192; row[offset + 2] = 0; }
                    else
                    {
                        var bit = (gray >> ((x - 4) / 2)) & 1;
                        row[offset] = row[offset + 2] = (byte)(bit == 1 ? 255 : 0); row[offset + 1] = (byte)(bit == 1 ? 0 : 255);
                    }
                }
                zip.Write(row);
            }
        }
        Chunk(image, "IDAT", compressed.ToArray()); Chunk(image, "IEND", Array.Empty<byte>());
        return image.ToArray();
    }
    private static void Put32(byte[] bytes, int at, uint value)
    { bytes[at] = (byte)(value >> 24); bytes[at + 1] = (byte)(value >> 16); bytes[at + 2] = (byte)(value >> 8); bytes[at + 3] = (byte)value; }
    private static void Chunk(Stream destination, string name, byte[] payload)
    {
        var type = Encoding.ASCII.GetBytes(name); var size = new byte[4]; Put32(size, 0, (uint)payload.Length);
        destination.Write(size); destination.Write(type); destination.Write(payload);
        uint crc = 0xffffffff;
        foreach (var value in type.Concat(payload))
        {
            crc ^= value;
            for (var bit = 0; bit < 8; bit++) crc = (crc >> 1) ^ ((crc & 1) == 1 ? 0xedb88320u : 0);
        }
        Put32(size, 0, crc ^ 0xffffffff); destination.Write(size);
    }
}
