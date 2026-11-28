// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics.CodeAnalysis;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Key = Windows.System.VirtualKey;
using Notepads.Markdown;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Threading;
using Notepads.Presentation.Controls.TextEditor;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Media;

namespace Notepads.Presentation.Controls.Markdown;

public sealed partial class MarkdownExtensionView
{
    [SuppressMessage("Usage", "CA2213:Disposable fields should be disposed",
        Justification = "The owning TextEditor waits for mapping completion before disposing this borrowed core.")]
    private TextEditorCore _linkEditor;
    private MarkdownView _linkView;
    private readonly PointerEventHandler _linkSourcePointerHandler;
    private readonly PointerEventHandler _linkSourceWheelHandler;
    private readonly PointerEventHandler _linkPreviewPointerHandler;
    private readonly PointerEventHandler _linkPreviewWheelHandler;
    private readonly MarkdownFollowCoordinator _linkCoordinator = new();
    [SuppressMessage("Usage", "CA2213:Disposable fields should be disposed",
        Justification = "The sole mapping operation disposes its cancellation source after native completion.")]
    private CancellationTokenSource _linkCancellation;
    private Task _linkCompletion = Task.CompletedTask;
    private LinkContext _linkContext;
    private bool _linkRunning, _linkRendering, _linkTargetAvailable, _linkViewportResample;
    private ulong _linkTargetLayout;
    private ulong _linkPreservedViewportRevision;
    private (MarkdownViewportSample Source, MarkdownViewportAnchor Preview)? _linkRestoredViewport;
    private int _linkApplying, _linkSampling;
    private bool _linkFollowEnabled;
    private long _linkSearchNavigate;
    private (long Byte, ulong NativeRevision, ulong SelectionRevision)? _linkSourceNavigation;
    private LinkSelection? _linkOwnedMatch;
    private LinkPublication _linkPublication;
    private bool LinkPreviewPane => _linkCoordinator.Pane == MarkdownFollowPane.Preview;
    private bool PreservingPreviewViewport => _linkPreservedViewportRevision != 0 &&
        _linkPreservedViewportRevision == _session.AcceptedRevision;
    private Task SourceLinkCompletion => _linkCompletion;

    // Compiled away in the product; the integration partial can hold a real
    // mapped caret result before publication to exercise a later selection.
    partial void OnLinkCaretMapped(MarkdownFollowWork work, ref Task publicationWait);
    partial void OnLinkSourceViewportMapped(MarkdownFollowWork work, ref Task publicationWait);

    private sealed record LinkContext(TextEditorCore Editor, MarkdownView View, MarkdownSourceSession Session,
        Guid Document, ulong Revision, ulong TextGeneration, MarkdownEditorStamp Stamp);

    private readonly record struct LinkSelection(bool Available, Guid Document, ulong Revision,
        ulong TextGeneration, ulong Start, ulong Length);

    private sealed class LinkPublication
    {
        internal int Notifications;
        internal LinkSelection First;
        internal bool MatchesAtNotification;
    }

    private static LinkSelection ReadLinkSelection(MarkdownSelection selection) => new(selection.IsAvailable,
        selection.DocumentId, selection.Revision, selection.TextGeneration, selection.Start, selection.Length);

    private void InitializeSourceLink()
    {
        DisposeSourceLink();
        _linkEditor = _editor;
        _linkView = _view;
    }

