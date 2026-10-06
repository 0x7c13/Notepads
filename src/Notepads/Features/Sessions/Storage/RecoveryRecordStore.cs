// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Validation;
using Notepads.Infrastructure.Diagnostics;

namespace Notepads.Features.Sessions.Storage;

/// <summary>
/// Immutable metadata persistence. Callers hold global store admission while allocating,
/// publishing, repairing or deleting. This class does not interpret workspace authority.
/// </summary>
internal sealed class RecoveryRecordStore
{
    private readonly IRecoveryStorageFaults faults;

    public RecoveryRecordStore(string rootPath, IRecoveryStorageFaults faults = null)
    {
        Paths = new RecoveryStoragePaths(rootPath);
        this.faults = faults;
    }

    public RecoveryStoragePaths Paths { get; }

    public RecoveryStamp CreatePublicationStamp(RecoveryArea area, RecoveryStamp epoch)
    {
        ArgumentNullException.ThrowIfNull(epoch);
        return new RecoveryStamp
        {
            ScopeId = epoch.ScopeId,
            EpochId = epoch.EpochId,
            Ordinal = AllocateOrdinal(area),
            OperationId = Guid.NewGuid()
        };
    }

    public RecoveryArea GetScopeArea(Guid scopeId, RecoveryAreaKind kind, Guid documentId = default)
    {
        var area = new RecoveryArea(kind, scopeId, documentId);
        RecoveryStoragePaths.ValidateArea(area);
        if (area.AllowsForeignOwners) throw new ArgumentException("Use a global area for multi-owner roots.", nameof(kind));
        return area;
    }

    public RecoveryArea GetGlobalArea(RecoveryAreaKind kind, Guid rootId = default)
    {
        var area = new RecoveryArea(kind, Guid.Empty, RootId: rootId);
        RecoveryStoragePaths.ValidateArea(area);
        if (!area.AllowsForeignOwners) throw new ArgumentException("Use a scope area for local roots.", nameof(kind));
        return area;
    }

    public ulong AllocateOrdinal(RecoveryArea area)
    {
        var scan = Enumerate(area);
        if (!scan.IsComplete) throw new IOException("Cannot allocate recovery order from an incomplete directory scan.", scan.Error);
        var occupied = scan.HighestOccupiedOrdinal;
        if (occupied == ulong.MaxValue) throw new InvalidDataException("Recovery ordering is exhausted.");
        return occupied + 1;
    }

    public RecoveryDirectoryScan<RecoveryAddress> Enumerate(RecoveryArea area)
    {
        var addresses = new HashSet<RecoveryAddress>();
        var unknown = new List<string>();
        ulong occupied = 0;
        try
        {
            var path = Paths.AreaPath(area);
            foreach (var entry in Directory.EnumerateFileSystemEntries(path))
            {
                Paths.EnsureSafePath(entry);
                if (IsDirectory(entry))
                {
                    if (area.DocumentId == Guid.Empty && area.Kind is RecoveryAreaKind.Rescue or RecoveryAreaKind.Pending &&
                        TryIdentity(Path.GetFileName(entry), out _))
                    {
                        continue;
                    }

                    if (area.AllowsForeignOwners && area.RootId == Guid.Empty && TryIdentity(Path.GetFileName(entry), out _)) continue;
                    unknown.Add(entry);
                }
                else if (RecoveryStoragePaths.TryParseFileName(area, Path.GetFileName(entry), out var address, out var copy, out var temporary))
                {
                    occupied = Math.Max(occupied, address.Ordinal);
                    if (copy == "intent" && area.Kind != RecoveryAreaKind.Decisions) unknown.Add(entry);
                    else if (!temporary) addresses.Add(address);
                }
                else
                {
                    unknown.Add(entry);
                }
            }
            var collision = addresses.GroupBy(address => address.Ordinal).FirstOrDefault(group => group.Count() > 1);
            var error = collision == null ? null : new InvalidDataException("Distinct recovery operations occupy the same ordinal.");
            return new(addresses.OrderBy(address => address.Ordinal).ThenBy(address => address.OperationId).ToArray(), unknown, error, occupied);
        }
        catch (DirectoryNotFoundException) when (addresses.Count == 0 && unknown.Count == 0) { return new(Array.Empty<RecoveryAddress>(), Array.Empty<string>()); }
        catch (Exception error) when (IsStorageFailure(error))
        {
            return new(addresses.ToArray(), unknown, error, occupied);
        }
    }

