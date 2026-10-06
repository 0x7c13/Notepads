// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Operations;

namespace NotepadsEditorTests;

internal static class DocumentOperationTests
{
    public static async Task RunAsync()
    {
        var order = new List<int>();
        using (var coordinator = new DocumentOperationCoordinator())
        {
            var finishFirst = NewSignal();
            var first = coordinator.RunAsync(async cancellation =>
            {
                order.Add(1);
                await finishFirst.Task;
                order.Add(2);
            });
            var second = coordinator.RunAsync(cancellation =>
            {
                order.Add(3);
                return Task.CompletedTask;
            });
            if (order.Count != 1 || second.IsCompleted)
                throw new Exception("A later save ran before the preceding save finished.");
            finishFirst.SetResult(true);
            await Task.WhenAll(first, second);
            if (order.Count != 3 || order[0] != 1 || order[1] != 2 || order[2] != 3)
                throw new Exception("Document saves completed out of order.");

            using (var cancellation = new CancellationTokenSource())
            {
                cancellation.Cancel();
                await ExpectAsync<OperationCanceledException>(coordinator.RunAsync(
                    token => throw new Exception("Canceled work was executed."), cancellation.Token));
            }
            await coordinator.RunAsync(cancellation => Task.CompletedTask);
            await coordinator.WaitForIdleAsync();
        }

        var closing = new DocumentOperationCoordinator();
        var finishCommit = NewSignal();
        CancellationToken activeCancellation = default;
        var commit = closing.RunAsync(async cancellation =>
        {
            activeCancellation = cancellation;
            // An already committed file write may need to finish unwinding.
            await finishCommit.Task;
        });
        var queued = closing.RunAsync(cancellation =>
            throw new Exception("A queued save ran after its tab closed."));
        var drain = closing.WaitForIdleAsync();
        closing.Dispose();
        closing.Dispose();
        if (!activeCancellation.IsCancellationRequested)
            throw new Exception("Closing the document did not cancel its active operation.");
        await ExpectAsync<OperationCanceledException>(queued);
        if (closing.Completion.IsCompleted)
            throw new Exception("Closing released the document lifetime before its committed write finished unwinding.");
        if (drain.IsCompleted)
            throw new Exception("The close barrier abandoned an active commit when its tab was removed.");
        await ExpectAsync<ObjectDisposedException>(closing.RunAsync(cancellation => Task.CompletedTask));
        finishCommit.SetResult(true);
        await commit;
        await closing.Completion;
        await drain;
        await closing.WaitForIdleAsync();
        Console.WriteLine("PASS: V2 document save ordering, queued cancellation, and closing during an active commit.");
    }

    private static TaskCompletionSource<bool> NewSignal() =>
        new(TaskCreationOptions.RunContinuationsAsynchronously);

    private static async Task ExpectAsync<TException>(Task operation) where TException : Exception
    {
        try
        {
            await operation;
        }
        catch (TException)
        {
            return;
        }
        throw new Exception("Expected " + typeof(TException).Name + ".");
    }
}