    private void StartSourceLink()
    {
        if (_linkRunning || _disposed || _linkEditor == null || _linkView == null) return;
        _linkRunning = true;
        // Reattaching the same revision already restores each pane's viewport. Wait for navigation
        // before applying a new mapping, even if layout or programmatic focus changes on return.
        _linkPreservedViewportRevision = !_restoringSessionViewport && _session.Status == MarkdownSessionStatus.Restoring
            ? _session.AcceptedRevision : 0;
        _linkRestoredViewport = null;
        _linkCoordinator.Reset(!_restoringSessionViewport && _linkView.FocusState != FocusState.Unfocused
            ? MarkdownFollowPane.Preview : MarkdownFollowPane.Source);
        _linkEditor.SelectionChanged += LinkSourceSelection;
        _linkEditor.MarkdownNavigateRequested += LinkSourceNavigate;
        _linkEditor.MarkdownSearchMatchChanged += LinkSourceSearchMatch;
        _linkEditor.ScrollStarted += LinkSourceScrollStarted;
        _linkEditor.MarkdownViewportChanged += LinkSourceViewport;
        _linkEditor.MarkdownViewportGeometryChanged += LinkSourceGeometry;
        _linkEditor.MarkdownAvailabilityChanged += LinkSourceAvailability;
        _linkView.SelectionChanged += LinkPreviewSelection;
        _linkView.ViewportChanged += LinkPreviewViewport;
        _linkView.VerticalScrollStarting += LinkPreviewScrollStarting;
        _linkView.VerticalScrollChanged += LinkPreviewExtentChanged;
        _linkEditor.GotFocus += LinkSourceFocus;
        _linkView.GotFocus += LinkPreviewFocus;
        _linkEditor.PreviewKeyDown += LinkSourceKey;
        _linkView.PreviewKeyDown += LinkPreviewKey;
        _linkEditor.AddHandler(PointerPressedEvent, _linkSourcePointerHandler, true);
        _linkEditor.AddHandler(PointerWheelChangedEvent, _linkSourceWheelHandler, true);
        _linkView.AddHandler(PointerPressedEvent, _linkPreviewPointerHandler, true);
        _linkView.AddHandler(PointerWheelChangedEvent, _linkPreviewWheelHandler, true);
        SourceLinkStateChanged();
    }

    private void StopSourceLink()
    {
        var editor = _linkEditor;
        CancelSourceLink();
        _linkSearchNavigate = 0;
        _linkSourceNavigation = null;
        ClearOwnedLinkMatch();
        editor?.StopMarkdownViewportSampling();
        if (!_linkRunning) return;
        _linkRunning = false;
        _linkEditor.SelectionChanged -= LinkSourceSelection;
        _linkEditor.MarkdownNavigateRequested -= LinkSourceNavigate;
        _linkEditor.MarkdownSearchMatchChanged -= LinkSourceSearchMatch;
        _linkEditor.ScrollStarted -= LinkSourceScrollStarted;
        _linkEditor.MarkdownViewportChanged -= LinkSourceViewport;
        _linkEditor.MarkdownViewportGeometryChanged -= LinkSourceGeometry;
        _linkEditor.MarkdownAvailabilityChanged -= LinkSourceAvailability;
        _linkView.SelectionChanged -= LinkPreviewSelection;
        _linkView.ViewportChanged -= LinkPreviewViewport;
        _linkView.VerticalScrollStarting -= LinkPreviewScrollStarting;
        _linkView.VerticalScrollChanged -= LinkPreviewExtentChanged;
        _linkEditor.GotFocus -= LinkSourceFocus;
        _linkView.GotFocus -= LinkPreviewFocus;
        _linkEditor.PreviewKeyDown -= LinkSourceKey;
        _linkView.PreviewKeyDown -= LinkPreviewKey;
        _linkEditor.RemoveHandler(PointerPressedEvent, _linkSourcePointerHandler);
        _linkEditor.RemoveHandler(PointerWheelChangedEvent, _linkSourceWheelHandler);
        _linkView.RemoveHandler(PointerPressedEvent, _linkPreviewPointerHandler);
        _linkView.RemoveHandler(PointerWheelChangedEvent, _linkPreviewWheelHandler);
    }

    private void DisposeSourceLink()
    {
        StopSourceLink();
        _linkEditor = null;
        _linkView = null;
    }

    private void CancelSourceLink(MarkdownFollowPane? nextPane = null)
    {
        StopLinkRendering();
        _linkCoordinator.Reset(nextPane ?? _linkCoordinator.Pane);
        if (!nextPane.HasValue) _linkContext = null;
        _linkTargetAvailable = false;
        _linkViewportResample = false;
        CancelLinkOperation();
    }

    private void CancelLinkOperation()
    {
        try { _linkCancellation?.Cancel(); }
        catch (Exception error) { LoggingService.LogException(error); }
    }

