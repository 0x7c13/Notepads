// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Storage;
using Windows.Storage;

namespace Notepads.Features.Sessions.Transfer;

internal enum WindowMoveEvidence { Absent, Received, Uncertain }

internal static class WindowMoveStore
{
    internal const int MaximumRequestBytes = 4096;

    private static string DirectoryFor(TransferToken token)
    {
        token.Validate();
        // Transient transport is separate from the strictly versioned recovery
        // catalog. It is never a recovery root or a source of document authority.
        return Path.Combine(ApplicationData.Current.LocalFolder.Path, "WindowMoves",
            token.SourceOwnerId.ToString("N"), token.TransferId.ToString("N"));
    }

    internal static string StatePath(TransferToken token) => Path.Combine(DirectoryFor(token), "state.bin");
    private static string RequestPath(TransferToken token) => Path.Combine(DirectoryFor(token), "request.json");

    internal static FileStream CreateState(TransferToken token)
    {
        Directory.CreateDirectory(DirectoryFor(token));
        return new FileStream(StatePath(token), FileMode.CreateNew, FileAccess.Write, FileShare.None, 65536,
            FileOptions.Asynchronous | FileOptions.SequentialScan);
    }

    internal static FileStream OpenState(TransferToken token) => new(StatePath(token), FileMode.Open,
        FileAccess.Read, FileShare.Read, 65536, FileOptions.Asynchronous | FileOptions.SequentialScan);

    internal static async Task PublishAsync(WindowMoveRequest request, CancellationToken cancellation)
    {
        request.Validate();
        var bytes = JsonSerializer.SerializeToUtf8Bytes(request, SessionJsonContext.Default.WindowMoveRequest);
        if (bytes.Length > MaximumRequestBytes) throw new InvalidDataException("The move request is too large.");
        using (await SessionRecoveryTransaction.EnterAsync(cancellation))
        {
            if (new FileInfo(StatePath(request.Source)).Length != request.StateBytes)
                throw new InvalidDataException("The captured live-state length changed.");
            await TransferRootReader.ReadSourceTransferAsync(request.Source, cancellation);
            if (await ReadEvidenceAsync(request.Source, cancellation) != WindowMoveEvidence.Absent)
                throw new InvalidDataException("This transfer already has receiving evidence.");
            var path = RequestPath(request.Source);
            var staging = path + ".pending";
            // No reader admits a partially written request. All names are derived
            // from validated identities, never from URI paths or serialized text.
            await using (var file = new FileStream(staging, FileMode.CreateNew, FileAccess.Write, FileShare.None,
                4096, FileOptions.Asynchronous))
            {
                await file.WriteAsync(bytes, cancellation);
                file.Flush(flushToDisk: true);
            }
            cancellation.ThrowIfCancellationRequested();
            File.Move(staging, path, overwrite: false);
        }
    }

    // Caller holds store admission, and a source-scope reader while using the state.
    internal static async Task<WindowMoveRequest> ReadPendingAsync(TransferToken token, CancellationToken cancellation)
    {
        await using var file = new FileStream(RequestPath(token), FileMode.Open, FileAccess.Read, FileShare.Read,
            4096, FileOptions.Asynchronous);
        if (file.Length is < 1 or > MaximumRequestBytes) throw new InvalidDataException("Invalid move request size.");
        var bytes = new byte[(int)file.Length];
        await file.ReadExactlyAsync(bytes, cancellation);
        var request = JsonSerializer.Deserialize(bytes, SessionJsonContext.Default.WindowMoveRequest);
        if (request == null) throw new InvalidDataException("Missing move request.");
        request.Validate();
        if (!token.Matches(request.Source) || await ReadEvidenceAsync(token, cancellation) != WindowMoveEvidence.Absent)
            throw new InvalidDataException("The move request is mismatched or already received.");
        return request;
    }

    internal static async Task RequirePendingAsync(WindowMoveRequest expected, CancellationToken cancellation)
    {
        if (!expected.Matches(await ReadPendingAsync(expected.Source, cancellation)))
            throw new InvalidDataException("The pending move request changed.");
    }

    // Unlike HasReceiptAsync, this examines durable evidence even if its target
    // closed/reset. Unreadable evidence must never be treated as permission to replay.
    private static async Task<WindowMoveEvidence> ReadEvidenceAsync(TransferToken token, CancellationToken cancellation)
    {
        try
        {
            foreach (var kind in new[] { RecoveryRecordKind.TransferReceipt, RecoveryRecordKind.SourceMoved })
            {
                try { await RecoveryRootStore.ReadTransferAsync(token.TransferId, kind, cancellation); return WindowMoveEvidence.Received; }
                catch (FileNotFoundException) { }
            }
            return WindowMoveEvidence.Absent;
        }
        catch (Exception ex) when (ex is IOException or InvalidDataException or UnauthorizedAccessException or JsonException)
        {
            return WindowMoveEvidence.Uncertain;
        }
    }

    internal static async Task<WindowMoveEvidence> FenceAsync(TransferToken token, CancellationToken cleanupCancellation)
    {
        using (await SessionRecoveryTransaction.EnterAsync(cleanupCancellation))
        {
            var evidence = await ReadEvidenceAsync(token, cleanupCancellation);
            // Retirement races receipt publication under this same admission. It
            // is independent of content cleanup, including when evidence is corrupt.
            File.Delete(RequestPath(token));
            return evidence;
        }
    }

    internal static async Task<bool> IsPendingAsync(TransferToken token, CancellationToken cancellation)
    {
        using (await SessionRecoveryTransaction.EnterAsync(cancellation))
            return File.Exists(RequestPath(token));
    }

    internal static async Task TryCleanupAsync(TransferToken token, CancellationToken cancellation)
    {
        using (await SessionRecoveryTransaction.EnterAsync(cancellation))
        using (var readers = SessionScopeLease.TryAcquireExclusiveReaders(token.SourceOwnerId))
        {
            if (readers == null || File.Exists(RequestPath(token))) return;
            var directory = DirectoryFor(token);
            if (Directory.Exists(directory)) Directory.Delete(directory, recursive: true);
        }
    }

    internal static Task CleanupInactiveAsync() => Task.Run(async () =>
    {
        var root = Path.Combine(ApplicationData.Current.LocalFolder.Path, "WindowMoves");
        if (!Directory.Exists(root)) return;
        foreach (var directory in Directory.EnumerateDirectories(root))
        {
            if (!Guid.TryParseExact(Path.GetFileName(directory), "N", out var owner) || owner == Guid.Empty) continue;
            using (await SessionRecoveryTransaction.EnterAsync())
            using (var writer = SessionScopeLease.TryAcquireInactiveWriter(owner))
            using (var readers = SessionScopeLease.TryAcquireExclusiveReaders(owner))
            {
                // An inactive source and no import reader prove no process can
                // still publish/consume these transient files. Durable content
                // roots remain entirely under the existing recovery collector.
                if (writer != null && readers != null && Directory.Exists(directory)) Directory.Delete(directory, recursive: true);
            }
        }
    });
}
