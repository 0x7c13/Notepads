// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Presentation.Controls.TextEditor;

namespace Notepads.Presentation.Controls.Markdown;

internal enum MarkdownFollowPane { Source, Preview }
internal enum MarkdownFollowKind { Viewport, Caret }
internal enum MarkdownFollowOperation { Caret, SourceViewport, PreviewViewport }

internal readonly record struct MarkdownFollowPoint(long Row, double Within) : IComparable<MarkdownFollowPoint>
{
    public int CompareTo(MarkdownFollowPoint other)
    {
        int row = Row.CompareTo(other.Row);
        return row == 0 ? Within.CompareTo(other.Within) : row;
    }
}

internal readonly record struct MarkdownFollowPair(long StartByte, long EndByte, ulong Start, ulong End)
{
    internal bool Matches(MarkdownViewportSample sample) => StartByte == sample.StartByte && EndByte == sample.EndByte;
}

internal readonly record struct MarkdownPreviewViewport(ulong Start, ulong End, double Progress, ulong Layout);

internal readonly record struct MarkdownPreviewFollowPair(ulong Start, ulong End, long StartByte, long EndByte, ulong Layout)
{
    internal bool Matches(MarkdownPreviewViewport sample) => Start == sample.Start && End == sample.End && Layout == sample.Layout;
}

internal readonly record struct MarkdownFollowDispatch(long Epoch, long Geometry, long ScrollSerial, long ViewportSerial, MarkdownFollowPane Pane);

internal readonly record struct MarkdownFollowWork(
    long Id, long Epoch, long Geometry, long ScrollSerial, long CaretSerial,
    MarkdownFollowPane Pane, MarkdownFollowOperation Operation, ulong Offset, ulong Length,
    MarkdownFollowPoint Position, MarkdownViewportSample Source, MarkdownSearchMatch? SearchMatch = null,
    ulong EndOffset = 0, ulong TargetLayout = 0, ulong? SourceSelectionRevision = null);

/// <summary>
/// One mapping admission, one latest viewport, and one selection debt. Only the
/// current scroll intent can reveal; selection synchronization has no other scroll authority.
/// The UI boundary owns strong document identities, cancellation and actual admission.
/// </summary>
internal sealed class MarkdownFollowCoordinator
{
    private long _nextWork, _viewportSerial;
    private long _caretSerial;
    private ulong _caretOffset, _caretLength;
    private ulong? _caretSourceSelectionRevision;
    private MarkdownSearchMatch? _searchMatch;
    private bool _caretDirty, _viewportDirty, _preferCaret = true;
    private bool _hasViewport;
    private MarkdownViewportSample _source;
    private ulong _previewOffset, _previewEnd, _previewLayout;
    private double _previewWithin;
    private MarkdownFollowPoint? _lastSeen, _lastApplied;
    private int _direction;
    private MarkdownFollowPair? _pair;
    private MarkdownPreviewFollowPair? _previewPair;

    internal long Epoch { get; private set; }
    internal long Geometry { get; private set; }
    internal long ScrollSerial { get; private set; }
    internal MarkdownFollowPane Pane { get; private set; }
    internal MarkdownFollowKind Kind { get; private set; } = MarkdownFollowKind.Viewport;
    internal MarkdownFollowWork? Active { get; private set; }
    internal bool HasPending => _caretDirty || _viewportDirty;
    internal MarkdownFollowPair? Pair => _pair;
    internal MarkdownPreviewFollowPair? PreviewPair => _previewPair;
    internal MarkdownFollowDispatch CaptureViewportDispatch() => new(Epoch, Geometry, ScrollSerial, _viewportSerial, Pane);
    internal bool ViewportDispatchCurrent(MarkdownFollowDispatch dispatch) =>
        dispatch == CaptureViewportDispatch() && Kind == MarkdownFollowKind.Viewport;
    private MarkdownPreviewViewport PreviewSample => new(_previewOffset, _previewEnd, _previewWithin, _previewLayout);

    internal void Reset(MarkdownFollowPane pane)
    {
        ++Epoch;
        ++ScrollSerial;
        Pane = pane;
        Kind = MarkdownFollowKind.Viewport;
        _caretDirty = _viewportDirty = _hasViewport = false;
        _lastSeen = _lastApplied = null;
        _direction = 0;
        _pair = null;
        _previewPair = null;
        _previewLayout = 0;
        _caretSourceSelectionRevision = null;
        _searchMatch = null;
        // A canceled native operation still owns the sole admission until retirement.
    }

