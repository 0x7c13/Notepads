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
using System.Text.Json;
using System.Threading.Tasks;
using Notepads.Markdown;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

// Input is exclusively supplied by the guarded external SendInput driver.
// Reuses the existing motion trace: it observes actual offsets and published
// anchors without adding a second animation clock or a product test hook.
internal sealed partial class MarkdownKeyboardFixture
{
    private readonly record struct SmoothProviderRead(long Qpc, double Offset, uint Lines);
    private readonly record struct SmoothWheelObservation(long Qpc, int Delta, bool Handled);

    [WinRT.GeneratedWinRTExposedType]
    private sealed partial class SmoothSettings(MarkdownKeyboardFixture owner) : IMarkdownScrollSettingsProvider
    {
        public uint VerticalWheelScrollLines
        {
            get
            {
                if (owner._smoothReads.Count >= 32)
                    throw new InvalidOperationException("Smooth fixture provider read budget exhausted.");
                owner._smoothReads.Add(new(Stopwatch.GetTimestamp(), owner._motionScroll.VerticalOffset, owner._smoothLines));
                var action = owner._smoothProviderAction;
                owner._smoothProviderAction = "none";
                if (action == "cancel") owner._motionPreview.CancelScrollAnimation();
                else if (action == "resource-throw")
                {
                    owner._smoothResourceTouched = true;
                    owner._smoothResourceGeneration = owner._editor.TestMarkdownPreview.TestInvalidateSmoothImageResources();
                    throw new InvalidOperationException("Intentional provider failure after real resource invalidation.");
                }
                else if (action == "reveal")
                {
                    var selection = owner._motionPreview.GetSelection();
                    owner._motionPreview.RevealRange(selection.DocumentId, selection.Revision,
                        selection.TextGeneration, 0, 0, true, false);
                }
                return owner._smoothLines;
            }
        }
    }

    [WinRT.GeneratedWinRTExposedType]
    private sealed partial class SmoothHostSettings : IMarkdownScrollSettingsProvider
    {
        public uint VerticalWheelScrollLines => WinUIEditor.EditorBaseControl.VerticalWheelScrollLines;
    }

    private readonly List<SmoothProviderRead> _smoothReads = new();
    private readonly List<SmoothWheelObservation> _smoothWheels = new();
    private uint _smoothLines;
    private string _smoothProviderAction = "none";
    private Canvas _smoothCanvas;
    private PointerEventHandler _smoothObserver;
    private bool _smoothPrepared, _smoothOriginalEnabled, _smoothOriginalFollow;
    private long _smoothActionQpc;
    private double _smoothActionOffset;
    private string _smoothAction = "none";
    private bool _smoothResourceTouched;
    private ulong _smoothResourceGeneration;

    internal async Task<string> PrepareSmoothAsync()
    {
        RestoreSmooth();
        await PrepareMotionAsync("text");
        _smoothOriginalEnabled = _motionPreview.IsSmoothScrollingEnabled;
        _smoothOriginalFollow = _editor.TestMarkdownPreview.FollowSource;
        _smoothCanvas = Descendants(_motionPreview).OfType<Canvas>().Single(item => item.Name == "DocumentExtent");
        _smoothObserver = (_, args) =>
        {
            if (_smoothWheels.Count >= 32) throw new InvalidOperationException("Smooth fixture wheel observation budget exhausted.");
            _smoothWheels.Add(new(Stopwatch.GetTimestamp(), args.GetCurrentPoint(_smoothCanvas).Properties.MouseWheelDelta,
                args.Handled));
        };
        _smoothCanvas.AddHandler(UIElement.PointerWheelChangedEvent, _smoothObserver, true);
        _smoothPrepared = true;
        return await ConfigureSmoothAsync(3, true, false, "none", 200);
    }

