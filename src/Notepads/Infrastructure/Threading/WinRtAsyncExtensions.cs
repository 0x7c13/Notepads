// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
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
        using var registration = cancellationToken.Register(operation.Cancel);
        await completion.ConfigureAwait(false);
    }

    internal static async Task<T> AsCompletionTask<T>(this IAsyncOperation<T> operation,
        CancellationToken cancellationToken)
    {
        var completion = operation.AsTask();
        using var registration = cancellationToken.Register(operation.Cancel);
        return await completion.ConfigureAwait(false);
    }

    internal static async Task AsCompletionTask<TProgress>(this IAsyncActionWithProgress<TProgress> operation,
        CancellationToken cancellationToken)
    {
        var completion = operation.AsTask();
        using var registration = cancellationToken.Register(operation.Cancel);
        await completion.ConfigureAwait(false);
    }
}
