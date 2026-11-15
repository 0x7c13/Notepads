// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Presentation.Controls.Markdown;
using Notepads.Presentation.Controls.TextEditor;

namespace Notepads.UnitTests;

public sealed class MarkdownFollowContinuityCoordinatorTests
{
    [Fact]
    public void SeveralPublishedPositionsInsideOnePreviewSpanNeedOnlyOneMapping()
    {
        var state = Preview();
        state.ObservePreview(80, 170, 0.1, 9);
        Assert.True(state.TryBegin(out var work));
        Assert.Equal(80UL, work.Offset);
        Assert.Equal(170UL, work.EndOffset);
        var pair = Pair(work);
        Assert.True(state.CompletePreview(work, pair, out var first));
        state.PreviewApplied(first, true);
        foreach (double progress in new[] { 0.2, 0.35, 0.6, 0.95 })
        {
            state.ObservePreview(80, 170, progress, 9);
            Assert.True(state.TryTakeCachedPreview(out var sample, out var cached));
            Assert.Equal(pair, cached);
            Assert.Equal(progress, sample.Progress);
            state.PreviewApplied(sample, true);
            Assert.False(state.TryBegin(out _));
        }
    }

    [Fact]
    public void PendingPreviewMappingReplaysTheLatestFractionOnlyForTheSameTwoEnds()
    {
        var state = Preview();
        state.ObservePreview(80, 170, 0.1, 9);
        Assert.True(state.TryBegin(out var work));
        state.ObservePreview(80, 170, 0.85, 9);
        Assert.False(state.TryBegin(out _));
        Assert.True(state.CompletePreview(work, Pair(work), out var replay));
        Assert.Equal(0.85, replay.Progress);
        state.PreviewApplied(replay, true);
        Assert.False(state.HasPending);
    }

    [Fact]
    public void AChangedPreviewEndCannotBorrowTheNewProgressForAnOldMappedSpan()
    {
        var state = Preview();
        state.ObservePreview(80, 170, 0.1, 9);
        Assert.True(state.TryBegin(out var old));
        state.ObservePreview(80, 180, 0.8, 9);
        Assert.True(state.CompletePreview(old, Pair(old), out var original));
        Assert.Equal(170UL, original.End);
        Assert.Equal(0.1, original.Progress);
        state.PreviewApplied(original, true);
        Assert.False(state.TryTakeCachedPreview(out _, out _));
        Assert.True(state.TryBegin(out var current));
        Assert.Equal(180UL, current.EndOffset);
    }

    [Fact]
    public void PreviewLayoutChangeRetiresOldWorkAndRequiresItsOwnPair()
    {
        var state = Preview();
        state.ObservePreview(80, 170, 0.3, 9);
        Assert.True(state.TryBegin(out var old));
        state.ObservePreview(80, 170, 0.7, 10);
        Assert.True(state.ActiveViewportObsolete);
        Assert.False(state.CompletePreview(old, Pair(old), out _));
        Assert.Null(state.PreviewPair);
        Assert.True(state.TryBegin(out var next));
        Assert.Equal(10UL, next.TargetLayout);
        Assert.True(state.CompletePreview(next, Pair(next), out var current));
        Assert.Equal(0.7, current.Progress);
    }

    [Fact]
    public void SourceReflowInvalidatesTheReverseGeometryEvenWithIdenticalMainEnds()
    {
        var state = Preview();
        InstallPreview(state, 0.4);
        state.SourceGeometryChanged();
        Assert.Null(state.PreviewPair);
        state.ObservePreview(80, 170, 0.4, 9);
        Assert.False(state.TryTakeCachedPreview(out _, out _));
        Assert.True(state.TryBegin(out var remap));
        Assert.True(state.CompletePreview(remap, Pair(remap), out _));
    }

    [Fact]
    public void AnOldReverseMappingCannotMoveAgainstTheNewWheelDirection()
    {
        var state = Preview();
        state.ObservePreview(80, 170, 0.2, 9);
        state.ObservePreview(80, 170, 0.7, 9);
        Assert.True(state.TryBegin(out var old));
        state.ObservePreview(80, 170, 0.4, 9);
        Assert.True(state.ActiveViewportObsolete);
        Assert.False(state.CompletePreview(old, Pair(old), out _));
        Assert.True(state.TryTakeCachedPreview(out var latest, out _));
        Assert.Equal(0.4, latest.Progress);
    }

