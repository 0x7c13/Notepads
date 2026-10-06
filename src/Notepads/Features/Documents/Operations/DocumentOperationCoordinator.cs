// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading;
using System.Threading.Tasks;

namespace Notepads.Features.Documents.Operations;

/// <summary>
/// Serializes document I/O and cancels queued work when its tab closes.
/// Resources remain alive until every operation has finished unwinding.
/// </summary>
internal sealed class DocumentOperationCoordinator : IDisposable
{
    private readonly object _sync = new();
    private readonly SemaphoreSlim _gate = new(1, 1);
    private readonly CancellationTokenSource _lifetime = new();
    private readonly TaskCompletionSource<object> _completion = new(
        TaskCreationOptions.RunContinuationsAsynchronously);
    private int _pendingOperations;
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

    public async Task RunAsync(Func<CancellationToken, Task> operation,
        CancellationToken cancellationToken = default)
    {
        if (operation == null) throw new ArgumentNullException(nameof(operation));
        lock (_sync)
        {
            if (_disposed) throw new ObjectDisposedException(nameof(DocumentOperationCoordinator));
            _pendingOperations++;
        }

        CancellationTokenSource cancellation = null;
        var acquired = false;
        try
        {
            cancellation = CancellationTokenSource.CreateLinkedTokenSource(_lifetime.Token, cancellationToken);
            await _gate.WaitAsync(cancellation.Token);
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
