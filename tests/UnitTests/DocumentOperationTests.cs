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

    private static Task Bound(Task task) => task.WaitAsync(TimeSpan.FromSeconds(5), TestContext.Current.CancellationToken);

    private static TaskCompletionSource Signal() => new(TaskCreationOptions.RunContinuationsAsynchronously);
}
