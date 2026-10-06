// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Windows.System;

namespace Notepads.Features.Sessions.Recovery;

/// <summary>Bounds independent restore preparation while keeping document order and drain ownership.</summary>
internal static class RecoveryRestorePipeline
{
    private const ulong ParallelDocumentBytes = 32 * 1024 * 1024;

    public static async Task RunAsync(IReadOnlyList<PreparedRecoveryDocument> documents,
        Func<int, PreparedRecoveryDocument, Task> restore, CancellationToken cancellationToken,
        Func<bool> hasMemoryHeadroom = null)
    {
        for (var index = 0; index < documents.Count;)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var pair = index + 1 < documents.Count && CanParallel(documents[index]) && CanParallel(documents[index + 1]) &&
                (hasMemoryHeadroom?.Invoke() ?? MemoryManager.AppMemoryUsageLevel == AppMemoryUsageLevel.Low);
            var first = Invoke(restore, index, documents[index]);
            if (pair)
            {
                // Both callbacks are asynchronous on the caller's UI apartment.
                // Native loaders perform their independent document work off UI.
                await Task.WhenAll(first, Invoke(restore, index + 1, documents[index + 1]));
                index += 2;
            }
            else { await first; index++; }
        }
    }

    private static Task Invoke(Func<int, PreparedRecoveryDocument, Task> restore, int index,
        PreparedRecoveryDocument document)
    {
        // A synchronous callback failure must still drain the other loader
        // before the caller can release its recovery pins.
        try { return restore(index, document) ?? Task.FromException(new InvalidOperationException("The restore callback returned no task.")); }
        catch (Exception error) { return Task.FromException(error); }
    }

    private static bool CanParallel(PreparedRecoveryDocument document)
    {
        if (document.InitializeFromFile) return false;
        var baseline = (ulong)(document.RecoveryBaseline ?? document.SavedSnapshot.Baseline).ByteLength;
        var prefix = document.Checkpoint?.CommittedByteLength ?? 0;
        // Insert payloads bound possible replay growth, even when the final
        // document is small. This is a scheduling hint, not a native heap bound.
        return baseline <= ParallelDocumentBytes && prefix <= ParallelDocumentBytes - baseline &&
            (document.Checkpoint?.CommittedDocumentByteLength ?? baseline) <= ParallelDocumentBytes;
    }
}