    private bool TryReadLinkContext(out LinkContext context)
    {
        context = null;
        if (!_linkRunning || !_active || _disposed ||
            (!FollowSource && (LinkPreviewPane || !_linkEditor.TryGetMarkdownSearchMatch(out _))) ||
            !ReferenceEquals(_editor, _linkEditor) || !ReferenceEquals(_view, _linkView) ||
            _session == null || !_session.IsCurrent || !_linkEditor.MarkdownCanRead ||
            _linkView.DisplayedDocumentId != _session.DocumentId ||
            _linkView.DisplayedRevision != _session.AcceptedRevision) return false;
        var selection = _linkView.GetSelection();
        if (!selection.IsAvailable || selection.DocumentId != _session.DocumentId ||
            selection.Revision != _session.AcceptedRevision) return false;
        var stamp = _session.AcceptedStamp;
        var previous = _linkContext;
        context = previous != null && ReferenceEquals(previous.Editor, _linkEditor) &&
            ReferenceEquals(previous.View, _linkView) && ReferenceEquals(previous.Session, _session) &&
            previous.Document == selection.DocumentId && previous.Revision == selection.Revision &&
            previous.TextGeneration == selection.TextGeneration && previous.Stamp == stamp
            ? previous
            : new(_linkEditor, _linkView, _session, selection.DocumentId, selection.Revision, selection.TextGeneration, stamp);
        return true;
    }

    private bool EnsureLinkContext(out LinkContext context)
    {
        if (!TryReadLinkContext(out context))
        {
            if (_linkContext != null || _linkCoordinator.HasPending) CancelSourceLink();
            return false;
        }
        if (_linkContext == context) return true;
        CancelSourceLink();
        _linkContext = context;
        bool pendingFind = _linkEditor.TryGetMarkdownSearchMatch(out var match) && _linkSearchNavigate == match.Serial;
        bool pendingGoTo = !LinkPreviewPane && _linkSourceNavigation is { } navigation &&
            navigation.Byte == context.Editor.MarkdownCaretByte && navigation.NativeRevision == context.Stamp.NativeRevision &&
            navigation.SelectionRevision == context.Editor.MarkdownSelectionRevision;
        _linkSourceNavigation = null;
        CaptureLinkCaret(context, pendingGoTo, true);
        CaptureLinkViewport(context);
        // Reopening/follow-on/restoration replays selection without granting it a second reveal.
        // A new explicit Find that waited for this source revision keeps its one reveal.
        if ((!pendingFind && !pendingGoTo) || !FollowSource) _linkCoordinator.BeginViewportIntent();
        return true;
    }

    private bool LinkContextCurrent(LinkContext context) => _linkContext == context &&
        TryReadLinkContext(out var current) && current == context;

    private void SourceLinkStateChanged()
    {
        try
        {
            if (_linkFollowEnabled != FollowSource)
            {
                _linkFollowEnabled = FollowSource;
                _linkSearchNavigate = 0;
                CancelSourceLink();
            }
            if (!EnsureLinkContext(out var context)) return;
            if (_linkSearchNavigate != 0) CaptureLinkCaret(context, false, true);
            ObserveTargetLayout(context, _linkView.GetViewportAnchor());
            CaptureLinkViewport(context);
            WakeSourceLink();
        }
        catch (Exception error) { LoggingService.LogException(error); CancelSourceLink(); }
    }

    private enum LinkIntent { Focus, Scroll, Navigate, Fragment }

    private void ReleasePreservedViewport()
    {
        _linkPreservedViewportRevision = 0;
        _linkRestoredViewport = null;
    }

    private void SetLinkPane(bool preview, LinkIntent intent)
    {
        bool focusOnly = intent == LinkIntent.Focus;
        // Synchronization suppresses its own focus/scroll notifications, but
        // an explicit command issued by a synchronous selection subscriber is newer.
        if (!_linkRunning || (_linkApplying != 0 && intent is not (LinkIntent.Navigate or LinkIntent.Fragment)) ||
            (focusOnly && LinkPreviewPane == preview)) return;
        if (_restoringSessionViewport && preview) return;
        if (intent == LinkIntent.Navigate) FinishSessionViewportRestore();
        if (!focusOnly)
        {
            ReleasePreservedViewport();
            _linkSourceNavigation = null;
            if (!preview || intent == LinkIntent.Scroll) CancelPendingFragment();
        }
        _linkEditor.InvalidateMarkdownFollowDispatch();
        try
        {
            _linkSearchNavigate = 0;
            if (FollowSource)
            {
                if (preview) _linkEditor.CancelScrollAnimation();
                else _linkView.CancelScrollAnimation();
            }
            if (preview && intent == LinkIntent.Navigate) _linkEditor.ClearMarkdownSearchMatch();
            if (LinkPreviewPane != preview)
                CancelSourceLink(preview ? MarkdownFollowPane.Preview : MarkdownFollowPane.Source);
            // Restoring keyboard focus (including on tab return) does not request a caret reveal.
            if (focusOnly) return;
            if (intent is LinkIntent.Scroll or LinkIntent.Fragment)
            {
                if (FollowSource)
                {
                    _linkCoordinator.BeginViewportIntent(retry: false);
                    CancelObsoleteViewport();
                }
                return;
            }
            if (!EnsureLinkContext(out var context)) return;
            CaptureLinkCaret(context, true, true);
            CancelObsoleteViewport();
            WakeSourceLink();
        }
        catch (Exception error) { LoggingService.LogException(error); CancelSourceLink(); }
    }

