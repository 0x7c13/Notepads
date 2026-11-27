// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using Windows.Foundation;

namespace Notepads.Infrastructure.Threading;

internal static class WinRtAsyncExtensions
{
    // AsTask(token) may cancel its task before the WinRT operation completes.
    // Owners of streams, document leases and admission gates must request
    // cancellation but await native completion before releasing those resources.
    internal static async Task AsCompletionTask(this IAsyncAction operation, CancellationToken cancellationToken)
    {
        var completion = operation.AsTask();
        using var registration = RegisterCancellation(operation, completion, cancellationToken);
        await completion.ConfigureAwait(false);
    }

    internal static async Task<T> AsCompletionTask<T>(this IAsyncOperation<T> operation,
        CancellationToken cancellationToken)
    {
        var completion = operation.AsTask();
        using var registration = RegisterCancellation(operation, completion, cancellationToken);
        return await completion.ConfigureAwait(false);
    }

    internal static async Task AsCompletionTask<TProgress>(this IAsyncActionWithProgress<TProgress> operation,
        CancellationToken cancellationToken)
    {
        var completion = operation.AsTask();
        using var registration = RegisterCancellation(operation, completion, cancellationToken);
        await completion.ConfigureAwait(false);
    }

    internal static async Task<TResult> AsCompletionTask<TResult, TProgress>(
        this IAsyncOperationWithProgress<TResult, TProgress> operation, CancellationToken cancellationToken)
    {
        var completion = operation.AsTask();
        using var registration = RegisterCancellation(operation, completion, cancellationToken);
        return await completion.ConfigureAwait(false);
    }

    private static CancellationTokenRegistration RegisterCancellation(IAsyncInfo operation, Task completion,
        CancellationToken cancellationToken)
    {
        if (completion.IsCompleted || !cancellationToken.CanBeCanceled) return default;
        return cancellationToken.Register(static state =>
        {
            try { ((IAsyncInfo)state).Cancel(); }
            // Match the SDK's cancellation bridge: an unavailable native endpoint
            // cannot accept Cancel, but its completion still owns the resources.
            catch (COMException error) when (error.HResult is unchecked((int)0x80010108) or
                unchecked((int)0x800706BA) or unchecked((int)0x89020001)) { }
        }, operation);
    }
}