    public RecoveryDirectoryScan<Guid> EnumerateScopeIds()
    {
        var scopes = new List<Guid>();
        var unknown = new List<string>();
        try
        {
            var path = Paths.ScopesPath;
            foreach (var entry in Directory.EnumerateFileSystemEntries(path))
            {
                Paths.EnsureSafePath(entry);
                if (IsDirectory(entry) && TryIdentity(Path.GetFileName(entry), out var scope)) scopes.Add(scope);
                else unknown.Add(entry);
            }
            return new(scopes, unknown);
        }
        catch (DirectoryNotFoundException) when (scopes.Count == 0 && unknown.Count == 0) { return new(Array.Empty<Guid>(), Array.Empty<string>()); }
        catch (Exception error) when (IsStorageFailure(error)) { return new(scopes, unknown, error); }
    }

    public RecoveryDirectoryScan<RecoveryArea> EnumerateScopeAreas(Guid scopeId, RecoveryAreaKind kind)
    {
        var root = GetScopeArea(scopeId, kind);
        if (kind is not (RecoveryAreaKind.Rescue or RecoveryAreaKind.Pending)) return new(new RecoveryArea[] { root }, Array.Empty<string>());
        return EnumerateChildAreas(root, identity => GetScopeArea(scopeId, kind, identity));
    }

    public RecoveryDirectoryScan<RecoveryArea> EnumerateGlobalAreas(RecoveryAreaKind kind)
    {
        var root = GetGlobalArea(kind);
        return EnumerateChildAreas(root, identity => GetGlobalArea(kind, identity));
    }

    /// <summary>Report roots outside known protocol directories instead of silently losing their references.</summary>
    public RecoveryDirectoryScan<string> ScanLayout(Guid? scopeId = null)
    {
        var entries = new List<string>();
        var unknown = new List<string>();
        try
        {
            var path = scopeId.HasValue ? Paths.ScopePath(scopeId.Value) : Paths.RootPath;
            foreach (var entry in Directory.EnumerateFileSystemEntries(path))
            {
                Paths.EnsureSafePath(entry);
                var name = Path.GetFileName(entry);
                var directory = IsDirectory(entry);
                var known = scopeId.HasValue ?
                    directory && name is "Checkpoints" or "Decisions" or "Rescue" or "Pending" ||
                    !directory && name is "writer.lock" or "readers.lock" :
                    directory && name is "Scopes" or "Transfers" or "Archives" || !directory && name == "store.lock";
                if (known) entries.Add(entry);
                else unknown.Add(entry);
            }
            return new(entries, unknown);
        }
        catch (DirectoryNotFoundException) when (entries.Count == 0 && unknown.Count == 0) { return new(Array.Empty<string>(), Array.Empty<string>()); }
        catch (Exception error) when (IsStorageFailure(error)) { return new(entries, unknown, error); }
    }

    public async Task<RecoveryPublicationResult> PublishAsync(RecoveryArea area, RecoveryRecord record, CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var address = new RecoveryAddress(area, record?.Stamp?.Ordinal ?? 0, record?.Stamp?.OperationId ?? Guid.Empty);
        var encoded = RecoveryRecordCodec.Encode(record, address);
        return await PublishEncodedAsync(address, encoded, cancellationToken).ConfigureAwait(false);
    }

    private async Task<RecoveryPublicationResult> PublishEncodedAsync(RecoveryAddress address, RecoveryEncodedRecord encoded,
        CancellationToken cancellationToken)
    {
        Exception failure = null;
        try
        {
            await WriteCopyAsync(address, "a", encoded.EnvelopeBytes, cancellationToken).ConfigureAwait(false);
            // Once A may exist, cancellation and mirror errors must be reconciled instead of undoing assets.
            await WriteCopyAsync(address, "b", encoded.EnvelopeBytes, cancellationToken).ConfigureAwait(false);
            return new(RecoveryPublicationState.Committed, address, encoded.PayloadSha256, RecoveryRedundancy.Mirrored);
        }
        catch (Exception error) when (IsPublicationFailure(error)) { failure = error; }
        var observed = await ReadAsync(address, cancellationToken: CancellationToken.None).ConfigureAwait(false);
        return Reconcile(address, encoded.PayloadSha256, observed.State, observed.PayloadSha256, observed.Redundancy, failure ?? observed.Error);
    }