    private void LinkSourceFocus(object sender, RoutedEventArgs args) => SetLinkPane(false, LinkIntent.Focus);
    private void LinkPreviewFocus(object sender, RoutedEventArgs args) => SetLinkPane(true, LinkIntent.Focus);
    private void LinkSourcePointer(object sender, PointerRoutedEventArgs args)
    {
        var intent = IsScrollBarPointer(args.OriginalSource, _linkEditor) ? LinkIntent.Scroll : LinkIntent.Focus;
        if (intent == LinkIntent.Focus) CancelPendingFragment();
        SetLinkPane(false, intent);
    }
    private void LinkPreviewPointer(object sender, PointerRoutedEventArgs args)
    {
        bool scrollBar = IsScrollBarPointer(args.OriginalSource, _linkView);
        // A touch on the restored preview can be a tap. Let the first actual viewport
        // movement claim scroll authority, as opposed to mapping back on pointer down.
        bool scroll = scrollBar || (!PreservingPreviewViewport || !_linkRestoredViewport.HasValue) &&
            args.Pointer.PointerDeviceType == Windows.Devices.Input.PointerDeviceType.Touch;
        SetLinkPane(true, scroll ? LinkIntent.Scroll : LinkIntent.Focus);
    }
    private void LinkSourceWheel(object sender, PointerRoutedEventArgs args) => SetLinkPane(false, LinkIntent.Scroll);
    private void LinkPreviewWheel(object sender, PointerRoutedEventArgs args) => SetLinkPane(true, LinkIntent.Scroll);
    private void LinkSourceKey(object sender, KeyRoutedEventArgs args)
    {
        CancelPendingFragment();
        SetLinkPane(false, LinkIntent.Focus);
    }
    private void LinkPreviewKey(object sender, KeyRoutedEventArgs args) => ApplyPreviewKeyIntent(args.Key);

    private void ApplyPreviewKeyIntent(Windows.System.VirtualKey key)
    {
        if (key is Key.Left or Key.Right or Key.Up or Key.Down or Key.PageUp or Key.PageDown or Key.Home or Key.End or Key.Space)
            SetLinkPane(true, LinkIntent.Scroll);
    }
    private void LinkSourceNavigate(object sender, EventArgs args)
    {
        SetLinkPane(false, LinkIntent.Navigate);
        if (_linkRunning && !_disposed && FollowSource && _linkContext == null)
            _linkSourceNavigation = (_linkEditor.MarkdownCaretByte, _linkEditor.MarkdownNativeRevision,
                _linkEditor.MarkdownSelectionRevision);
    }

    private void LinkSourceSearchMatch(object sender, EventArgs args)
    {
        if (!_linkRunning) return;
        try
        {
            if (!_linkEditor.TryGetMarkdownSearchMatch(out var match))
            {
                _linkSearchNavigate = 0;
                var dispatch = _linkCoordinator.CaptureViewportDispatch();
                var editor = _linkEditor;
                var selectionRevision = editor.MarkdownSelectionRevision;
                bool cleared = ClearOwnedLinkMatch();
                // Folding the old highlight can synchronously publish a new source
                // command. Its caret/reveal debt must survive this older clear.
                if (!_linkRunning || _disposed || !ReferenceEquals(editor, _linkEditor) ||
                    dispatch != _linkCoordinator.CaptureViewportDispatch() ||
                    selectionRevision != editor.MarkdownSelectionRevision) return;
                if (!cleared)
                {
                    DiscardClearedSourceCaret(_linkContext, selectionRevision);
                    return;
                }
                if (!LinkPreviewPane && EnsureLinkContext(out var current))
                {
                    CaptureLinkCaret(current, false, true);
                    WakeSourceLink();
                }
                return;
            }
            // Source search is an explicit source command even while preview keeps focus.
            SetLinkPane(false, LinkIntent.Navigate);
            _linkSearchNavigate = match.Serial;
            if (EnsureLinkContext(out var context)) CaptureLinkCaret(context, false, true);
            WakeSourceLink();
        }
        catch (Exception error) { LoggingService.LogException(error); CancelSourceLink(); }
    }