    internal void SourceGeometryChanged()
    {
        ++Geometry;
        ++ScrollSerial;
        _previewPair = null;
        _lastSeen = _lastApplied = null;
        _direction = 0;
        _hasViewport = _viewportDirty = false;
        // The byte-to-preview pair is semantic and may survive source reflow.
    }

    internal void ObserveCaret(ulong offset, ulong length, bool navigate, bool force = false,
        MarkdownSearchMatch? searchMatch = null, ulong? sourceSelectionRevision = null)
    {
        bool changed = force || offset != _caretOffset || length != _caretLength || searchMatch != _searchMatch ||
            sourceSelectionRevision != _caretSourceSelectionRevision;
        if (!changed) return;
        _caretOffset = offset;
        _caretLength = length;
        _caretSourceSelectionRevision = sourceSelectionRevision;
        _searchMatch = searchMatch;
        ++_caretSerial;
        _caretDirty = true;
        if (navigate)
        {
            Kind = MarkdownFollowKind.Caret;
            ++ScrollSerial;
            _viewportDirty = false;
        }
        else if (Kind == MarkdownFollowKind.Caret)
        {
            Kind = MarkdownFollowKind.Viewport;
            ++ScrollSerial;
        }
    }

    internal bool DiscardSourceCaret()
    {
        if (Pane != MarkdownFollowPane.Source || _searchMatch.HasValue) return false;
        ++_caretSerial;
        _caretDirty = false;
        // Retire this selection debt, including an already admitted result,
        // without taking viewport ownership or admitting a second operation.
        BeginViewportIntent(retry: false);
        return true;
    }

    internal bool DiscardClearedSourceCaret(ulong offset, ulong sourceSelectionRevision)
    {
        if (Pane != MarkdownFollowPane.Source) return false;
        // The host has proved that the explicit source match was cleared. Keep
        // its ordinary caret as the observed baseline while retiring the old
        // debt: a delayed notification at this same position and native revision
        // is not new input. A later same-byte selection claim still takes effect.
        _caretOffset = offset;
        _caretLength = 0;
        _caretSourceSelectionRevision = sourceSelectionRevision;
        _searchMatch = null;
        return DiscardSourceCaret();
    }

    internal void BeginViewportIntent(bool retry = true)
    {
        if (Kind != MarkdownFollowKind.Viewport)
        {
            Kind = MarkdownFollowKind.Viewport;
            ++ScrollSerial;
            _lastApplied = null;
            _direction = 0;
        }
        if (retry && _hasViewport) _viewportDirty = true;
    }

    internal static double BottomBlend(double progress, double extent, double viewportHeight)
    {
        if (progress == 1) return 1;
        if (!(progress > 0) || !(extent > 0) || !(viewportHeight > 0)) return 0;
        // Ease over the driver's last screen of travel, measured from its actual end.
        double span = Math.Min(extent, viewportHeight);
        double t = Math.Clamp(1 - (1 - progress) * extent / span, 0, 1);
        return t * t;
    }

    internal bool ObserveSource(MarkdownViewportSample sample)
    {
        Validate(sample);
        bool changed = !_hasViewport || !SameViewport(_source, sample);
        if (!changed) return false;
        _source = sample;
        ObserveViewport(new(sample.DisplayRow, sample.Fraction));
        return true;
    }

    internal bool IsCurrentSourceViewport(MarkdownViewportSample sample) =>
        _hasViewport && Pane == MarkdownFollowPane.Source && SameViewport(_source, sample);

    internal bool PreviewLayoutChanged(ulong layout)
    {
        if (_previewLayout == layout) return false;
        _previewLayout = layout;
        ++Geometry;
        _previewPair = null;
        _lastApplied = null;
        _direction = 0;
        if (Pane == MarkdownFollowPane.Preview)
        {
            _hasViewport = _viewportDirty = false;
            _lastSeen = null;
        }
        return true;
    }

    internal bool ObservePreview(ulong start, ulong end, double progress, ulong layout)
    {
        if (start > end || end > long.MaxValue || !double.IsFinite(progress) || progress < 0 || progress > 1)
            throw new ArgumentOutOfRangeException(nameof(start));
        PreviewLayoutChanged(layout);
        return ObservePreviewCore(start, end, progress);
    }

