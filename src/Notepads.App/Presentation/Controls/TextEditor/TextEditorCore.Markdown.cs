// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Infrastructure.Diagnostics;
using Windows.UI.Xaml;
using WinUIEditor;

namespace Notepads.Presentation.Controls.TextEditor;

internal readonly record struct MarkdownTextModification(long Position, long DeleteLength, long InsertLength);

internal readonly record struct MarkdownUpdateUIContext(long Serial, ulong Attachment, ulong DocumentRevision,
    ulong SelectionRevision, bool SelectionFromScroll);

public sealed partial class TextEditorCore
{
    private bool _markdownViewportAwaitingLayout;
    private double _markdownScrollExtent = -1;
    private long _markdownDispatchSerial;
    private MarkdownUpdateUIContext? _markdownUpdateUIContext;
    private MarkdownSearchMatch? _markdownSearchMatch;
    private long _markdownSearchSerial;

    internal event EventHandler MarkdownSearchMatchChanged;

    internal bool TryGetMarkdownSearchMatch(out MarkdownSearchMatch match)
    {
        match = _markdownSearchMatch.GetValueOrDefault();
        return _markdownSearchMatch.HasValue && MarkdownCanRead && IsEnabled &&
            match.Attachment == _view.DocumentAttachmentGeneration && match.NativeRevision == Native.DocumentRevision &&
            match.SelectionRevision == Native.SelectionRevision && match.ContentVersion == ContentVersion &&
            match.Start >= 0 && match.End >= match.Start && match.End <= Native.Length &&
            Native.SelectionStart == match.Start && Native.SelectionEnd == match.End;
    }

    private void PublishMarkdownSearchMatch(long start, long end, ulong attachment, ulong revision, long contentVersion)
    {
        if (!MarkdownCanRead || !IsEnabled || attachment != _view.DocumentAttachmentGeneration ||
            revision != Native.DocumentRevision || contentVersion != ContentVersion || start < 0 || end < start ||
            end > Native.Length || Native.SelectionStart != start || Native.SelectionEnd != end) return;
        _markdownSearchMatch = new(++_markdownSearchSerial, start, end, attachment, revision,
            Native.SelectionRevision, contentVersion);
        RaiseMarkdownSearchMatchChanged();
    }

    internal void ClearMarkdownSearchMatch()
    {
        if (!_markdownSearchMatch.HasValue) return;
        _markdownSearchMatch = null;
        ++_markdownSearchSerial;
        RaiseMarkdownSearchMatchChanged();
    }

    private void InvalidateMarkdownSearchMatch()
    {
        if (_markdownSearchMatch.HasValue && !TryGetMarkdownSearchMatch(out _)) ClearMarkdownSearchMatch();
    }

    private void RaiseMarkdownSearchMatchChanged()
    {
        if (MarkdownSearchMatchChanged == null) return;
        foreach (EventHandler subscriber in MarkdownSearchMatchChanged.GetInvocationList())
        {
            try { subscriber(this, EventArgs.Empty); }
            catch (Exception error) { LoggingService.LogException(error); }
        }
    }

    internal void InvalidateMarkdownFollowDispatch() => ++_markdownDispatchSerial;

    private MarkdownUpdateUIContext CaptureMarkdownUpdateUI() => new(++_markdownDispatchSerial,
        _view.DocumentAttachmentGeneration, Native.DocumentRevision, _view.SelectionRevision,
        _view.IsSelectionFromScroll);

    internal bool TryGetMarkdownUpdateUI(out MarkdownUpdateUIContext context)
    {
        context = _markdownUpdateUIContext.GetValueOrDefault();
        return _markdownUpdateUIContext.HasValue && context.Serial == _markdownDispatchSerial &&
            context.Attachment == _view.DocumentAttachmentGeneration &&
            context.DocumentRevision == Native.DocumentRevision && context.SelectionRevision == _view.SelectionRevision &&
            context.SelectionFromScroll == _view.IsSelectionFromScroll;
    }