    [Fact]
    public void ARejectedNativeAdmissionParksUntilARealLayoutSignal()
    {
        var state = Preview();
        InstallPreview(state, 0.4);
        state.ObservePreview(80, 170, 0.7, 9);
        Assert.True(state.TryTakeCachedPreview(out var sample, out _));
        state.PreviewApplied(sample, false);
        Assert.False(state.HasPending);
        Assert.False(state.TryBegin(out _));
        Assert.False(state.TryTakeCachedPreview(out _, out _));
        state.RetryCurrent();
        Assert.True(state.TryTakeCachedPreview(out var retry, out _));
        Assert.Equal(sample, retry);
    }

    [Fact]
    public void SameDirectionReentrantProgressInvalidatesTheOlderAdmissionCommit()
    {
        var state = Preview();
        InstallPreview(state, 0.1);
        var old = state.CaptureViewportDispatch();
        state.ObservePreview(80, 170, 0.2, 9);
        Assert.Equal(old.ScrollSerial, state.ScrollSerial);
        Assert.False(state.ViewportDispatchCurrent(old));
        Assert.True(state.ViewportDispatchCurrent(state.CaptureViewportDispatch()));
    }

    [Theory]
    [InlineData(0)]
    [InlineData(1)]
    [InlineData(2)]
    public void ExternalPaneGeometryOrCaretChangesInvalidateAViewportCommit(int change)
    {
        var state = Preview();
        InstallPreview(state, 0.1);
        var old = state.CaptureViewportDispatch();
        if (change == 0) state.Reset(MarkdownFollowPane.Source);
        else if (change == 1) state.SourceGeometryChanged();
        else state.ObserveCaret(120, 0, true);
        Assert.False(state.ViewportDispatchCurrent(old));
    }

    [Fact]
    public void RootFindNavigationRetainsPriorityOverCachedSourceFractionAndTargetReflow()
    {
        var state = new MarkdownFollowCoordinator();
        state.Reset(MarkdownFollowPane.Source);
        state.PreviewLayoutChanged(9);
        state.ObserveSource(new(0, 100, 0, 0.3, 1, 1000, 1));
        Assert.True(state.TryBegin(out var viewport));
        Assert.True(state.CompleteSource(viewport, new(0, 100, 0, 80), out var sample));
        state.SourceApplied(sample, true);
        var match = new MarkdownSearchMatch(1, 20, 25, 2, 3, 4, 5);
        state.ObserveCaret(20, 5, true, searchMatch: match);
        state.ObserveSource(new(0, 100, 0, 0.6, 1, 1000, 1));
        Assert.False(state.TryTakeCachedSource(out _, out _));
        Assert.True(state.TryBegin(out var found));
        state.PreviewLayoutChanged(10);
        Assert.True(state.CompleteCaret(found, out bool reveal));
        Assert.True(reveal);
        Assert.Equal(match, found.SearchMatch);
    }

    [Fact]
    public void AnObsoleteMappingStillOwnsTheOnlyNativeAdmissionUntilItRetires()
    {
        var state = Preview();
        state.ObservePreview(80, 170, 0.1, 9);
        Assert.True(state.TryBegin(out var old));
        state.Reset(MarkdownFollowPane.Source);
        state.ObserveSource(new(0, 100, 0, 0.2, 1, 1000, 1));
        Assert.False(state.TryBegin(out _));
        Assert.False(state.CompletePreview(old, Pair(old), out _));
        Assert.True(state.TryBegin(out var next));
        Assert.Equal(MarkdownFollowOperation.SourceViewport, next.Operation);
    }

    [Theory]
    [InlineData(double.NaN)]
    [InlineData(-0.01)]
    [InlineData(1.01)]
    public void PublishedPreviewProgressMustBeFiniteAndInsideTheSpan(double progress)
    {
        var state = Preview();
        Assert.Throws<ArgumentOutOfRangeException>(() => state.ObservePreview(80, 170, progress, 9));
        Assert.False(state.HasPending);
    }

    private static MarkdownFollowCoordinator Preview()
    {
        var state = new MarkdownFollowCoordinator();
        state.Reset(MarkdownFollowPane.Preview);
        return state;
    }

    private static MarkdownPreviewFollowPair Pair(MarkdownFollowWork work) =>
        new(work.Offset, work.EndOffset, 100, 200, work.TargetLayout);

    private static void InstallPreview(MarkdownFollowCoordinator state, double progress)
    {
        state.ObservePreview(80, 170, progress, 9);
        Assert.True(state.TryBegin(out var work));
        Assert.True(state.CompletePreview(work, Pair(work), out var sample));
        state.PreviewApplied(sample, true);
    }
}