    internal async Task<string> ConfigureSmoothAsync(uint lines, bool smooth, bool follow, string providerAction, double offset)
    {
        RequireSmooth();
        if (providerAction is not ("none" or "cancel" or "reveal" or "resource-throw") || !double.IsFinite(offset) || offset < 0)
            throw new ArgumentException("Invalid smooth test configuration.");
        StopMotionTrace();
        _editor.TestMarkdownPreview.FollowSource = false;
        _motionPreview.CancelScrollAnimation();
        _source.CancelScrollAnimation();
        _motionPreview.IsSmoothScrollingEnabled = smooth;
        _smoothLines = lines;
        _smoothProviderAction = "none";
        _motionPreview.SetScrollSettingsProvider(new SmoothSettings(this));
        var target = Math.Clamp(offset, 0, _motionScroll.ScrollableHeight);
        _motionScroll.ChangeView(null, target, null, true);
        await UntilAsync(() => Math.Abs(_motionScroll.VerticalOffset - target) < 0.01,
            "The actual ScrollViewer settles at the explicit smooth fixture origin.");
        await TabDragTestHost.FrameAsync();
        await TabDragTestHost.FrameAsync();
        _motionPreview.Focus(FocusState.Programmatic);
        _editor.TestMarkdownPreview.FollowSource = follow;
        _smoothReads.Clear();
        _smoothWheels.Clear();
        _smoothProviderAction = providerAction;
        _smoothAction = "none";
        _smoothActionQpc = 0;
        _smoothActionOffset = _motionScroll.VerticalOffset;
        return SmoothSnapshot();
    }

    internal string SmoothAction(string action)
    {
        RequireSmooth();
        _smoothAction = action;
        _smoothActionQpc = Stopwatch.GetTimestamp();
        _smoothActionOffset = _motionScroll.VerticalOffset;
        switch (action)
        {
            case "cancel": _motionPreview.CancelScrollAnimation(); break;
            case "reveal":
                var selection = _motionPreview.GetSelection();
                _motionPreview.RevealRange(selection.DocumentId, selection.Revision, selection.TextGeneration, 0, 0, true, false);
                break;
            case "source-focus": _source.Focus(FocusState.Programmatic); break;
            case "follow": _editor.TestMarkdownPreview.FollowSource = true; break;
            default: throw new ArgumentException("Unknown smooth fixture action.");
        }
        return SmoothSnapshot();
    }