    private bool ClearOwnedLinkMatch()
    {
        var owned = _linkOwnedMatch;
        _linkOwnedMatch = null;
        if (owned is not { Available: true } prior || _linkView == null) return true;
        try
        {
            if (ReadLinkSelection(_linkView.GetSelection()) != prior) return false;
            // A nested selection belongs to that caller. Do not enqueue an
            // ordinary source caret over it after the old match is folded.
            return TryPublishLinkSelection(_linkView, prior with { Length = 0 }, out _);
        }
        catch (Exception error) { LoggingService.LogException(error); }
        return true;
    }

    private void DiscardClearedSourceCaret(LinkContext context, ulong selectionRevision)
    {
        if (context != null && !LinkPreviewPane && LinkContextCurrent(context) &&
            !context.Editor.TryGetMarkdownSearchMatch(out _) && context.Editor.MarkdownSelectionRevision == selectionRevision)
            _linkCoordinator.DiscardClearedSourceCaret(checked((ulong)context.Editor.MarkdownCaretByte), selectionRevision);
    }

    private static bool IsScrollBarPointer(object originalSource, DependencyObject pane)
    {
        for (var element = originalSource as DependencyObject; element != null && element != pane;
            element = VisualTreeHelper.GetParent(element))
            if (element is ScrollBar) return true;
        return false;
    }

    private void LinkSourceScrollStarted(object sender, EventArgs args) => SetLinkPane(false, LinkIntent.Scroll);

    private void LinkSourceSelection(object sender, RoutedEventArgs args)
    {
        if (LinkPreviewPane || _linkApplying != 0) return;
        try
        {
            if (!_linkEditor.TryGetMarkdownUpdateUI(out _)) return;
            if (!EnsureLinkContext(out var context)) return;
            if (!context.Editor.TryGetMarkdownUpdateUI(out _)) return;
            CaptureLinkCaret(context, false);
            CancelObsoleteViewport();
            WakeSourceLink();
        }
        catch (Exception error) { LoggingService.LogException(error); CancelSourceLink(); }
    }

    private void LinkSourceViewport(object sender, EventArgs args)
    {
        if (!FollowSource || LinkPreviewPane || _linkApplying != 0 ||
            (PreservingPreviewViewport && !_linkRestoredViewport.HasValue) ||
            !_linkEditor.TryGetMarkdownUpdateUI(out _)) return;
        _linkViewportResample = true;
        WakeSourceLink();
    }

    private void LinkSourceGeometry(object sender, EventArgs args)
    {
        if (_linkApplying != 0) return;
        try
        {
            _linkCoordinator.SourceGeometryChanged();
            // Reflow retires the admitted coordinates, not the current
            // explicit navigation's selection/reveal debt.
            _linkCoordinator.RetryCurrent();
            CancelObsoleteViewport();
            _linkViewportResample = true;
            WakeSourceLink();
        }
        catch (Exception error) { LoggingService.LogException(error); CancelSourceLink(); }
    }

    private void LinkSourceAvailability(object sender, EventArgs args)
    {
        CancelSourceLink();
        SourceLinkStateChanged();
    }