    private bool ObservePreviewCore(ulong start, ulong end, double progress)
    {
        if (start > long.MaxValue || !double.IsFinite(progress)) throw new ArgumentOutOfRangeException(nameof(start));
        bool changed = !_hasViewport || start != _previewOffset || end != _previewEnd || progress != _previewWithin;
        if (!changed) return false;
        _previewOffset = start;
        _previewEnd = end;
        _previewWithin = progress;
        ObserveViewport(new((long)start, progress));
        return true;
    }

    private void ObserveViewport(MarkdownFollowPoint position)
    {
        ++_viewportSerial;
        if (Kind == MarkdownFollowKind.Caret)
        {
            _lastSeen = position;
            _hasViewport = true;
            return;
        }
        int direction = _lastSeen is { } previous ? Math.Sign(position.CompareTo(previous)) : 0;
        if (direction != 0)
        {
            if (_direction != 0 && direction != _direction)
            {
                ++ScrollSerial;
                _lastApplied = null;
            }
            _direction = direction;
        }
        _lastSeen = position;
        _hasViewport = _viewportDirty = true;
    }

    internal void RetryCurrent()
    {
        if (Kind == MarkdownFollowKind.Caret) _caretDirty = true;
        else if (_hasViewport) _viewportDirty = true;
    }

    internal bool ActiveViewportObsolete => Active is { } active &&
        (active.Epoch != Epoch || (active.Operation != MarkdownFollowOperation.Caret &&
        (active.ScrollSerial != ScrollSerial || active.Geometry != Geometry)));

    internal bool TryTakeCachedSource(out MarkdownViewportSample sample, out MarkdownFollowPair pair)
    {
        sample = default;
        pair = default;
        if (Pane != MarkdownFollowPane.Source || Kind != MarkdownFollowKind.Viewport || !_viewportDirty ||
            !_hasViewport || _pair is not { } cached || !cached.Matches(_source)) return false;
        _viewportDirty = false;
        if (!CanAdvance(new(_source.DisplayRow, _source.Fraction))) return false;
        sample = _source;
        pair = cached;
        return true;
    }

    internal bool TryTakeCachedPreview(out MarkdownPreviewViewport sample, out MarkdownPreviewFollowPair pair)
    {
        sample = default;
        pair = default;
        if (Pane != MarkdownFollowPane.Preview || Kind != MarkdownFollowKind.Viewport || !_viewportDirty ||
            !_hasViewport || _previewPair is not { } cached || !cached.Matches(PreviewSample)) return false;
        _viewportDirty = false;
        if (!CanAdvance(new((long)_previewOffset, _previewWithin))) return false;
        sample = PreviewSample;
        pair = cached;
        return true;
    }

    internal bool TryBegin(out MarkdownFollowWork work, bool follow = true)
    {
        work = default;
        if (!follow)
        {
            if (Kind == MarkdownFollowKind.Caret)
            {
                Kind = MarkdownFollowKind.Viewport;
                ++ScrollSerial;
            }
            _viewportDirty = false;
            if (Pane != MarkdownFollowPane.Source || !_searchMatch.HasValue) _caretDirty = false;
        }
        if (Active != null) return false;
        bool caret = _caretDirty && (!_viewportDirty || _preferCaret || Kind == MarkdownFollowKind.Caret);
        if (!caret && !_viewportDirty) return false;
        if (caret)
        {
            _caretDirty = false;
            _preferCaret = false;
            work = new(++_nextWork, Epoch, Geometry, ScrollSerial, _caretSerial, Pane,
                MarkdownFollowOperation.Caret, _caretOffset, _caretLength, default, default, _searchMatch,
                SourceSelectionRevision: _caretSourceSelectionRevision);
        }
        else
        {
            _viewportDirty = false;
            _preferCaret = true;
            work = new(++_nextWork, Epoch, Geometry, ScrollSerial, 0, Pane,
                Pane == MarkdownFollowPane.Source ? MarkdownFollowOperation.SourceViewport : MarkdownFollowOperation.PreviewViewport,
                _previewOffset, 0, _lastSeen.GetValueOrDefault(), _source,
                EndOffset: _previewEnd, TargetLayout: _previewLayout);
        }
        Active = work;
        return true;
    }

