// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Presentation.Controls.Markdown;
using Notepads.Presentation.Controls.TextEditor;

namespace Notepads.UnitTests;

public sealed class MarkdownFollowCoordinatorTests
{
    [Theory]
    [InlineData(0, 2000, 400, 0)]
    [InlineData(0.7, 2000, 400, 0)]
    [InlineData(0.8, 2000, 400, 0)]
    [InlineData(0.85, 2000, 400, 0.0625)]
    [InlineData(0.9, 2000, 400, 0.25)]
    [InlineData(0.95, 2000, 400, 0.5625)]
    [InlineData(1, 2000, 400, 1)]
    [InlineData(0.5, 100, 400, 0.25)]
    [InlineData(0.95, -1, 400, 0)]
    public void BottomBlendEasesAcrossTheLastViewport(double progress, double extent, double height, double expected)
    {
        Assert.Equal(expected, MarkdownFollowCoordinator.BottomBlend(progress, extent, height), 10);
    }

    [Fact]
    public void StartingAScrollWaitsForTheNextViewportSample()
    {
        var state = NewSource();
        state.ObserveCaret(20, 0, true);
        state.ObserveSource(new(0, 20, 0, 0, 1, 100, 1));
        Assert.True(state.TryBegin(out var caret));

        state.BeginViewportIntent(retry: false);
        Assert.True(state.CompleteCaret(caret, out bool reveal));
        Assert.False(reveal);
        Assert.False(state.TryBegin(out _));

        state.ObserveSource(new(20, 40, 1, 0, 1, 100, 1));
        Assert.True(state.TryBegin(out var viewport));
        Assert.Equal(MarkdownFollowOperation.SourceViewport, viewport.Operation);
        Assert.Equal(20L, viewport.Source.StartByte);
    }

    [Fact]
    public void AViewportIntentRetiresCaretRevealButKeepsItsSelectionSynchronization()
    {
        var state = NewSource();
        state.ObserveCaret(1000, 0, true);
        Assert.True(state.TryBegin(out var caret));
        state.BeginViewportIntent();
        state.ObserveSource(Sample(4, 0.25));
        Assert.False(state.TryBegin(out _));
        Assert.True(state.CompleteCaret(caret, out bool reveal));
        Assert.False(reveal);
        Assert.True(state.TryBegin(out var viewport));
        Assert.Equal(MarkdownFollowOperation.SourceViewport, viewport.Operation);
        Assert.True(state.CompleteSource(viewport, Pair(viewport), out var sample));
        state.SourceApplied(sample, true);
        Assert.False(state.HasPending);
    }

