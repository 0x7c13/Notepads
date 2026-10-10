// Copyright (c) 2026, Jiaqi (0x7c13) Liu. See LICENSE.
// These implementations exist only in the source-linked editor test assembly.
using System;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Windows.Foundation;
using WinUIEditor;

namespace NotepadsEditorTests
{
    internal static class SessionRecoveryCancellationHooks
    {
        internal static Action<string, int, CancellationToken> BaselineRead;
        internal static Action<string, int, CancellationToken> LeasedRead;
        internal static Action<IAsyncOperation<EditorJournalCheckpoint>, CancellationToken> CheckpointOpening;
        internal static Action<EditorJournalCheckpoint, CancellationToken> CheckpointOpened;

        internal static void Reset()
        {
            BaselineRead = null;
            LeasedRead = null;
            CheckpointOpening = null;
            CheckpointOpened = null;
        }
    }
}

namespace Notepads.Features.Documents.Storage
{
    public sealed partial class DocumentBaseline
    {
        static partial void OnBaselineRead(string path, int byteCount, CancellationToken cancellationToken) =>
            NotepadsEditorTests.SessionRecoveryCancellationHooks.BaselineRead?.Invoke(path, byteCount, cancellationToken);
    }

    internal sealed partial class DocumentAssetLease
    {
        static partial void OnLeasedRead(string path, int byteCount, CancellationToken cancellationToken) =>
            NotepadsEditorTests.SessionRecoveryCancellationHooks.LeasedRead?.Invoke(path, byteCount, cancellationToken);

        internal static (string Path, int Leases, Task Retirement)[] TestRecoveryAssets(Guid owner)
        {
            var prefix = owner.ToString("N") + "-";
            lock (RegistrySync)
                return Registry.Values.Where(asset => asset.Path != null &&
                    Path.GetFileName(asset.Path).StartsWith(prefix, StringComparison.Ordinal))
                    .Select(asset => (asset.Path, asset.LeaseCount, (Task)asset.Deletion?.Task ?? Task.CompletedTask)).ToArray();
        }
    }

    public sealed partial class DocumentJournal
    {
        static partial void OnCheckpointOpening(IAsyncOperation<EditorJournalCheckpoint> operation, CancellationToken cancellationToken) =>
            NotepadsEditorTests.SessionRecoveryCancellationHooks.CheckpointOpening?.Invoke(operation, cancellationToken);

        static partial void OnCheckpointOpened(EditorJournalCheckpoint checkpoint, CancellationToken cancellationToken) =>
            NotepadsEditorTests.SessionRecoveryCancellationHooks.CheckpointOpened?.Invoke(checkpoint, cancellationToken);
    }
}