    public async Task<RecoveryReadResult> ReadAsync(RecoveryAddress address, bool repair = false, CancellationToken cancellationToken = default)
    {
        RecoveryStoragePaths.ValidateAddress(address);
        var a = await ReadCopyAsync(address, "a", cancellationToken).ConfigureAwait(false);
        var b = await ReadCopyAsync(address, "b", cancellationToken,
            a.State == RecoveryReadState.Valid ? a.Encoded : null).ConfigureAwait(false);
        if (a.State == RecoveryReadState.Valid && b.State == RecoveryReadState.Valid)
        {
            if (!SamePayload(a.Encoded, b.Encoded))
                return new(RecoveryReadState.Conflict, address, Error: new InvalidDataException("Conflicting complete recovery mirrors."));
            return Valid(address, a.Encoded, RecoveryRedundancy.Mirrored);
        }
        if (a.State == RecoveryReadState.Unreadable || b.State == RecoveryReadState.Unreadable)
            return new(RecoveryReadState.Unreadable, address, Error: a.Error ?? b.Error);
        var verified = a.State == RecoveryReadState.Valid ? a : b.State == RecoveryReadState.Valid ? b : null;
        if (verified != null)
        {
            var peer = ReferenceEquals(verified, a) ? b : a;
            if (repair)
            {
                try
                {
                    cancellationToken.ThrowIfCancellationRequested();
                    if (peer.State == RecoveryReadState.Corrupt) DeleteCopy(address, peer.Copy);
                    await WriteCopyAsync(address, peer.Copy, verified.Encoded.EnvelopeBytes, cancellationToken).ConfigureAwait(false);
                    return Valid(address, verified.Encoded, RecoveryRedundancy.Repaired);
                }
                catch (Exception error) when (IsPublicationFailure(error))
                {
                    // A verified surviving copy remains committed when maintenance fails.
                    return Valid(address, verified.Encoded, RecoveryRedundancy.Single, error);
                }
            }
            return Valid(address, verified.Encoded, RecoveryRedundancy.Single, peer.Error);
        }
        if (a.State == RecoveryReadState.Absent && b.State == RecoveryReadState.Absent)
            return new(RecoveryReadState.Absent, address);
        return new(RecoveryReadState.Corrupt, address, Error: a.Error ?? b.Error);
    }

    public async Task<RecoveryPublicationResult> PublishResetIntentAsync(RecoveryArea area, RecoveryRecord record, CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (area.Kind != RecoveryAreaKind.Decisions || record?.Kind != RecoveryRecordKind.Reset)
            throw new ArgumentException("Only a scope-local Reset decision can publish an intent.", nameof(record));
        var address = new RecoveryAddress(area, record.Stamp?.Ordinal ?? 0, record.Stamp?.OperationId ?? Guid.Empty);
        var encoded = RecoveryRecordCodec.Encode(record, address);
        try
        {
            await WriteCopyAsync(address, "intent", encoded.EnvelopeBytes, cancellationToken).ConfigureAwait(false);
            return new(RecoveryPublicationState.Committed, address, encoded.PayloadSha256, RecoveryRedundancy.Single);
        }
        catch (Exception error) when (IsPublicationFailure(error))
        {
            var read = await ReadResetIntentAsync(address, CancellationToken.None).ConfigureAwait(false);
            return Reconcile(address, encoded.PayloadSha256, read.State, read.PayloadSha256, RecoveryRedundancy.Single, error);
        }
    }

    public async Task<RecoveryIntentResult> ReadResetIntentAsync(RecoveryAddress address, CancellationToken cancellationToken = default)
    {
        RecoveryStoragePaths.ValidateAddress(address);
        if (address.Area.Kind != RecoveryAreaKind.Decisions) throw new ArgumentException("Reset intent is scope-local.", nameof(address));
        var copy = await ReadCopyAsync(address, "intent", cancellationToken).ConfigureAwait(false);
        if (copy.State == RecoveryReadState.Valid && copy.Encoded.Record.Kind != RecoveryRecordKind.Reset)
            return new(RecoveryReadState.Corrupt, address, Error: new InvalidDataException("An intent does not contain a Reset decision."));
        return new(copy.State, address, copy.Encoded?.Record, copy.Encoded?.PayloadSha256, copy.Error);
    }

