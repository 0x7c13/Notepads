// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Features.Documents.Operations;

namespace Notepads.UnitTests;

public sealed class DocumentOperationTests
{
    [Fact]
    public async Task AdmittedDocumentOperationsRunInOrder()
    {
        using var coordinator = new DocumentOperationCoordinator();
        var order = new List<int>();
        var finishFirst = Signal();
        var first = coordinator.RunAsync(async cancellation =>
        {
            order.Add(1);
            await finishFirst.Task;
            order.Add(2);
        }, TestContext.Current.CancellationToken);
        var second = coordinator.RunAsync(cancellation =>
        {
            order.Add(3);
            return Task.CompletedTask;
        }, TestContext.Current.CancellationToken);
        try
        {
            Assert.Equal(new[] { 1 }, order);
            Assert.False(second.IsCompleted);
        }
        finally { finishFirst.TrySetResult(); }
        await Task.WhenAll(first, second).WaitAsync(TimeSpan.FromSeconds(5), TestContext.Current.CancellationToken);
        Assert.Equal(new[] { 1, 2, 3 }, order);
        await Bound(coordinator.WaitForIdleAsync());
    }

    [Fact]
    public async Task CanceledOperationNeverRunsAndDoesNotBlockSubsequentWork()
    {
        using var coordinator = new DocumentOperationCoordinator();
        using var cancellation = CancellationTokenSource.CreateLinkedTokenSource(TestContext.Current.CancellationToken);
        cancellation.Cancel();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => Bound(coordinator.RunAsync(
            token => throw new InvalidOperationException("Canceled operation executed."), cancellation.Token)));
        var ran = false;
        await Bound(coordinator.RunAsync(token => { ran = true; return Task.CompletedTask; }, TestContext.Current.CancellationToken));
        Assert.True(ran);
        await Bound(coordinator.WaitForIdleAsync());
    }

    [Fact]
    public async Task FaultedOperationReleasesTheQueueForLaterWork()
    {
        using var coordinator = new DocumentOperationCoordinator();
        await Assert.ThrowsAsync<InvalidOperationException>(() => Bound(coordinator.RunAsync(
            token => throw new InvalidOperationException("Injected operation failure."), TestContext.Current.CancellationToken)));
        var ran = false;
        await Bound(coordinator.RunAsync(token => { ran = true; return Task.CompletedTask; }, TestContext.Current.CancellationToken));
        Assert.True(ran);
        await Bound(coordinator.WaitForIdleAsync());
    }

    [Fact]
    public async Task CloseCancelsQueuedWorkButKeepsTheLifetimeUntilAnActiveCommitUnwinds()
    {
        using var closing = new DocumentOperationCoordinator();
        var finishCommit = Signal();
        CancellationToken activeCancellation = default;
        var commit = closing.RunAsync(async cancellation =>
        {
            activeCancellation = cancellation;
            await finishCommit.Task;
        }, TestContext.Current.CancellationToken);
        var queued = closing.RunAsync(cancellation =>
            throw new InvalidOperationException("Queued operation ran after close."), TestContext.Current.CancellationToken);
        var drain = closing.WaitForIdleAsync();
        try
        {
            closing.Dispose();
            closing.Dispose();
            Assert.True(activeCancellation.IsCancellationRequested);
            await Assert.ThrowsAnyAsync<OperationCanceledException>(() => Bound(queued));
            Assert.False(closing.Completion.IsCompleted);
            Assert.False(drain.IsCompleted);
            await Assert.ThrowsAsync<ObjectDisposedException>(() => Bound(closing.RunAsync(token => Task.CompletedTask, TestContext.Current.CancellationToken)));
        }
        finally { finishCommit.TrySetResult(); }
        await Task.WhenAll(commit, closing.Completion, drain).WaitAsync(TimeSpan.FromSeconds(5), TestContext.Current.CancellationToken);
        await Bound(closing.WaitForIdleAsync());
    }

    [Fact]
    public async Task CloseDrainsCanceledObservationsWithoutCancelingAnAdmittedWrite()
    {
        using var coordinator = new DocumentOperationCoordinator();
        var releaseRead = Signal();
        var canceled = Signal();
        var releaseWrite = Signal();
        var write = coordinator.RunAsync(async token =>
        {
            await releaseWrite.Task;
            token.ThrowIfCancellationRequested();
        });
        var read = coordinator.ObserveAsync(async token =>
        {
            using var registration = token.Register(() => canceled.TrySetResult());
            await releaseRead.Task;
            token.ThrowIfCancellationRequested();
            return 0;
        }, TestContext.Current.CancellationToken);
        try
        {
            var drain = coordinator.CancelObservationsAndWaitAsync();
            await Bound(canceled.Task);
            Assert.False(drain.IsCompleted);
            Assert.False(write.IsCompleted);
            releaseRead.SetResult();
            await Bound(drain);
            await Assert.ThrowsAnyAsync<OperationCanceledException>(() => Bound(read));
            Assert.False(write.IsCompleted);
            Assert.Equal(1, await coordinator.ObserveAsync(_ => Task.FromResult(1), TestContext.Current.CancellationToken));
        }
        finally { releaseRead.TrySetResult(); releaseWrite.TrySetResult(); }
        await Bound(write);
    }

    [Fact]
    public async Task DisposalAndCloseBothWaitForAnActiveObservationToReleaseItsResources()
    {
        using var coordinator = new DocumentOperationCoordinator();
        var release = Signal();
        var canceled = Signal();
        var read = coordinator.ObserveAsync(async token =>
        {
            using var registration = token.Register(() => canceled.TrySetResult());
            await release.Task;
            token.ThrowIfCancellationRequested();
            return true;
        }, TestContext.Current.CancellationToken);
        var drain = coordinator.CancelObservationsAndWaitAsync();
        try
        {
            coordinator.Dispose();
            await Bound(canceled.Task);
            Assert.False(coordinator.Completion.IsCompleted);
            Assert.False(drain.IsCompleted);
        }
        finally { release.TrySetResult(); }
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => Bound(read));
        await Bound(Task.WhenAll(drain, coordinator.Completion));
    }

    [Fact]
    public async Task TryRunSkipsAnOccupiedQueueAndProtectsItsAcceptedOperation()
    {
        using var coordinator = new DocumentOperationCoordinator();
        var release = Signal();
        var accepted = coordinator.TryRunAsync(_ => release.Task, TestContext.Current.CancellationToken);
        try
        {
            await Bound(coordinator.TryRunAsync(_ => throw new InvalidOperationException("An occupied gate admitted a poll."),
                TestContext.Current.CancellationToken));
            Assert.False(accepted.IsCompleted);
            Assert.False(coordinator.WaitForIdleAsync().IsCompleted);
        }
        finally { release.TrySetResult(); }
        await Bound(accepted);
        await Bound(coordinator.WaitForIdleAsync());
    }

    private static Task Bound(Task task) => task.WaitAsync(TimeSpan.FromSeconds(5), TestContext.Current.CancellationToken);

    private static TaskCompletionSource Signal() => new(TaskCreationOptions.RunContinuationsAsynchronously);
}