    private void LinkPreviewSelection(MarkdownView sender, MarkdownSelection selection)
    {
        if (_linkPublication is { } publication)
        {
            ++publication.Notifications;
            if (publication.Notifications == 1)
            {
                publication.First = ReadLinkSelection(selection);
                publication.MatchesAtNotification = publication.First == ReadLinkSelection(sender.GetSelection());
            }
            return;
        }
        if (_linkApplying != 0 || _restoringSessionViewport) return;
        if (!LinkPreviewPane && _linkView.FocusState == FocusState.Unfocused)
        {
            // A caller may select preview text without moving keyboard focus.
            // Its newer nonempty selection wins over ordinary source caret work;
            // explicit source Find and the viewport driver keep their authority.
            if (selection.IsAvailable && selection.Length > 0 && ReferenceEquals(sender, _linkView) &&
                ReadLinkSelection(selection) == ReadLinkSelection(sender.GetSelection()) &&
                TryReadLinkContext(out var current) && current.Document == selection.DocumentId &&
                current.Revision == selection.Revision && current.TextGeneration == selection.TextGeneration)
                _linkCoordinator.DiscardSourceCaret();
            return;
        }
        try
        {
            _linkOwnedMatch = null;
            if (_linkEditor.TryGetMarkdownSearchMatch(out _)) _linkEditor.ClearMarkdownSearchMatch();
            if (!LinkPreviewPane) SetLinkPane(true, LinkIntent.Focus);
            if (!EnsureLinkContext(out var context)) return;
            CaptureLinkCaret(context, false);
            CancelObsoleteViewport();
            WakeSourceLink();
        }
        catch (Exception error) { LoggingService.LogException(error); CancelSourceLink(); }
    }

    private void LinkPreviewViewport(MarkdownView sender, MarkdownViewportAnchor anchor)
    {
        if (_linkApplying != 0) return;
        try
        {
            if (!EnsureLinkContext(out var context)) return;
            bool wake = ObserveTargetLayout(context, anchor);
            if (PreservingPreviewViewport && _linkRestoredViewport is { } restored && AnchorCurrent(context, anchor))
            {
                if (anchor.LayoutGeneration != restored.Preview.LayoutGeneration)
                    _linkRestoredViewport = (restored.Source, anchor);
                else if (LinkPreviewPane && (anchor.PreviewUtf16Offset != restored.Preview.PreviewUtf16Offset ||
                    anchor.OffsetWithinLine != restored.Preview.OffsetWithinLine))
                    ReleasePreservedViewport();
            }
            if (LinkPreviewPane && !PreservingPreviewViewport && AnchorCurrent(context, anchor))
            {
                if (_linkCoordinator.ObservePreview(anchor.ScrollStartUtf16Offset, anchor.ScrollEndUtf16Offset,
                    anchor.ScrollProgress, anchor.LayoutGeneration))
                {
                    _linkCoordinator.BeginViewportIntent();
                    wake = true;
                }
                CancelObsoleteViewport();
                TryApplyCachedViewport(context);
            }
            if (wake) WakeSourceLink();
        }
        catch (Exception error) { LoggingService.LogException(error); CancelSourceLink(); }
    }

    private void CaptureLinkCaret(LinkContext context, bool navigate, bool force = false)
    {
        if (LinkPreviewPane)
        {
            // A wheel may take viewport authority while the source's search range stays selected.
            if (context.Editor.TryGetMarkdownSearchMatch(out _)) return;
            if (!FollowSource) return;
            var selection = context.View.GetSelection();
            if (selection.IsAvailable && selection.DocumentId == context.Document &&
                selection.Revision == context.Revision && selection.TextGeneration == context.TextGeneration)
                _linkCoordinator.ObserveCaret(selection.Start, selection.Length, navigate, force);
        }
        else if (context.Editor.TryGetMarkdownSearchMatch(out var match))
        {
            bool explicitFind = _linkSearchNavigate == match.Serial;
            _linkSearchNavigate = 0;
            _linkCoordinator.ObserveCaret(checked((ulong)match.Start), checked((ulong)(match.End - match.Start)),
                FollowSource && (navigate || explicitFind), force, match);
        }
        else if (FollowSource)
            _linkCoordinator.ObserveCaret(checked((ulong)context.Editor.MarkdownCaretByte), 0, navigate, force,
                sourceSelectionRevision: context.Editor.MarkdownSelectionRevision);
    }

