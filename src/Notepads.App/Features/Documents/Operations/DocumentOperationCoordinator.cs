// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading;
using System.Threading.Tasks;

namespace Notepads.Features.Documents.Operations;

/// <summary>
/// Serializes document I/O and cancels queued work when its tab closes.
/// Resources remain alive until every operation has finished unwinding.
/// </summary>
internal sealed partial class DocumentOperationCoordinator : IDisposable
{
    private readonly object _sync = new();
    private readonly SemaphoreSlim _gate = new(1, 1);
    private readonly CancellationTokenSource _lifetime = new();
    private readonly TaskCompletionSource<object> _completion = new(
        TaskCreationOptions.RunContinuationsAsynchronously);
    private int _pendingOperations;
    private Observations _observations;
    private bool _disposed;

    /// <summary>Completes after disposal and every owned operation has unwound.</summary>
    public Task Completion => _completion.Task;

    /// <summary>Drains admitted work even if its tab is removed while a caller waits.</summary>
    public async Task WaitForIdleAsync()
    {
        try
        {
            await RunAsync(cancellation => Task.CompletedTask);
        }
        catch (OperationCanceledException)
        {
            await Completion;
        }
        catch (ObjectDisposedException)
        {
            await Completion;
        }
    }

    // A picker observation starts at the user's choice, before a queued save can change the destination.
    // Its cancellation is independent of a save already admitted to the serialized queue.
    public async Task<T> ObserveAsync<T>(Func<CancellationToken, Task<T>> observe, CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(observe);
        Observations batch;
        lock (_sync)
        {
            BeginOperation();
            batch = _observations ??= new Observations();
            batch.Count++;
        }
        try
        {
            using var linked = CancellationTokenSource.CreateLinkedTokenSource(_lifetime.Token, cancellationToken, batch.Cancellation.Token);
            linked.Token.ThrowIfCancellationRequested();
            return await observe(linked.Token).ConfigureAwait(false);
        }
        finally
        {
            try { await FinishObservationAsync(batch).ConfigureAwait(false); }
            finally { CompleteOperation(); }
        }
    }

    // A close decision must wait for the picker read to release its handle, even
    // when the storage provider needs time to respond to cancellation.
    public Task CancelObservationsAndWaitAsync()
    {
        lock (_sync)
        {
            var batch = _observations;
            if (batch == null) return Task.CompletedTask;
            batch.Canceling ??= batch.Cancellation.CancelAsync();
            return batch.Drained.Task;
        }
    }

    private async Task FinishObservationAsync(Observations batch)
    {
        Task canceling;
        lock (_sync)
        {
            if (--batch.Count != 0) return;
            canceling = batch.Canceling;
            _observations = null;
        }

        try
        {
            if (canceling != null) await canceling.ConfigureAwait(false);
        }
        finally
        {
            batch.Dispose();
            batch.Drained.TrySetResult();
        }
    }

    private sealed partial class Observations : IDisposable
    {
        internal readonly CancellationTokenSource Cancellation = new();
        internal readonly TaskCompletionSource Drained = new(TaskCreationOptions.RunContinuationsAsynchronously);
        internal int Count;
        internal Task Canceling;
        public void Dispose() => Cancellation.Dispose();
    }

    private void BeginOperation()
    {
        lock (_sync)
        {
            if (_disposed) throw new ObjectDisposedException(nameof(DocumentOperationCoordinator));
            _pendingOperations++;
        }
    }

    public Task RunAsync(Func<CancellationToken, Task> operation, CancellationToken cancellationToken = default) =>
        RunCoreAsync(operation, cancellationToken, waitForTurn: true);

    public Task TryRunAsync(Func<CancellationToken, Task> operation, CancellationToken cancellationToken) =>
        RunCoreAsync(operation, cancellationToken, waitForTurn: false);

    private async Task RunCoreAsync(Func<CancellationToken, Task> operation, CancellationToken cancellationToken, bool waitForTurn)
    {
        ArgumentNullException.ThrowIfNull(operation);
        BeginOperation();

        CancellationTokenSource cancellation = null;
        var acquired = false;
        try
        {
            cancellation = CancellationTokenSource.CreateLinkedTokenSource(_lifetime.Token, cancellationToken);
            if (waitForTurn) await _gate.WaitAsync(cancellation.Token);
            else if (!_gate.Wait(0, cancellation.Token)) return;
            acquired = true;
            cancellation.Token.ThrowIfCancellationRequested();
            await operation(cancellation.Token);
        }
        finally
        {
            if (acquired) _gate.Release();
            cancellation?.Dispose();
            CompleteOperation();
        }
    }

    public void Dispose()
    {
        lock (_sync)
        {
            if (_disposed) return;
            _disposed = true;
            // Cancellation callbacks may finish pending operations. Retain a
            // lease while cancellation runs, without invoking them under lock.
            _pendingOperations++;
        }

        try
        {
            _lifetime.Cancel();
        }
        finally
        {
            CompleteOperation();
        }
    }

    private void CompleteOperation()
    {
        lock (_sync)
        {
            _pendingOperations--;
            if (_disposed && _pendingOperations == 0)
            {
                _gate.Dispose();
                _lifetime.Dispose();
                _completion.TrySetResult(null);
            }
        }
    }
}
