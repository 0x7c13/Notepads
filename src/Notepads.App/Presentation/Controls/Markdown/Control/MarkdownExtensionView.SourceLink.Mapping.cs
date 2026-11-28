// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics.CodeAnalysis;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
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
    private async Task MapSourceLinkAsync(LinkContext context, MarkdownFollowWork work,
        CancellationTokenSource cancellation)
    {
        var token = cancellation.Token;
        try
        {
            if (token.IsCancellationRequested || !LinkContextCurrent(context)) return;
            if (work.SearchMatch is { } match)
            {
                await MapSourceMatchAsync(context, work, match, token);
                return;
            }
            if (!FollowSource) return;
            if (work.Operation == MarkdownFollowOperation.SourceViewport)
            {
                var mapped = await MapLinkSpan(context, checked((ulong)work.Source.StartByte),
                    checked((ulong)work.Source.EndByte), true, token);
                if (token.IsCancellationRequested || !LinkContextCurrent(context)) return;
                Task publicationWait = null;
                OnLinkSourceViewportMapped(work, ref publicationWait);
                if (publicationWait != null) await publicationWait.WaitAsync(token);
                if (token.IsCancellationRequested || !LinkContextCurrent(context)) return;
                if (_linkViewportResample)
                {
                    _linkViewportResample = false;
                    CaptureLinkViewport(context);
                }
                var pair = new MarkdownFollowPair(work.Source.StartByte, work.Source.EndByte,
                    mapped.Start.PreviewUtf16Offset, (mapped.IsDiscontinuous ? mapped.Start : mapped.End).PreviewUtf16Offset);
                if (_linkCoordinator.CompleteSource(work, pair, out var sample)) ApplySourceViewport(context, sample, pair);
            }
            else if (work.Operation == MarkdownFollowOperation.PreviewViewport)
            {
                var mapped = await MapLinkSpan(context, work.Offset, work.EndOffset, false, token);
                if (token.IsCancellationRequested || !LinkContextCurrent(context)) return;
                var pair = new MarkdownPreviewFollowPair(work.Offset, work.EndOffset,
                    checked((long)mapped.Start.SourceByteOffset),
                    checked((long)(mapped.IsDiscontinuous ? mapped.Start : mapped.End).SourceByteOffset), work.TargetLayout);
                if (_linkCoordinator.CompletePreview(work, pair, out var sample)) ApplyPreviewViewport(context, sample, pair);
            }
            else
            {
                bool source = work.Pane == MarkdownFollowPane.Source;
                var affinity = source && context.Editor.MarkdownIsLineStartByte(checked((long)work.Offset))
                    ? MarkdownPositionAffinity.After : MarkdownPositionAffinity.Before;
                var mapped = await MapLinkPosition(context, work.Offset, source, affinity, token);
                if (token.IsCancellationRequested || !LinkContextCurrent(context)) return;
                if (work.Operation == MarkdownFollowOperation.Caret)
                {
                    Task publicationWait = null;
                    OnLinkCaretMapped(work, ref publicationWait);
                    if (publicationWait != null) await publicationWait.WaitAsync(token);
                    if (token.IsCancellationRequested || !LinkContextCurrent(context)) return;
                    if (!LinkCaretCurrent(context, work) || !_linkCoordinator.CompleteCaret(work, out bool reveal)) return;
                    if (source)
                    {
                        if (!TryPublishLinkSelection(context, mapped.PreviewUtf16Offset, 0, out _))
                        {
                            // A nested external selection takes authority unless a newer
                            // source command already retired this publication.
                            if (LinkContextCurrent(context) && _linkCoordinator.CaretCurrent(work))
                                SetLinkPane(true, LinkIntent.Focus);
                            return;
                        }
                        if (!CanRevealLinkCaret(context, work, reveal, token) ||
                            !LinkCaretCurrent(context, work)) return;
                        ++_linkApplying;
                        try { context.View.RevealRange(context.Document, context.Revision, context.TextGeneration,
                            mapped.PreviewUtf16Offset, 0, false, false); }
                        finally { --_linkApplying; }
                    }
                    else
                    {
                        var sourceByte = checked((long)mapped.SourceByteOffset);
                        ++_linkApplying;
                        try { context.Editor.MarkdownSetCaretByte(sourceByte); }
                        finally { --_linkApplying; }
                        if (!CanRevealLinkCaret(context, work, reveal, token) || context.Editor.HasSelection ||
                            context.Editor.MarkdownCaretByte != sourceByte) return;
                        ++_linkApplying;
                        try { context.Editor.MarkdownRevealByte(sourceByte, false); }
                        finally { --_linkApplying; }
                    }
                }
            }
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested) { }
        catch (Exception error) { LoggingService.LogException(error); }
        finally
        {
            _linkCoordinator.Retire(work);
            if (ReferenceEquals(_linkCancellation, cancellation)) _linkCancellation = null;
            cancellation.Dispose();
            WakeSourceLink();
        }
    }

    private async Task<MarkdownMappedSpan> MapLinkSpan(LinkContext context, ulong start, ulong end, bool source,
        CancellationToken token)
    {
        token.ThrowIfCancellationRequested();
        var operation = source
            ? context.View.MapSourceSpanAsync(context.Document, context.Revision, context.TextGeneration,
                start, end, MarkdownPositionAffinity.After)
            : context.View.MapPreviewSpanAsync(context.Document, context.Revision, context.TextGeneration,
                start, end, MarkdownPositionAffinity.After);
        var mapped = await operation.AsCompletionTask(token);
        token.ThrowIfCancellationRequested();
        ValidateLinkPosition(context, mapped.Start);
        ValidateLinkPosition(context, mapped.End);
        return mapped;
    }

    private static void ValidateLinkPosition(LinkContext context, MarkdownMappedPosition mapped)
    {
        if (mapped.DocumentId != context.Document || mapped.Revision != context.Revision ||
            mapped.TextGeneration != context.TextGeneration || mapped.SourceByteOffset > checked((ulong)context.Stamp.Length) ||
            mapped.PreviewUtf16Offset > context.View.DisplayedTextLength)
            throw new InvalidDataException("A source-follow mapping does not belong to the admitted document.");
    }

    private async Task<MarkdownMappedPosition> MapLinkPosition(LinkContext context, ulong offset, bool source,
        MarkdownPositionAffinity affinity, CancellationToken token)
    {
        token.ThrowIfCancellationRequested();
        var operation = source
            ? context.View.MapSourcePositionAsync(context.Document, context.Revision, context.TextGeneration, offset, affinity)
            : context.View.MapPreviewPositionAsync(context.Document, context.Revision, context.TextGeneration, offset, affinity);
        var mapped = await operation.AsCompletionTask(token);
        token.ThrowIfCancellationRequested();
        ValidateLinkPosition(context, mapped);
        return mapped;
    }

    private async Task MapSourceMatchAsync(LinkContext context, MarkdownFollowWork work,
        MarkdownSearchMatch match, CancellationToken token)
    {
        if (!SourceMatchCurrent(context, work, match)) return;
        var first = checked((ulong)match.Start);
        var last = checked((ulong)match.End);
        MarkdownMappedPosition start, end;
        if (first == last)
        {
            start = end = await MapLinkPosition(context, first, true, MarkdownPositionAffinity.After, token);
        }
        else
        {
            var span = await MapLinkSpan(context, first, last, true, token);
            if (!SourceMatchCurrent(context, work, match)) return;
            start = end = span.Start;
            if (!span.IsDiscontinuous)
            {
                end = await MapLinkPosition(context, last, true, MarkdownPositionAffinity.Before, token);
                if (start.Correspondence is MarkdownCorrespondence.Atomic or MarkdownCorrespondence.Object)
                    start = await MapLinkPosition(context, first, true, MarkdownPositionAffinity.Before, token);
                if (end.Correspondence is MarkdownCorrespondence.Atomic or MarkdownCorrespondence.Object)
                    end = span.End;
            }
        }
        if (token.IsCancellationRequested || !SourceMatchCurrent(context, work, match) ||
            !_linkCoordinator.CompleteCaret(work, out bool reveal)) return;
        var offset = start.PreviewUtf16Offset;
        var length = end.PreviewUtf16Offset >= offset ? end.PreviewUtf16Offset - offset : 0;
        if (offset > context.View.DisplayedTextLength || length > context.View.DisplayedTextLength - offset)
            throw new InvalidDataException("The source match maps outside the displayed Main text.");
        if (!TryPublishLinkSelection(context, offset, length, out var applied))
        {
            // A nested selection belongs to the reentrant caller, never to this match.
            if (context.Editor.TryGetMarkdownSearchMatch(out var latest) && latest.Serial != match.Serial) return;
            _linkOwnedMatch = null;
            context.Editor.ClearMarkdownSearchMatch();
            if (context.Editor.TryGetMarkdownSearchMatch(out latest) && latest.Serial != match.Serial) return;
            SetLinkPane(true, LinkIntent.Focus);
            return;
        }
        if (!applied.Available || applied.Document != context.Document || applied.Revision != context.Revision ||
            applied.TextGeneration != context.TextGeneration) return;
        if (!SourceMatchProjectionCurrent(context, match))
        {
            // Clear/close can reenter before this first publication has an owned tuple.
            // Its notification proof permits folding A only while A is still current.
            if (_linkOwnedMatch == applied) _linkOwnedMatch = null;
            if (ReadLinkSelection(context.View.GetSelection()) == applied)
            {
                var dispatch = _linkCoordinator.CaptureViewportDispatch();
                var selectionRevision = context.Editor.MarkdownSelectionRevision;
                if (!TryPublishLinkSelection(context.View, applied with { Length = 0 }, out _) &&
                    dispatch == _linkCoordinator.CaptureViewportDispatch() &&
                    LinkContextCurrent(context) && selectionRevision == context.Editor.MarkdownSelectionRevision)
                    DiscardClearedSourceCaret(context, selectionRevision);
            }
            return;
        }
        _linkOwnedMatch = applied;
        // A newer pane/scroll intent may retire this work while the source match remains valid.
        // Retain its projection and ownership, but never revive the retired work's reveal.
        if (!CanRevealLinkCaret(context, work, reveal, token) ||
            !SourceMatchCurrent(context, work, match)) return;
        ++_linkApplying;
        try { context.View.RevealRange(context.Document, context.Revision, context.TextGeneration, offset, length, false, true); }
        finally { --_linkApplying; }
    }

    private bool TryPublishLinkSelection(LinkContext context, ulong offset, ulong length, out LinkSelection applied)
        => TryPublishLinkSelection(context.View,
            new(true, context.Document, context.Revision, context.TextGeneration, offset, length), out applied);

    private bool TryPublishLinkSelection(MarkdownView view, LinkSelection target, out LinkSelection applied)
    {
        var before = ReadLinkSelection(view.GetSelection());
        var publication = new LinkPublication();
        var previousPublication = _linkPublication;
        ++_linkApplying;
        _linkPublication = publication;
        try { view.SelectRange(target.Document, target.Revision, target.TextGeneration, target.Start, target.Length); }
        finally { _linkPublication = previousPublication; --_linkApplying; }
        applied = ReadLinkSelection(view.GetSelection());
        bool ownChange = publication.Notifications == 1 && publication.MatchesAtNotification && publication.First == applied;
        bool noChange = publication.Notifications == 0 && before == applied;
        return ownChange || noChange;
    }

    private bool CanRevealLinkCaret(LinkContext context, MarkdownFollowWork work, bool reveal, CancellationToken token) =>
        !token.IsCancellationRequested && reveal && FollowSource && LinkContextCurrent(context) &&
        _linkCoordinator.CanRevealCaret(work);

    private bool SourceMatchCurrent(LinkContext context, MarkdownFollowWork work, MarkdownSearchMatch match) =>
        LinkContextCurrent(context) && _linkCoordinator.CaretCurrent(work) &&
        context.Editor.TryGetMarkdownSearchMatch(out var current) && current == match;

    private bool SourceMatchProjectionCurrent(LinkContext context, MarkdownSearchMatch match) =>
        _linkRunning && _active && !_disposed && _enabled &&
        ReferenceEquals(_linkEditor, context.Editor) && ReferenceEquals(_linkView, context.View) &&
        ReferenceEquals(_session, context.Session) && context.Session.IsCurrent &&
        context.Session.DocumentId == context.Document && context.Session.AcceptedRevision == context.Revision &&
        context.Session.AcceptedStamp == context.Stamp &&
        context.Editor.TryGetMarkdownSearchMatch(out var current) && current == match;

    private static bool LinkCaretCurrent(LinkContext context, MarkdownFollowWork work)
    {
        // Native selection writes can advance the revision before UpdateUI runs.
        if (work.Pane == MarkdownFollowPane.Source) return !context.Editor.TryGetMarkdownSearchMatch(out _) &&
            checked((ulong)context.Editor.MarkdownCaretByte) == work.Offset &&
            context.Editor.MarkdownSelectionRevision == work.SourceSelectionRevision;
        var selection = context.View.GetSelection();
        return selection.IsAvailable && selection.DocumentId == context.Document && selection.Revision == context.Revision &&
            selection.TextGeneration == context.TextGeneration && selection.Start == work.Offset && selection.Length == work.Length;
    }

    private void TryApplyCachedViewport(LinkContext context)
    {
        if (!FollowSource || _linkApplying != 0 || !LinkContextCurrent(context)) return;
        if (_linkCoordinator.TryTakeCachedSource(out var source, out var sourcePair))
            ApplySourceViewport(context, source, sourcePair);
        else if (_linkCoordinator.TryTakeCachedPreview(out var preview, out var previewPair))
            ApplyPreviewViewport(context, preview, previewPair);
    }

    private bool LinkViewportDispatchCurrent(LinkContext context, MarkdownFollowDispatch dispatch) =>
        FollowSource && LinkContextCurrent(context) &&
        _linkCoordinator.ViewportDispatchCurrent(dispatch);

    private void ApplySourceViewport(LinkContext context, MarkdownViewportSample sample, MarkdownFollowPair pair)
    {
        var dispatch = _linkCoordinator.CaptureViewportDispatch();
        if (dispatch.Pane != MarkdownFollowPane.Source || !LinkViewportDispatchCurrent(context, dispatch)) return;
        bool restoring = _restoringSessionViewport;
        if (restoring && (_linkViewportResample || !_linkCoordinator.IsCurrentSourceViewport(sample))) return;
        var anchor = context.View.GetViewportAnchor();
        if (!LinkViewportDispatchCurrent(context, dispatch)) return;
        if (AnchorCurrent(context, anchor) && anchor.LayoutGeneration != _linkTargetLayout)
        {
            ObserveTargetLayout(context, anchor);
            WakeSourceLink();
            return;
        }
        bool admitted = false;
        if (AnchorCurrent(context, anchor))
        {
            ++_linkApplying;
            try
            {
                if (restoring)
                {
                    bool atTop = sample.DisplayRow == 0 && sample.Fraction == 0;
                    admitted = context.View.TryRestoreFollowViewport(context.Document, context.Revision,
                        context.TextGeneration, anchor.LayoutGeneration, atTop ? 0 : pair.Start, atTop ? 0 : pair.End,
                        sample.Fraction, atTop ? 1 : sample.TopBlend, sample.BottomBlend);
                }
                else if (sample.BottomBlend == 1) admitted = context.View.TrySetVerticalScrollProgress(1);
                else if (sample.DisplayRow == 0 && sample.Fraction == 0) admitted = context.View.TrySetVerticalScrollProgress(0);
                else admitted = context.View.SetFollowViewportBetweenPositions(context.Document, context.Revision,
                    context.TextGeneration, anchor.LayoutGeneration, pair.Start, pair.End, sample.Fraction,
                    sample.TopBlend, sample.BottomBlend);
            }
            finally { --_linkApplying; }
        }
        // A native callback may replace the context, intent or geometry during admission.
        if (!LinkViewportDispatchCurrent(context, dispatch)) return;
        var after = context.View.GetViewportAnchor();
        if (!LinkViewportDispatchCurrent(context, dispatch)) return;
        if (!AnchorCurrent(context, after))
        {
            _linkTargetAvailable = false;
            _linkCoordinator.SourceApplied(sample, false);
            return;
        }
        if (after.LayoutGeneration != anchor.LayoutGeneration)
        {
            ObserveTargetLayout(context, after);
            WakeSourceLink();
            return;
        }
        if (!admitted) _linkTargetAvailable = false;
        _linkCoordinator.SourceApplied(sample, admitted);
        if (restoring && admitted && !_linkViewportResample && _linkCoordinator.IsCurrentSourceViewport(sample))
        {
            _linkPreservedViewportRevision = context.Revision;
            _linkRestoredViewport = (sample, after);
            FinishSessionViewportRestore();
        }
    }

    private void ApplyPreviewViewport(LinkContext context, MarkdownPreviewViewport sample, MarkdownPreviewFollowPair pair)
    {
        var dispatch = _linkCoordinator.CaptureViewportDispatch();
        if (dispatch.Pane != MarkdownFollowPane.Preview || !LinkViewportDispatchCurrent(context, dispatch)) return;
        var anchor = context.View.GetViewportAnchor();
        if (!LinkViewportDispatchCurrent(context, dispatch)) return;
        if (!AnchorCurrent(context, anchor))
        {
            _linkTargetAvailable = false;
            _linkCoordinator.PreviewApplied(sample, false);
            return;
        }
        if (anchor.LayoutGeneration != pair.Layout)
        {
            ObserveTargetLayout(context, anchor);
            CaptureLinkViewport(context);
            WakeSourceLink();
            return;
        }
        bool admitted;
        ++_linkApplying;
        try
        {
            var progress = context.View.VerticalScrollProgress;
            admitted = progress is 0 or 1
                ? context.Editor.MarkdownSetScrollEndpoint(progress)
                : context.Editor.MarkdownSetViewportBetweenSourcePositions(pair.StartByte, pair.EndByte, sample.Progress);
        }
        finally { --_linkApplying; }
        if (!LinkViewportDispatchCurrent(context, dispatch)) return;
        var after = context.View.GetViewportAnchor();
        if (!LinkViewportDispatchCurrent(context, dispatch)) return;
        if (!AnchorCurrent(context, after))
        {
            _linkTargetAvailable = false;
            _linkCoordinator.PreviewApplied(sample, false);
            return;
        }
        if (after.LayoutGeneration != pair.Layout)
        {
            ObserveTargetLayout(context, after);
            CaptureLinkViewport(context);
            WakeSourceLink();
            return;
        }
        // false parks until the core reports a subsequent real paint/layout signal.
        _linkCoordinator.PreviewApplied(sample, admitted);
    }
}