    private void CaptureLinkViewport(LinkContext context)
    {
        if (!FollowSource || (PreservingPreviewViewport &&
            (LinkPreviewPane || !_linkRestoredViewport.HasValue))) return;
        if (LinkPreviewPane)
        {
            var anchor = context.View.GetViewportAnchor();
            if (AnchorCurrent(context, anchor))
                _linkCoordinator.ObservePreview(anchor.ScrollStartUtf16Offset, anchor.ScrollEndUtf16Offset,
                    anchor.ScrollProgress, anchor.LayoutGeneration);
        }
        else
        {
            if (_linkSampling != 0) { _linkViewportResample = true; return; }
            long epoch = _linkCoordinator.Epoch, geometry = _linkCoordinator.Geometry;
            ++_linkSampling;
            try
            {
                if (context.Editor.TryGetMarkdownViewport(out var sample) && epoch == _linkCoordinator.Epoch &&
                    geometry == _linkCoordinator.Geometry && sample.NativeRevision == context.Stamp.NativeRevision &&
                    sample.Length == context.Stamp.Length && sample.ContentVersion == context.Stamp.ContentVersion)
                {
                    // A restored pair is already aligned. Resume follow when the source actually moves,
                    // including caret keys that can scroll without a ScrollStarted event.
                    if (PreservingPreviewViewport && _linkRestoredViewport is { } restored)
                    {
                        if (MarkdownFollowCoordinator.SameViewport(restored.Source, sample)) return;
                        ReleasePreservedViewport();
                    }
                    var (progress, extent, viewportHeight) = context.Editor.MarkdownScrollMetrics;
                    // Ease the preview's scrollable inset out as the source leaves its fixed top.
                    double topRemaining = progress >= 0 && extent > 0
                        ? Math.Max(0, 1 - progress * extent / (2 * PreviewPadding))
                        : 0;
                    _linkCoordinator.ObserveSource(sample with
                    {
                        BottomBlend = MarkdownFollowCoordinator.BottomBlend(progress, extent, viewportHeight),
                        TopBlend = topRemaining * topRemaining
                    });
                }
            }
            finally
            {
                --_linkSampling;
                if (_linkViewportResample) WakeSourceLink();
            }
        }
    }

    private static bool AnchorCurrent(LinkContext context, MarkdownViewportAnchor anchor) => anchor.IsAvailable &&
        anchor.DocumentId == context.Document && anchor.Revision == context.Revision &&
        anchor.TextGeneration == context.TextGeneration;

    private bool ObserveTargetLayout(LinkContext context, MarkdownViewportAnchor anchor)
    {
        if (!AnchorCurrent(context, anchor)) { _linkTargetAvailable = false; return false; }
        bool changed = !_linkTargetAvailable || _linkTargetLayout != anchor.LayoutGeneration;
        _linkTargetAvailable = true;
        _linkTargetLayout = anchor.LayoutGeneration;
        if (_linkCoordinator.PreviewLayoutChanged(anchor.LayoutGeneration)) CancelObsoleteViewport();
        if (changed) _linkCoordinator.RetryCurrent();
        return changed;
    }

    private void CancelObsoleteViewport()
    {
        if (_linkCoordinator.ActiveViewportObsolete) CancelLinkOperation();
    }

    private void LinkPreviewScrollStarting(MarkdownView sender, object args) => SetLinkPane(true, LinkIntent.Scroll);

    private void LinkPreviewExtentChanged(MarkdownView sender, object args)
    {
        if (!_restoringSessionViewport || _linkApplying != 0 || args is not true) return;
        _linkCoordinator.RetryCurrent();
        WakeSourceLink();
    }

    private void WakeSourceLink()
    {
        if (!_linkRunning || !_active || _disposed || _linkRendering ||
            (!_linkCoordinator.HasPending && !_linkViewportResample)) return;
        _linkRendering = true;
        CompositionTarget.Rendering += LinkRendering;
    }

    private void StopLinkRendering()
    {
        if (!_linkRendering) return;
        _linkRendering = false;
        CompositionTarget.Rendering -= LinkRendering;
    }

    private void LinkRendering(object sender, object args)
    {
        // One shot: native completion or a new actual input/layout signal will wake us again.
        StopLinkRendering();
        try
        {
            if (!EnsureLinkContext(out var context)) return;
            bool resample = _linkViewportResample;
            _linkViewportResample = false;
            ObserveTargetLayout(context, context.View.GetViewportAnchor());
            if (resample)
            {
                CaptureLinkViewport(context);
                if (_restoringSessionViewport) _linkCoordinator.RetryCurrent();
            }
            CancelObsoleteViewport();
            TryApplyCachedViewport(context);
            if (_linkCoordinator.TryBegin(out var work, FollowSource))
            {
                var cancellation = new CancellationTokenSource();
                _linkCancellation = cancellation;
                _linkCompletion = MapSourceLinkAsync(context, work, cancellation);
            }
        }
        catch (Exception error) { LoggingService.LogException(error); CancelSourceLink(); }
    }

}
