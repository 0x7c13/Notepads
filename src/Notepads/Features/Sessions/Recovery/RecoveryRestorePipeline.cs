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
        var active = new List<Task>(2);
        try
        {
            for (var index = 0; index < documents.Count;)
            {
                // Observe every completed peer before admitting another row. WhenAny
                // can report success even when another loader has already failed.
                for (var pending = active.Count - 1; pending >= 0; pending--)
                {
                    if (!active[pending].IsCompleted) continue;
                    await active[pending];
                    active.RemoveAt(pending);
                }
                cancellationToken.ThrowIfCancellationRequested();
                if (active.Count == 2)
                {
                    await Task.WhenAny(active);
                    continue;
                }

                var parallel = CanParallel(documents[index]);
                var headroom = hasMemoryHeadroom?.Invoke() ?? MemoryManager.AppMemoryUsageLevel == AppMemoryUsageLevel.Low;
                if (active.Count != 0 && (!parallel || !headroom))
                {
                    await Task.WhenAll(active);
                    active.Clear();
                    continue;
                }

                // Both callbacks are asynchronous on the caller's UI apartment.
                // Native loaders perform their independent document work off UI.
                active.Add(Invoke(restore, index, documents[index]));
                index++;
                if (!parallel || !headroom)
                {
                    await Task.WhenAll(active);
                    active.Clear();
                }
            }
            await Task.WhenAll(active);
            cancellationToken.ThrowIfCancellationRequested();
        }
        catch
        {
            // Admission checks can fail too. Keep ownership of every loader until
            // it finishes, then preserve the original failure for the caller.
            try { await Task.WhenAll(active); }
            catch { }
            throw;
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