    /// <summary>Complete a verified Reset barrier using its original bytes, replacing only proven corrupt peers.</summary>
    public async Task<RecoveryPublicationResult> CompleteResetIntentAsync(RecoveryAddress address, CancellationToken cancellationToken = default)
    {
        RecoveryStoragePaths.ValidateAddress(address);
        if (address.Area.Kind != RecoveryAreaKind.Decisions) throw new ArgumentException("Reset intent is scope-local.", nameof(address));
        var intent = await ReadCopyAsync(address, "intent", cancellationToken).ConfigureAwait(false);
        if (intent.State != RecoveryReadState.Valid || intent.Encoded.Record.Kind != RecoveryRecordKind.Reset)
        {
            return new(intent.State == RecoveryReadState.Unreadable ? RecoveryPublicationState.Indeterminate : RecoveryPublicationState.NotCommitted,
                address, intent.Encoded?.PayloadSha256, Error: intent.Error ?? new InvalidDataException("The reset intent cannot be verified."));
        }

        var encoded = intent.Encoded;
        var a = await ReadCopyAsync(address, "a", cancellationToken).ConfigureAwait(false);
        var b = await ReadCopyAsync(address, "b", cancellationToken).ConfigureAwait(false);
        if (a.State == RecoveryReadState.Unreadable || b.State == RecoveryReadState.Unreadable ||
            a.State == RecoveryReadState.Valid && !SamePayload(a.Encoded, encoded) || b.State == RecoveryReadState.Valid && !SamePayload(b.Encoded, encoded))
        {
            return new(RecoveryPublicationState.Indeterminate, address, encoded.PayloadSha256,
                Error: a.Error ?? b.Error ?? new InvalidDataException("A reset intent conflicts with its final decision."));
        }

        try
        {
            foreach (var peer in new[] { a, b })
            {
                if (peer.State == RecoveryReadState.Valid) continue;
                cancellationToken.ThrowIfCancellationRequested();
                if (peer.State == RecoveryReadState.Corrupt) DeleteCopy(address, peer.Copy);
                await WriteCopyAsync(address, peer.Copy, encoded.EnvelopeBytes, cancellationToken).ConfigureAwait(false);
            }
            return new(RecoveryPublicationState.Committed, address, encoded.PayloadSha256, RecoveryRedundancy.Mirrored);
        }
        catch (Exception error) when (IsPublicationFailure(error))
        {
            var observed = await ReadAsync(address, cancellationToken: CancellationToken.None).ConfigureAwait(false);
            return Reconcile(address, encoded.PayloadSha256, observed.State, observed.PayloadSha256, observed.Redundancy, error);
        }
    }

    /// <summary>Delete metadata only after the catalog proves retirement under global admission.</summary>
    public Task<bool> DeleteAsync(RecoveryAddress address, CancellationToken cancellationToken = default)
    {
        RecoveryStoragePaths.ValidateAddress(address);
        cancellationToken.ThrowIfCancellationRequested();
        try
        {
            DeleteCopy(address, "a");
            DeleteCopy(address, "b");
            return Task.FromResult(true);
        }
        catch (Exception error) when (IsStorageFailure(error)) { return Task.FromResult(false); }
    }

    public async Task<IReadOnlyList<RecoveryAddress>> DeleteRetiredAsync(IEnumerable<RecoveryAddress> retired, CancellationToken cancellationToken = default)
    {
        var retained = new List<RecoveryAddress>();
        foreach (var address in retired)
            if (!await DeleteAsync(address, cancellationToken).ConfigureAwait(false)) retained.Add(address);
        return retained;
    }

    private RecoveryDirectoryScan<RecoveryArea> EnumerateChildAreas(RecoveryArea root, Func<Guid, RecoveryArea> createArea)
    {
        var areas = new List<RecoveryArea> { root };
        var unknown = new List<string>();
        try
        {
            var path = Paths.AreaPath(root);
            foreach (var entry in Directory.EnumerateDirectories(path))
            {
                Paths.EnsureSafePath(entry);
                if (TryIdentity(Path.GetFileName(entry), out var identity)) areas.Add(createArea(identity));
                else unknown.Add(entry);
            }
            return new(areas, unknown);
        }
        catch (DirectoryNotFoundException) when (areas.Count == 1 && unknown.Count == 0) { return new(areas, unknown); }
        catch (Exception error) when (IsStorageFailure(error)) { return new(areas, unknown, error); }
    }