    internal string SmoothSnapshot()
    {
        RequireSmooth();
        var native = _source.Editor;
        var anchor = _motionPreview.GetViewportAnchor();
        using var stream = new MemoryStream();
        using (var json = new Utf8JsonWriter(stream))
        {
            json.WriteStartObject();
            json.WriteNumber("schema", 1);
            json.WriteNumber("qpc", Stopwatch.GetTimestamp());
            json.WriteNumber("frequency", Stopwatch.Frequency);
            json.WriteBoolean("animationsEnabled", new UISettings().AnimationsEnabled);
            json.WriteBoolean("smooth", _motionPreview.IsSmoothScrollingEnabled);
            json.WriteBoolean("follow", _editor.TestMarkdownPreview.FollowSource);
            json.WriteNumber("lines", _smoothLines);
            Number(json, "offset", _motionScroll.VerticalOffset);
            Number(json, "maximum", _motionScroll.ScrollableHeight);
            Number(json, "viewportHeight", _motionScroll.ViewportHeight);
            Number(json, "scale", _motionPreview.XamlRoot.RasterizationScale);
            json.WriteBoolean("anchorAvailable", anchor.IsAvailable);
            json.WriteBoolean("surfaceReady", _motionPreview.IsSurfaceReady);
            json.WriteBoolean("resourceTouched", _smoothResourceTouched);
            json.WriteNumber("resourceGeneration", _smoothResourceGeneration);
            json.WriteNumber("mainOffset", anchor.PreviewUtf16Offset);
            Number(json, "within", anchor.OffsetWithinLine);
            json.WriteNumber("sourceCaret", native.CurrentPos);
            json.WriteNumber("sourceAnchor", native.Anchor);
            json.WriteNumber("sourceRevision", native.DocumentRevision);
            json.WriteNumber("sourceSelectionRevision", native.SelectionRevision);
            json.WriteNumber("sourceFirstVisibleRow", native.FirstVisibleLine);
            json.WriteNumber("sourceParagraph", SmoothParagraph(_editor.GetText(), _source.ViewportSourceByte));
            json.WriteNumber("previewParagraph", SmoothParagraph(_motionPreview.DisplayedText, checked((long)anchor.PreviewUtf16Offset)));
            json.WriteBoolean("sourceFocus", _source.FocusState != FocusState.Unfocused);
            json.WriteBoolean("previewFocus", _motionPreview.FocusState != FocusState.Unfocused);
            json.WriteString("sourceSha256", _motionSourceSha);
            WriteRect(json, "viewport", Rect(_motionViewport));
            WriteRect(json, "source", Rect(_source));
            json.WriteString("action", _smoothAction);
            json.WriteNumber("actionQpc", _smoothActionQpc);
            Number(json, "actionOffset", _smoothActionOffset);
            json.WriteStartArray("providerReads");
            foreach (var read in _smoothReads)
            {
                json.WriteStartObject(); json.WriteNumber("qpc", read.Qpc); Number(json, "offset", read.Offset);
                json.WriteNumber("lines", read.Lines); json.WriteEndObject();
            }
            json.WriteEndArray();
            json.WriteStartArray("wheelObservations");
            foreach (var wheel in _smoothWheels)
            {
                json.WriteStartObject(); json.WriteNumber("qpc", wheel.Qpc); json.WriteNumber("delta", wheel.Delta);
                json.WriteBoolean("handled", wheel.Handled); json.WriteEndObject();
            }
            json.WriteEndArray(); json.WriteEndObject(); json.Flush();
        }
        return "markdownSmooth64=" + Convert.ToBase64String(stream.ToArray()) + "\n";
    }

    internal void RestoreSmooth()
    {
        if (!_smoothPrepared) return;
        _smoothPrepared = false;
        StopMotionTrace();
        if (_smoothCanvas != null && _smoothObserver != null)
            _smoothCanvas.RemoveHandler(UIElement.PointerWheelChangedEvent, _smoothObserver);
        _smoothCanvas = null; _smoothObserver = null;
        _motionPreview.CancelScrollAnimation();
        _motionPreview.SetScrollSettingsProvider(new SmoothHostSettings());
        _motionPreview.IsSmoothScrollingEnabled = _smoothOriginalEnabled;
        if (_smoothResourceTouched)
        {
            _editor.TestMarkdownPreview.TestRestoreSmoothImageResources();
            _smoothResourceTouched = false;
            _smoothResourceGeneration = 0;
        }
        _editor.TestMarkdownPreview.FollowSource = _smoothOriginalFollow;
        _smoothReads.Clear(); _smoothWheels.Clear();
    }

    private void RequireSmooth()
    {
        if (!_smoothPrepared || _motionPreview == null || !_editor.TestMarkdownPreview.SourceSession.IsCurrent)
            throw new InvalidOperationException("Prepare the current owned smooth fixture first.");
    }

    private static int SmoothParagraph(string text, long offset)
    {
        if (text.Length == 0 || offset < 0 || offset > text.Length) return -1;
        // LastIndexOf's startIndex bounds the final character of a candidate;
        // include the complete five-character prefix whose start is offset.
        var start = text.LastIndexOf("MMMM ", (int)Math.Min(offset + 4, text.Length - 1), StringComparison.Ordinal);
        return start >= 0 && start + 9 <= text.Length && int.TryParse(text.AsSpan(start + 5, 4),
            NumberStyles.None, CultureInfo.InvariantCulture, out var paragraph) ? paragraph : -1;
    }
}