    internal bool CompleteCaret(MarkdownFollowWork work, out bool reveal)
    {
        reveal = false;
        if (!Retire(work) || work.Epoch != Epoch || work.Pane != Pane || work.CaretSerial != _caretSerial) return false;
        reveal = work.ScrollSerial == ScrollSerial && Kind == MarkdownFollowKind.Caret;
        return true;
    }

    internal bool CaretCurrent(MarkdownFollowWork work) => work.Epoch == Epoch && work.Pane == Pane &&
        work.CaretSerial == _caretSerial;

    internal bool CanRevealCaret(MarkdownFollowWork work) => CaretCurrent(work) &&
        work.ScrollSerial == ScrollSerial && Kind == MarkdownFollowKind.Caret;

    internal bool CompleteSource(MarkdownFollowWork work, MarkdownFollowPair pair, out MarkdownViewportSample sample)
    {
        sample = work.Source;
        if (!Retire(work) || work.Epoch != Epoch || work.Pane != Pane ||
            work.Operation != MarkdownFollowOperation.SourceViewport) return false;
        if (pair.StartByte != work.Source.StartByte || pair.EndByte != work.Source.EndByte || pair.Start > pair.End)
            throw new ArgumentOutOfRangeException(nameof(pair));
        _pair = pair;
        if (work.Geometry != Geometry || work.ScrollSerial != ScrollSerial || Kind != MarkdownFollowKind.Viewport ||
            !CanAdvance(work.Position)) return false;
        // Latest fraction is usable only inside these exact semantic row endpoints.
        if (_hasViewport && pair.Matches(_source) && _source.DisplayRow == work.Source.DisplayRow)
            sample = _source;
        return true;
    }

    internal bool CompletePreview(MarkdownFollowWork work, MarkdownPreviewFollowPair pair,
        out MarkdownPreviewViewport sample)
    {
        sample = new(work.Offset, work.EndOffset, work.Position.Within, work.TargetLayout);
        if (!Retire(work) || work.Epoch != Epoch || work.Pane != Pane ||
            work.Operation != MarkdownFollowOperation.PreviewViewport) return false;
        if (!pair.Matches(sample) || pair.StartByte < 0 || pair.EndByte < pair.StartByte)
            throw new ArgumentOutOfRangeException(nameof(pair));
        if (work.Geometry != Geometry || work.TargetLayout != _previewLayout) return false;
        _previewPair = pair;
        if (work.ScrollSerial != ScrollSerial || Kind != MarkdownFollowKind.Viewport || !CanAdvance(work.Position))
            return false;
        if (_hasViewport && pair.Matches(PreviewSample)) sample = PreviewSample;
        return true;
    }

    internal bool Retire(MarkdownFollowWork work)
    {
        if (Active is not { } active || active.Id != work.Id) return false;
        Active = null;
        return true;
    }

    internal void SourceApplied(MarkdownViewportSample sample, bool admitted)
    {
        if (admitted) _lastApplied = new(sample.DisplayRow, sample.Fraction);
        if (!admitted || (_hasViewport && SameViewport(_source, sample))) _viewportDirty = false;
    }

    internal void PreviewApplied(MarkdownPreviewViewport sample, bool admitted)
    {
        if (admitted) _lastApplied = new((long)sample.Start, sample.Progress);
        if (!admitted || (_hasViewport && sample == PreviewSample)) _viewportDirty = false;
    }

    private bool CanAdvance(MarkdownFollowPoint position)
    {
        if (!_hasViewport || _lastSeen is not { } latest) return false;
        int toLatest = position.CompareTo(latest);
        if ((_direction >= 0 && toLatest > 0) || (_direction < 0 && toLatest < 0)) return false;
        if (_lastApplied is not { } applied) return true;
        int progress = position.CompareTo(applied);
        return _direction >= 0 ? progress >= 0 : progress <= 0;
    }

    internal static bool SameViewport(MarkdownViewportSample left, MarkdownViewportSample right) =>
        left.StartByte == right.StartByte && left.EndByte == right.EndByte && left.DisplayRow == right.DisplayRow &&
        left.Fraction == right.Fraction;

    private static void Validate(MarkdownViewportSample sample)
    {
        if (sample.StartByte < 0 || sample.EndByte < sample.StartByte || sample.EndByte > sample.Length ||
            sample.DisplayRow < 0 || !double.IsFinite(sample.Fraction) || sample.Fraction < 0 || sample.Fraction > 1)
            throw new ArgumentOutOfRangeException(nameof(sample));
    }
}