    internal event EventHandler<MarkdownTextModification> MarkdownModified;
    internal event EventHandler MarkdownReset;
    internal event EventHandler MarkdownViewportChanged;
    internal event EventHandler MarkdownNavigateRequested;
    internal event EventHandler MarkdownViewportGeometryChanged;
    internal event EventHandler MarkdownAvailabilityChanged;

    internal bool MarkdownCanRead => !_disposed && !_settingText && !_movingToWindow;
    internal ulong MarkdownNativeRevision => Native.DocumentRevision;
    internal long MarkdownUtf8Length => Native.Length;
    internal long MarkdownCaretByte => Native.CurrentPos;
    internal ulong MarkdownSelectionRevision => _view.SelectionRevision;
    internal bool MarkdownIsLineStartByte(long position) => position > 0 &&
        position == Native.PositionFromLine(Native.LineFromPosition(position));

    internal long MarkdownViewportByte => _view.ViewportSourceByte;

    internal (double Progress, double Extent, double ViewportHeight) MarkdownScrollMetrics =>
        MarkdownCanRead && IsEnabled ? (_view.VerticalScrollProgress, _view.VerticalScrollExtent, _view.ActualHeight) : (-1, 0, 0);

    internal bool MarkdownSetScrollEndpoint(double progress)
    {
        if (!MarkdownCanRead || !IsEnabled) return false;
        bool admitted = _view.TrySetVerticalScrollProgress(progress);
        if (!admitted) _markdownViewportAwaitingLayout = true;
        return admitted;
    }

    internal void StopMarkdownViewportSampling() => _markdownViewportAwaitingLayout = false;

    internal bool TryGetMarkdownViewport(out MarkdownViewportSample sample)
    {
        sample = default;
        if (!MarkdownCanRead) return false;
        var revision = Native.DocumentRevision;
        var length = Native.Length;
        var version = ContentVersion;
        var viewport = _view.GetSourceViewport();
        if (!viewport.IsAvailable)
        {
            // A coalesced geometry signal may precede the bounded native wrap
            // work. One subsequent real paint wakes this unavailable sample;
            // no timer, forced redraw or self-scheduled retry is introduced.
            _markdownViewportAwaitingLayout = true;
            return false;
        }
        if (!MarkdownCanRead || Native.DocumentRevision != revision ||
            Native.Length != length || ContentVersion != version || viewport.StartByte < 0 ||
            viewport.EndByte < viewport.StartByte || viewport.EndByte > length || viewport.DisplayRow < 0 ||
            !double.IsFinite(viewport.Fraction) || viewport.Fraction < 0 || viewport.Fraction > 1)
            return false;
        _markdownViewportAwaitingLayout = false;
        sample = new MarkdownViewportSample(viewport.StartByte, viewport.EndByte, viewport.DisplayRow,
            viewport.Fraction, revision, length, version);
        return true;
    }

    internal bool MarkdownSetViewportBetweenSourcePositions(long start, long end, double progress)
    {
        if (!MarkdownCanRead || start < 0 || end < start || end > Native.Length ||
            !double.IsFinite(progress) || progress < 0 || progress > 1) return false;
        var attachment = _view.DocumentAttachmentGeneration;
        var revision = Native.DocumentRevision;
        var length = Native.Length;
        var version = ContentVersion;
        bool admitted = _view.SetViewportBetweenSourcePositions(start, end, progress);
        if (!MarkdownCanRead || attachment != _view.DocumentAttachmentGeneration ||
            revision != Native.DocumentRevision || length != Native.Length || version != ContentVersion) return false;
        // No timer or Rendering retry: an unavailable native layout gets one wake
        // from its next actual Painted notification, just like viewport sampling.
        if (!admitted) _markdownViewportAwaitingLayout = true;
        return admitted;
    }

    internal void MarkdownSetCaretByte(long position)
    {
        if (MarkdownCanRead && IsEnabled) Native.SetEmptySelection(Math.Clamp(position, 0, Native.Length));
    }