    [Fact]
    public void TheSameViewportNotificationAfterSelectionDoesNotStealCaretNavigation()
    {
        var state = NewSource();
        var initial = Sample(3, 0.4);
        Apply(state, initial);
        state.ObserveCaret(120, 0, true);
        long serial = state.ScrollSerial;
        Assert.False(state.ObserveSource(initial));
        Assert.Equal(serial, state.ScrollSerial);
        Assert.True(state.TryBegin(out var caret));
        Assert.Equal(MarkdownFollowOperation.Caret, caret.Operation);
        Assert.True(state.CompleteCaret(caret, out bool reveal));
        Assert.True(reveal);
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public void RestoredStateHasOneViewportAuthorityIndependentOfSelectionAdmissionOrder(bool caretFirst)
    {
        var state = NewSource();
        if (caretFirst) state.ObserveCaret(1000, 0, false, true);
        state.ObserveSource(Sample(7, 0.5));
        if (!caretFirst) state.ObserveCaret(1000, 0, false, true);
        state.BeginViewportIntent();
        int selections = 0, viewports = 0;
        while (state.TryBegin(out var work))
        {
            if (work.Operation == MarkdownFollowOperation.Caret)
            {
                Assert.True(state.CompleteCaret(work, out bool reveal));
                Assert.False(reveal);
                ++selections;
            }
            else
            {
                Assert.True(state.CompleteSource(work, Pair(work), out var sample));
                state.SourceApplied(sample, true);
                ++viewports;
            }
        }
        Assert.Equal((1, 1), (selections, viewports));
    }

    [Fact]
    public void EightSlowMappingsPublishDuringContinuousCrossRowInputAndThenReachTheLatest()
    {
        var state = NewSource();
        state.ObserveSource(Sample(1, 0.2));
        var published = new List<long>();
        for (int index = 0; index < 8; ++index)
        {
            Assert.True(state.TryBegin(out var active));
            state.ObserveSource(Sample(index + 2, 0.7));
            Assert.False(state.ActiveViewportObsolete);
            Assert.False(state.TryBegin(out _));
            Assert.True(state.CompleteSource(active, Pair(active), out var admitted));
            // Each result uses its own row/fraction, never the latest different row's fraction.
            Assert.Equal(active.Source, admitted);
            state.SourceApplied(admitted, true);
            published.Add(admitted.DisplayRow);
        }
        Assert.Equal(Enumerable.Range(1, 8).Select(value => (long)value), published);
        Assert.True(state.TryBegin(out var final));
        Assert.True(state.CompleteSource(final, Pair(final), out var latest));
        Assert.Equal((9L, 0.7), (latest.DisplayRow, latest.Fraction));
        state.SourceApplied(latest, true);
        Assert.False(state.HasPending);
        Assert.Null(state.Active);
    }

    [Fact]
    public void SeveralFractionsInTheSamePairReuseOneSemanticMapping()
    {
        var state = NewSource();
        Apply(state, Sample(4, 0.1));
        foreach (double fraction in new[] { 0.25, 0.5, 0.75, 1.0 })
        {
            state.ObserveSource(Sample(4, fraction));
            Assert.True(state.TryTakeCachedSource(out var sample, out var pair));
            Assert.Equal(fraction, sample.Fraction);
            Assert.Equal((400UL, 500UL), (pair.Start, pair.End));
            state.SourceApplied(sample, true);
            Assert.False(state.TryBegin(out _));
        }
        state.ObserveSource(Sample(5, 0));
        Assert.True(state.TryBegin(out var boundary));
        Assert.Equal(500L, boundary.Source.StartByte);
        Assert.Equal(500UL, state.Pair.Value.End);
    }

    [Fact]
    public void LatestFractionCanReplaceAnAdmittedFractionOnlyInsideTheSameRowPair()
    {
        var state = NewSource();
        state.ObserveSource(Sample(2, 0.2));
        Assert.True(state.TryBegin(out var active));
        state.ObserveSource(Sample(2, 0.8));
        Assert.True(state.CompleteSource(active, Pair(active), out var latest));
        Assert.Equal(0.8, latest.Fraction);
        state.SourceApplied(latest, true);
        Assert.False(state.HasPending);
    }

    [Fact]
    public void AReversalInvalidatesTheOldDirectionWithoutOpeningASecondAdmission()
    {
        var state = NewSource();
        Apply(state, Sample(3, 0));
        state.ObserveSource(Sample(5, 0.3));
        Assert.True(state.TryBegin(out var forward));
        state.ObserveSource(Sample(4, 0.9));
        Assert.True(state.ActiveViewportObsolete);
        Assert.False(state.TryBegin(out _));
        Assert.False(state.CompleteSource(forward, Pair(forward), out _));
        Assert.True(state.TryBegin(out var reverse));
        Assert.True(state.CompleteSource(reverse, Pair(reverse), out var sample));
        Assert.Equal(4L, sample.DisplayRow);
    }

    [Fact]
    public void TargetReflowRetainsTheLastIntentAndNeedsNoNewSourceInput()
    {
        var state = NewSource();
        Apply(state, Sample(6, 0.6));
        Assert.False(state.HasPending);
        state.RetryCurrent();
        Assert.True(state.TryTakeCachedSource(out var sample, out _));
        Assert.Equal((6L, 0.6), (sample.DisplayRow, sample.Fraction));
        state.SourceApplied(sample, true);
        Assert.False(state.HasPending);
    }

    [Fact]
    public void UnavailableTargetParksWithoutLosingTheLastSampleOrPolling()
    {
        var state = NewSource();
        state.ObserveSource(Sample(2, 0.5));
        Assert.True(state.TryBegin(out var active));
        Assert.True(state.CompleteSource(active, Pair(active), out var sample));
        state.SourceApplied(sample, false);
        Assert.False(state.HasPending);
        Assert.False(state.TryBegin(out _));
        state.RetryCurrent();
        Assert.True(state.TryTakeCachedSource(out var retry, out _));
        Assert.Equal(sample, retry);
    }

    [Fact]
    public void SourceReflowRetiresOldCoordinatesButKeepsSemanticEndpoints()
    {
        var state = NewSource();
        Apply(state, Sample(3, 0.4));
        var semantic = state.Pair;
        state.ObserveSource(Sample(4, 0.2));
        Assert.True(state.TryBegin(out var oldLayout));
        state.SourceGeometryChanged();
        Assert.True(state.ActiveViewportObsolete);
        Assert.False(state.CompleteSource(oldLayout, Pair(oldLayout), out _));
        Assert.NotNull(state.Pair);
        var reflowed = Sample(9, 0.6) with { StartByte = 400, EndByte = 500 };
        state.ObserveSource(reflowed);
        Assert.True(state.TryTakeCachedSource(out var current, out _));
        Assert.Equal(9L, current.DisplayRow);
        Assert.Equal(0.6, current.Fraction);
    }

    [Fact]
    public void StrongResetRetiresTheOldOperationBeforeAdmittingItsReplacement()
    {
        var state = NewSource();
        state.ObserveSource(Sample(2, 0));
        Assert.True(state.TryBegin(out var old));
        state.Reset(MarkdownFollowPane.Source);
        state.ObserveSource(Sample(7, 0.3));
        Assert.True(state.ActiveViewportObsolete);
        Assert.False(state.TryBegin(out _));
        Assert.False(state.CompleteSource(old, Pair(old), out _));
        Assert.Null(state.Pair);
        Assert.True(state.TryBegin(out var current));
        Assert.False(state.Retire(old));
        Assert.Equal(current, state.Active);
        Assert.True(state.CompleteSource(current, Pair(current), out var sample));
        Assert.Equal(7L, sample.DisplayRow);
    }

    [Fact]
    public void ContinuousCaretAndViewportUpdatesBothMakeBoundedProgress()
    {
        var state = NewSource();
        int carets = 0, viewports = 0;
        for (int step = 1; step <= 12; ++step)
        {
            state.ObserveCaret((ulong)step, 0, false);
            state.ObserveSource(Sample(step, 0.3));
            Assert.True(state.TryBegin(out var work));
            if (work.Operation == MarkdownFollowOperation.Caret)
            {
                Assert.True(state.CompleteCaret(work, out bool reveal));
                Assert.False(reveal);
                ++carets;
            }
            else
            {
                Assert.True(state.CompleteSource(work, Pair(work), out var sample));
                state.SourceApplied(sample, true);
                ++viewports;
            }
        }
        Assert.Equal((6, 6), (carets, viewports));
    }

    [Fact]
    public void ReversePreviewViewportAlsoKeepsOneAdmissionAndMonotonicOwnProgress()
    {
        var state = new MarkdownFollowCoordinator();
        state.Reset(MarkdownFollowPane.Preview);
        state.ObservePreview(30, 35, 0.3, 1);
        Assert.True(state.TryBegin(out var first));
        state.ObservePreview(40, 45, 0.5, 1);
        Assert.False(state.ActiveViewportObsolete);
        Assert.True(state.CompletePreview(first, new(first.Offset, first.EndOffset, 30, 35, first.TargetLayout), out var firstSample));
        Assert.Equal((30UL, 35UL, 1UL), (firstSample.Start, firstSample.End, firstSample.Layout));
        state.PreviewApplied(firstSample, true);
        Assert.True(state.TryBegin(out var final));
        Assert.Equal(40UL, final.Offset);
        Assert.True(state.CompletePreview(final, new(final.Offset, final.EndOffset, 40, 45, final.TargetLayout), out var finalSample));
        Assert.Equal((40UL, 45UL, 0.5, 1UL), (finalSample.Start, finalSample.End, finalSample.Progress, finalSample.Layout));
    }

    [Fact]
    public void EmptyAndTerminalRowsKeepARealZeroLengthSemanticInterval()
    {
        var state = NewSource();
        state.ObserveSource(new(0, 0, 0, 0, 1, 0, 1));
        Assert.True(state.TryBegin(out var empty));
        Assert.True(state.CompleteSource(empty, new(0, 0, 0, 0), out var sample));
        state.SourceApplied(sample, true);
        Assert.False(state.HasPending);
    }

    [Fact]
    public void InvalidFractionsAndSourceBoundsCannotEnterTheScheduler()
    {
        var state = NewSource();
        foreach (var invalid in new[] { Sample(1, double.NaN), Sample(1, double.PositiveInfinity),
            Sample(1, -0.1), Sample(1, 1.1), Sample(1, 0) with { EndByte = 10001 },
            Sample(1, 0) with { EndByte = 0 }, Sample(1, 0) with { DisplayRow = -1 } })
            Assert.Throws<ArgumentOutOfRangeException>(() => state.ObserveSource(invalid));
        Assert.False(state.HasPending);
        Assert.Null(state.Active);
    }

    [Fact]
    public void CaretAutoScrollAndRenderResamplingKeepTheCaretRevealAuthority()
    {
        var state = NewSource();
        Apply(state, Sample(1, 0));
        state.ObserveCaret(9000, 0, true);
        long serial = state.ScrollSerial;
        state.ObserveSource(Sample(70, 0.2));
        state.ObserveSource(Sample(69, 0.8));
        Assert.Equal(serial, state.ScrollSerial);
        Assert.Equal(MarkdownFollowKind.Caret, state.Kind);
        Assert.True(state.TryBegin(out var caret));
        Assert.Equal(MarkdownFollowOperation.Caret, caret.Operation);
        Assert.True(state.CompleteCaret(caret, out bool reveal));
        Assert.True(reveal);
        Assert.False(state.TryBegin(out _));
    }

    [Fact]
    public void AnExplicitScrollUsesTheLatestPassiveSampleAndKeepsSelectionDebt()
    {
        var state = NewSource();
        Apply(state, Sample(1, 0));
        state.ObserveCaret(9000, 0, true);
        state.ObserveSource(Sample(70, 0.2));
        state.BeginViewportIntent();
        Assert.True(state.TryBegin(out var work));
        if (work.Operation == MarkdownFollowOperation.Caret)
        {
            Assert.True(state.CompleteCaret(work, out bool reveal));
            Assert.False(reveal);
            Assert.True(state.TryBegin(out work));
        }
        Assert.Equal(MarkdownFollowOperation.SourceViewport, work.Operation);
        Assert.Equal(70L, work.Source.DisplayRow);
        Assert.True(state.CompleteSource(work, Pair(work), out var sample));
        state.SourceApplied(sample, true);
    }

    [Fact]
    public void SourceReflowRetriesTheCurrentCaretWithoutGrantingViewportAuthority()
    {
        var state = NewSource();
        Apply(state, Sample(1, 0));
        state.ObserveCaret(9000, 0, true);
        Assert.True(state.TryBegin(out var old));
        state.SourceGeometryChanged();
        state.ObserveSource(Sample(5, 0.6));
        state.RetryCurrent();
        Assert.True(state.CompleteCaret(old, out bool oldReveal));
        Assert.False(oldReveal);
        Assert.True(state.TryBegin(out var current));
        Assert.Equal(MarkdownFollowOperation.Caret, current.Operation);
        Assert.True(state.CompleteCaret(current, out bool reveal));
        Assert.True(reveal);
        Assert.False(state.TryBegin(out _));
    }

    [Fact]
    public void PassivePreviewResamplingAlsoKeepsCaretNavigation()
    {
        var state = new MarkdownFollowCoordinator();
        state.Reset(MarkdownFollowPane.Preview);
        state.ObservePreview(100, 110, 0, 1);
        state.ObserveCaret(9000, 0, true);
        long serial = state.ScrollSerial;
        state.ObservePreview(7000, 7010, 0.2, 1);
        Assert.Equal(serial, state.ScrollSerial);
        Assert.True(state.TryBegin(out var caret));
        Assert.True(state.CompleteCaret(caret, out bool reveal));
        Assert.True(reveal);
        Assert.False(state.TryBegin(out _));
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void ExternalPreviewSelectionDiscardsQueuedOrdinarySourceCaret(bool navigate)
    {
        var state = NewSource();
        state.ObserveCaret(10, 0, navigate);
        long epoch = state.Epoch, geometry = state.Geometry;
        Assert.True(state.DiscardSourceCaret());
        Assert.Equal(MarkdownFollowPane.Source, state.Pane);
        Assert.Equal(MarkdownFollowKind.Viewport, state.Kind);
        Assert.Equal((epoch, geometry), (state.Epoch, state.Geometry));
        state.RetryCurrent();
        state.ObserveCaret(10, 0, false);
        Assert.False(state.TryBegin(out _));

        // Explicit Go To at the same source position is still a new command.
        state.ObserveCaret(10, 0, true, force: true);
        Assert.True(state.TryBegin(out var next));
        Assert.True(state.CompleteCaret(next, out bool reveal));
        Assert.True(reveal);
    }

    [Fact]
    public void ExternalPreviewSelectionRejectsHeldCaretWithoutStealingViewportOrAdmission()
    {
        var state = NewSource();
        state.ObserveCaret(10, 0, false);
        state.ObserveSource(Sample(4, 0.25));
        Assert.True(state.TryBegin(out var old));
        Assert.Equal(MarkdownFollowOperation.Caret, old.Operation);
        var dispatch = state.CaptureViewportDispatch();

        Assert.True(state.DiscardSourceCaret());
        Assert.Equal(dispatch, state.CaptureViewportDispatch());
        Assert.Equal(old, state.Active);
        Assert.False(state.CaretCurrent(old));
        Assert.False(state.TryBegin(out _));
        Assert.False(state.CompleteCaret(old, out bool reveal));
        Assert.False(reveal);
        Assert.True(state.TryBegin(out var viewport));
        Assert.Equal(MarkdownFollowOperation.SourceViewport, viewport.Operation);
        Assert.True(state.CompleteSource(viewport, Pair(viewport), out var sample));
        state.SourceApplied(sample, true);
        Assert.False(state.HasPending);
        state.ObserveCaret(11, 0, false);
        Assert.True(state.TryBegin(out var next));
        Assert.True(state.CompleteCaret(next, out _));
    }

    [Fact]
    public void ExternalPreviewSelectionDoesNotRetireAnExplicitSourceFind()
    {
        var state = NewSource();
        var match = new MarkdownSearchMatch(1, 10, 16, 1, 1, 1, 1);
        state.ObserveCaret(10, 6, true, searchMatch: match);
        Assert.True(state.TryBegin(out var work));
        Assert.False(state.DiscardSourceCaret());
        Assert.True(state.CaretCurrent(work));
        Assert.Equal(match, work.SearchMatch);
        Assert.True(state.CompleteCaret(work, out bool reveal));
        Assert.True(reveal);
    }

    [Fact]
    public void SourceCaretDiscardDoesNotRetirePreviewDrivenSelectionWork()
    {
        var state = new MarkdownFollowCoordinator();
        state.Reset(MarkdownFollowPane.Preview);
        state.ObserveCaret(40, 6, true);
        Assert.True(state.TryBegin(out var work));
        Assert.False(state.DiscardSourceCaret());
        Assert.True(state.CompleteCaret(work, out bool reveal));
        Assert.True(reveal);
    }

    private static MarkdownFollowCoordinator NewSource()
    {
        var state = new MarkdownFollowCoordinator();
        state.Reset(MarkdownFollowPane.Source);
        return state;
    }

    private static MarkdownViewportSample Sample(long row, double fraction) =>
        new(row * 100, (row + 1) * 100, row, fraction, 1, 10000, 1);

    private static MarkdownFollowPair Pair(MarkdownFollowWork work) =>
        new(work.Source.StartByte, work.Source.EndByte, (ulong)work.Source.StartByte, (ulong)work.Source.EndByte);

    private static void Apply(MarkdownFollowCoordinator state, MarkdownViewportSample sample)
    {
        state.ObserveSource(sample);
        Assert.True(state.TryBegin(out var work));
        Assert.True(state.CompleteSource(work, Pair(work), out var applied));
        state.SourceApplied(applied, true);
    }
}