    private async Task WriteCopyAsync(RecoveryAddress address, string copy, byte[] bytes, CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var path = Paths.CopyPath(address, copy);
        Directory.CreateDirectory(Paths.AreaPath(address.Area));
        var temporary = Paths.TemporaryPath(address, copy);
        try
        {
            faults?.OnStep(RecoveryStorageStep.BeforeTemporaryWrite, address, copy);
            using (var stream = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None, 16384, FileOptions.Asynchronous))
            {
                await stream.WriteAsync(bytes, cancellationToken).ConfigureAwait(false);
                stream.Flush(flushToDisk: true);
            }
            faults?.OnStep(RecoveryStorageStep.AfterTemporaryFlush, address, copy);
            OperationMetrics.MetadataCopyWritten(bytes.Length);
            cancellationToken.ThrowIfCancellationRequested();
            faults?.OnStep(RecoveryStorageStep.BeforeRename, address, copy);
            File.Move(Paths.EnsureSafePath(temporary), Paths.EnsureSafePath(path), overwrite: false);
            faults?.OnStep(RecoveryStorageStep.AfterRename, address, copy);
        }
        finally
        {
            // A rename lost acknowledgement may already have committed the final path. Never delete it here.
            try { File.Delete(Paths.EnsureSafePath(temporary)); }
            catch (Exception error) when (IsStorageFailure(error)) { }
        }
    }

    private async Task<CopyRead> ReadCopyAsync(RecoveryAddress address, string copy, CancellationToken cancellationToken,
        RecoveryEncodedRecord verifiedPeer = null)
    {
        OperationMetrics.MetadataReadAttempt();
        cancellationToken.ThrowIfCancellationRequested();
        try
        {
            var path = Paths.CopyPath(address, copy);
            faults?.OnStep(RecoveryStorageStep.BeforeRead, address, copy);
            using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read, 16384, FileOptions.Asynchronous | FileOptions.SequentialScan);
            if (stream.Length <= 0 || stream.Length > RecoveryRecordCodec.MaximumMetadataByteLength)
                throw new InvalidDataException("Invalid recovery metadata length.");
            var bytes = new byte[checked((int)stream.Length)];
            await stream.ReadExactlyAsync(bytes, cancellationToken).ConfigureAwait(false);
            OperationMetrics.MetadataBytesRead(bytes.Length);
            // Identical complete bytes share the already verified payload.
            // Different peers still undergo independent validation/conflict detection.
            var encoded = verifiedPeer != null && bytes.AsSpan().SequenceEqual(verifiedPeer.EnvelopeBytes) ?
                verifiedPeer : RecoveryRecordCodec.Decode(bytes, address);
            return new(copy, RecoveryReadState.Valid, encoded);
        }
        catch (FileNotFoundException) { return new(copy, RecoveryReadState.Absent); }
        catch (DirectoryNotFoundException) { return new(copy, RecoveryReadState.Absent); }
        catch (InvalidDataException error) { return new(copy, RecoveryReadState.Corrupt, Error: error); }
        catch (Exception error) when (IsStorageFailure(error)) { return new(copy, RecoveryReadState.Unreadable, Error: error); }
    }

    private void DeleteCopy(RecoveryAddress address, string copy)
    {
        faults?.OnStep(RecoveryStorageStep.BeforeDelete, address, copy);
        File.Delete(Paths.CopyPath(address, copy));
    }

    private static RecoveryReadResult Valid(RecoveryAddress address, RecoveryEncodedRecord encoded, RecoveryRedundancy redundancy, Exception error = null) =>
        new(RecoveryReadState.Valid, address, encoded.Record, encoded.PayloadSha256, encoded.EnvelopeBytes, redundancy, error);

    private static RecoveryPublicationResult Reconcile(RecoveryAddress address, string expectedHash, RecoveryReadState state,
        string observedHash, RecoveryRedundancy redundancy, Exception error)
    {
        var publication = state switch
        {
            RecoveryReadState.Valid when string.Equals(expectedHash, observedHash, StringComparison.Ordinal) => RecoveryPublicationState.Committed,
            RecoveryReadState.Unreadable or RecoveryReadState.Conflict => RecoveryPublicationState.Indeterminate,
            _ => RecoveryPublicationState.NotCommitted
        };
        return new(publication, address, expectedHash, publication == RecoveryPublicationState.Committed ? redundancy : RecoveryRedundancy.None, error);
    }

    private static bool SamePayload(RecoveryEncodedRecord a, RecoveryEncodedRecord b) =>
        string.Equals(a.PayloadSha256, b.PayloadSha256, StringComparison.Ordinal);

    private static bool IsStorageFailure(Exception error) => error is IOException or InvalidDataException or UnauthorizedAccessException;
    private static bool IsPublicationFailure(Exception error) => IsStorageFailure(error) || error is OperationCanceledException;
    private static bool TryIdentity(string value, out Guid identity) => Guid.TryParseExact(value, "N", out identity) && identity != Guid.Empty;
    private static bool IsDirectory(string path) => (File.GetAttributes(path) & FileAttributes.Directory) != 0;
    private sealed record CopyRead(string Copy, RecoveryReadState State, RecoveryEncodedRecord Encoded = null, Exception Error = null);
}