    internal void MarkdownRevealByte(long position) => MarkdownRevealByte(position, false);

    internal void MarkdownRevealByte(long position, bool alignToTop)
    {
        if (!MarkdownCanRead) return;
        position = Math.Clamp(position, 0, Native.Length);
        if (alignToTop) _view.RevealSourceByte(position);
        else Native.ScrollRange(position, position);
    }

    private void NotifyMarkdownModification(ModifiedEventArgs args)
    {
        if (_settingText || MarkdownModified == null) return;
        var inserted = (args.ModificationType & (int)ModificationFlags.InsertText) != 0;
        var deleted = (args.ModificationType & (int)ModificationFlags.DeleteText) != 0;
        // Never retain WinRT notification arguments or their borrowed text pointer.
        var modification = new MarkdownTextModification(args.Position,
            deleted ? args.Length : 0, inserted ? args.Length : 0);
        foreach (EventHandler<MarkdownTextModification> subscriber in MarkdownModified.GetInvocationList())
        {
            try { subscriber(this, modification); }
            catch (Exception error) { LoggingService.LogException(error); }
        }
    }

    private void NotifyMarkdownReset()
    {
        ClearMarkdownSearchMatch();
        _markdownViewportAwaitingLayout = false;
        if (_disposed || MarkdownReset == null) return;
        foreach (EventHandler subscriber in MarkdownReset.GetInvocationList())
        {
            try { subscriber(this, EventArgs.Empty); }
            catch (Exception error) { LoggingService.LogException(error); }
        }
    }

    private void NotifyMarkdownViewportChanged()
    {
        if (_disposed || _settingText || MarkdownViewportChanged == null) return;
        foreach (EventHandler subscriber in MarkdownViewportChanged.GetInvocationList())
        {
            try { subscriber(this, EventArgs.Empty); }
            catch (Exception error) { LoggingService.LogException(error); }
        }
    }

    private void NotifyMarkdownAvailabilityChanged()
    {
        ClearMarkdownSearchMatch();
        _markdownViewportAwaitingLayout = false;
        if (_disposed || MarkdownAvailabilityChanged == null) return;
        foreach (EventHandler subscriber in MarkdownAvailabilityChanged.GetInvocationList())
        {
            try { subscriber(this, EventArgs.Empty); }
            catch (Exception error) { LoggingService.LogException(error); }
        }
    }

    private void NotifyMarkdownViewportGeometryChanged()
    {
        if (_disposed || _settingText || MarkdownViewportGeometryChanged == null) return;
        foreach (EventHandler subscriber in MarkdownViewportGeometryChanged.GetInvocationList())
        {
            try { subscriber(this, EventArgs.Empty); }
            catch (Exception error) { LoggingService.LogException(error); }
        }
    }

    private void OnNativeMarkdownPainted(Editor sender, PaintedEventArgs args)
    {
        if (!MarkdownCanRead || MarkdownViewportGeometryChanged == null) return;
        // Lazy wrapping can change the scroll range without resizing the view.
        double extent = _view.VerticalScrollExtent;
        if (!_markdownViewportAwaitingLayout && extent == _markdownScrollExtent) return;
        _markdownScrollExtent = extent;
        _markdownViewportAwaitingLayout = false;
        NotifyMarkdownViewportGeometryChanged();
    }

    private void MarkdownViewport_Loaded(object sender, RoutedEventArgs args)
    {
        if (_disposed) return;
        Native.Painted -= OnNativeMarkdownPainted;
        Native.Painted += OnNativeMarkdownPainted;
    }

    private void MarkdownViewport_Unloaded(object sender, RoutedEventArgs args)
    {
        _markdownViewportAwaitingLayout = false;
        Native.Painted -= OnNativeMarkdownPainted;
    }

    private void MarkdownEnabledChanged(object sender, DependencyPropertyChangedEventArgs args)
    {
        if (!IsEnabled) ClearMarkdownSearchMatch();
    }
}
