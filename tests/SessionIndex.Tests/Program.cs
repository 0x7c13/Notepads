// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------


using System;
using System.Collections.Generic;
using System.Diagnostics.Metrics;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Legacy;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Validation;
using Notepads.Infrastructure.Diagnostics;

namespace NotepadsSessionIndexTests;

internal static class Program
{
    private static int assertions;

    private static async Task Main(string[] args)
    {
        if (args.Length == 1 && args[0].StartsWith("--metrics-", StringComparison.Ordinal))
        {
            await TestFailingObserverAsync(args[0]);
            Console.WriteLine($"PASS: {assertions} persistence assertions with {args[0]}.");
            return;
        }
        await TestMirrorsAsync();
        await TestPublicationFailuresAsync();
        await TestConflictAndOwnershipAsync();
        await TestResetIntentAsync();
        await TestDeletionAndOrderingAsync();
        TestCodecBounds();
        TestTrustedPathBoundary();
        await TestUnreadableAsync();
        await TestTransferShapeAsync();
        Console.WriteLine($"PASS: {assertions} session metadata assertions on real FileStream storage.");
    }

    private static async Task TestFailingObserverAsync(string mode)
    {
        if (mode is not ("--metrics-initialization-failure" or "--metrics-counter-failure" or "--metrics-duration-failure"))
            throw new ArgumentException("Unknown metrics failure mode.", nameof(mode));
        var callbacks = 0;
        using var listener = new MeterListener();
        listener.InstrumentPublished = (instrument, observer) =>
        {
            if (instrument.Meter.Name != OperationMetrics.MeterName) return;
            if (mode == "--metrics-initialization-failure")
            { callbacks++; throw new InvalidOperationException("Failing instrument observer."); }
            observer.EnableMeasurementEvents(instrument);
        };
        listener.SetMeasurementEventCallback<long>((instrument, value, tags, state) =>
        {
            if (mode == "--metrics-counter-failure")
            { callbacks++; throw new InvalidOperationException("Failing counter observer."); }
        });
        listener.SetMeasurementEventCallback<double>((instrument, value, tags, state) =>
        {
            if (mode == "--metrics-duration-failure")
            { callbacks++; throw new InvalidOperationException("Failing duration observer."); }
        });
        listener.Start();
        using var fixture = new StoreFixture();
        RecoveryPublicationResult publication;
        using (OperationMetrics.Measure("test.persistence"))
            publication = await fixture.Store.PublishAsync(fixture.Area, fixture.Checkpoint());
        Check(publication.IsCommitted, "an observer failure cannot undo publication");
        Check(callbacks > 0, "the requested observer failure actually ran");
        Check((await fixture.Store.ReadAsync(publication.Address)).State == RecoveryReadState.Valid,
            "both durable mirrors remain readable after a metrics failure");
        var retry = await fixture.Store.PublishAsync(fixture.Area, fixture.Checkpoint());
        Check(retry.IsCommitted, "future persistence remains available after disabling faulty observers");
    }

    private static async Task TestMirrorsAsync()
    {
        using var fixture = new StoreFixture();
        var record = fixture.Checkpoint();
        var result = await fixture.Store.PublishAsync(fixture.Area, record);
        Check(result.State == RecoveryPublicationState.Committed && result.Redundancy == RecoveryRedundancy.Mirrored, "both mirrors commit");
        var address = result.Address;
        var read = await fixture.Store.ReadAsync(address);
        Check(read.State == RecoveryReadState.Valid && read.Record.Session.TextEditors.Count == 1 && read.Redundancy == RecoveryRedundancy.Mirrored, "round trip retains workspace membership");
        var a = fixture.Store.Paths.CopyPath(address, "a");
        var b = fixture.Store.Paths.CopyPath(address, "b");
        Check(File.ReadAllBytes(a).SequenceEqual(File.ReadAllBytes(b)), "A/B bytes are identical");
        File.WriteAllText(a, "{truncated");
        read = await fixture.Store.ReadAsync(address, repair: true);
        Check(read.State == RecoveryReadState.Valid && read.Redundancy == RecoveryRedundancy.Repaired, "B repairs corrupt A");
        Check(File.ReadAllBytes(a).SequenceEqual(File.ReadAllBytes(b)), "repair preserves verified original envelope bytes");
        File.WriteAllText(a, "{\"\\uD800\":1}");
        read = await fixture.Store.ReadAsync(address, repair: true);
        Check(read.State == RecoveryReadState.Valid && read.Redundancy == RecoveryRedundancy.Repaired,
            "malformed escaped Unicode in a corrupt mirror is classified and repaired");
        File.Delete(b);
        read = await fixture.Store.ReadAsync(address, repair: true);
        Check(read.Redundancy == RecoveryRedundancy.Repaired && File.Exists(b), "missing B is repaired");
        File.Delete(b);
        read = await fixture.WithFaults(new BoundaryFault(RecoveryStorageStep.BeforeTemporaryWrite, "b")).ReadAsync(address, repair: true);
        Check(read.State == RecoveryReadState.Valid && read.Redundancy == RecoveryRedundancy.Single && read.Error != null,
            "failed repair is degradation of a verified commit rather than lost authority");
        await fixture.Store.ReadAsync(address, repair: true);
        File.WriteAllText(a, "bad A");
        File.WriteAllText(b, "bad B");
        Check((await fixture.Store.ReadAsync(address)).State == RecoveryReadState.Corrupt, "two invalid mirrors are corruption, not an empty session");
        File.Delete(a);
        File.Delete(b);
        Check((await fixture.Store.ReadAsync(address)).State == RecoveryReadState.Absent, "two genuinely absent mirrors are absent");
    }

    private static async Task TestPublicationFailuresAsync()
    {
        foreach (var point in new[] { RecoveryStorageStep.BeforeTemporaryWrite, RecoveryStorageStep.AfterTemporaryFlush, RecoveryStorageStep.BeforeRename, RecoveryStorageStep.AfterRename })
        {
            using var fixture = new StoreFixture();
            var failing = fixture.WithFaults(new BoundaryFault(point, "a"));
            var result = await failing.PublishAsync(fixture.Area, fixture.Checkpoint());
            var expected = point == RecoveryStorageStep.AfterRename ? RecoveryPublicationState.Committed : RecoveryPublicationState.NotCommitted;
            Check(result.State == expected, $"A failure at {point} reconciles exact publication");
            Check(!Directory.EnumerateFiles(fixture.Root, "*.tmp", SearchOption.AllDirectories).Any(), "ordinary failure removes only temporary paths");
        }
        foreach (var point in new[] { RecoveryStorageStep.BeforeTemporaryWrite, RecoveryStorageStep.AfterTemporaryFlush, RecoveryStorageStep.BeforeRename, RecoveryStorageStep.AfterRename })
        {
            using var fixture = new StoreFixture();
            var failing = fixture.WithFaults(new BoundaryFault(point, "b"));
            var result = await failing.PublishAsync(fixture.Area, fixture.Checkpoint());
            Check(result.State == RecoveryPublicationState.Committed, $"B failure at {point} does not invalidate A");
            Check(result.Redundancy == (point == RecoveryStorageStep.AfterRename ? RecoveryRedundancy.Mirrored : RecoveryRedundancy.Single), "redundancy follows reconciled final files");
        }
        using (var fixture = new StoreFixture())
        using (var canceled = new CancellationTokenSource())
        {
            var faults = new CallbackFault((point, address, copy) =>
            {
                if (point == RecoveryStorageStep.AfterRename && copy == "a") canceled.Cancel();
            });
            var result = await fixture.WithFaults(faults).PublishAsync(fixture.Area, fixture.Checkpoint(), canceled.Token);
            Check(result.State == RecoveryPublicationState.Committed && result.Redundancy == RecoveryRedundancy.Single, "cancellation after A drains reconciliation without deleting authority");
        }
        using (var fixture = new StoreFixture())
        {
            var faults = new CallbackFault((point, address, copy) =>
            {
                if (point == RecoveryStorageStep.AfterRename && copy == "a" || point == RecoveryStorageStep.BeforeRead)
                    throw new IOException("lost commit acknowledgement and unavailable reconciliation");
            });
            var result = await fixture.WithFaults(faults).PublishAsync(fixture.Area, fixture.Checkpoint());
            Check(result.State == RecoveryPublicationState.Indeterminate && File.Exists(fixture.Store.Paths.CopyPath(result.Address, "a")), "unreadable reconciliation preserves a potentially committed candidate");
        }
    }

    private static async Task TestConflictAndOwnershipAsync()
    {
        using var fixture = new StoreFixture();
        var record = fixture.Checkpoint();
        var result = await fixture.Store.PublishAsync(fixture.Area, record);
        var competing = fixture.Checkpoint();
        competing.Stamp.Ordinal = record.Stamp.Ordinal;
        competing.Stamp.OperationId = record.Stamp.OperationId;
        var bytes = RecoveryRecordCodec.Encode(competing, result.Address).EnvelopeBytes;
        File.WriteAllBytes(fixture.Store.Paths.CopyPath(result.Address, "b"), bytes);
        Check((await fixture.Store.ReadAsync(result.Address, repair: true)).State == RecoveryReadState.Conflict, "valid unequal peers block rather than choosing A");
        Check(File.ReadAllBytes(fixture.Store.Paths.CopyPath(result.Address, "b")).SequenceEqual(bytes), "repair never overwrites a valid conflict");
        var publication = await fixture.Store.PublishAsync(fixture.Area, record);
        Check(publication.State == RecoveryPublicationState.Indeterminate, "a conflicting operation cannot report successful publication");
        var foreign = fixture.Checkpoint();
        foreign.Session.TextEditors[0] = Editor(Guid.NewGuid());
        foreign.Session.SelectedTextEditor = foreign.Session.TextEditors[0].Id;
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Encode(foreign, Address(fixture.Area, foreign)), "scope-local foreign references are rejected");
        var archiveArea = fixture.Store.GetGlobalArea(RecoveryAreaKind.Archives, Guid.NewGuid());
        foreign.Kind = RecoveryRecordKind.Archive;
        Check((await fixture.Store.PublishAsync(archiveArea, foreign)).IsCommitted, "explicit multi-owner archive can protect foreign assets");
        var local = fixture.Checkpoint();
        local.Session.TextEditors[0].EditingFileFutureAccessToken = "Notepads:DocumentOwner:" + Guid.NewGuid().ToString("N") + ":grant";
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Encode(local, Address(fixture.Area, local)), "foreign grant does not establish scope-local ownership");
        var legacy = fixture.Checkpoint();
        legacy.Session.UnrecoveredLegacyEditors.Add(new TextEditorSessionDataV1
        {
            Id = Guid.NewGuid(),
            StateMetaData = new DocumentMetadata(),
            EditingFileFutureAccessToken = "unscoped V1 grant",
            PendingBackupFilePath = "foreign legacy resource"
        });
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Encode(legacy, Address(fixture.Area, legacy)),
            "scope-local roots cannot hide unscoped failed legacy assets or grants");
        legacy.Kind = RecoveryRecordKind.Archive;
        Check((await fixture.Store.PublishAsync(fixture.Store.GetGlobalArea(RecoveryAreaKind.Archives, Guid.NewGuid()), legacy)).IsCommitted,
            "manual multi-owner archives protect failed legacy rows without weakening local ownership");
        var duplicate = fixture.Checkpoint();
        duplicate.Session.TextEditors.Add(duplicate.Session.TextEditors[0]);
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Encode(duplicate, Address(fixture.Area, duplicate)), "duplicate editor IDs are rejected");
        var invalid = fixture.Checkpoint();
        invalid.Session.TextEditors[0].Journal.CommittedSequence = 1;
        invalid.Session.TextEditors[0].Journal.BaselineSequence = 2;
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Encode(invalid, Address(fixture.Area, invalid)), "invalid committed prefix ranges are rejected");
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Encode(record, new RecoveryAddress(fixture.Area, record.Stamp.Ordinal + 1, record.Stamp.OperationId)), "filename ordering must match checksummed identity");
        Throws<ArgumentException>(() => fixture.Store.Paths.UnderRoot("..", "elsewhere"), "path components cannot escape Recovery");
        Throws<ArgumentException>(() => fixture.Store.GetScopeArea(fixture.Owner, RecoveryAreaKind.Transfers), "global paths cannot masquerade as local roots");
    }

    private static async Task TestResetIntentAsync()
    {
        using var fixture = new StoreFixture();
        var area = fixture.Store.GetScopeArea(fixture.Owner, RecoveryAreaKind.Decisions);
        var reset = new RecoveryRecord
        {
            Kind = RecoveryRecordKind.Reset,
            Stamp = new RecoveryStamp { ScopeId = fixture.Owner, EpochId = Guid.NewGuid(), OperationId = Guid.NewGuid(), Ordinal = 1 },
            PreviousEpochId = fixture.Owner,
            ResetAcceptedLegacySources = [new string('a', 64)]
        };
        var intent = await fixture.Store.PublishResetIntentAsync(area, reset);
        Check(intent.IsCommitted, "reset barrier is durable before decision publication");
        var readIntent = await fixture.Store.ReadResetIntentAsync(intent.Address);
        Check(readIntent.State == RecoveryReadState.Valid && readIntent.Record.Stamp.EpochId == reset.Stamp.EpochId, "intent binds exact new epoch and operation");
        Check((await fixture.Store.ReadAsync(intent.Address)).State == RecoveryReadState.Absent, "an intent alone is not a complete decision");
        Check(fixture.Store.AllocateOrdinal(area) == 2, "intent occupies its ordinal");
        File.WriteAllText(fixture.Store.Paths.CopyPath(intent.Address, "a"), "damaged interrupted A");
        File.WriteAllText(fixture.Store.Paths.CopyPath(intent.Address, "b"), "damaged interrupted B");
        Check((await fixture.Store.CompleteResetIntentAsync(intent.Address)).IsCommitted,
            "verified intent can finish a reset whose interrupted final copies are both corrupt");
        var result = await fixture.Store.PublishAsync(area, reset);
        Check(result.IsCommitted && result.PayloadSha256 == intent.PayloadSha256, "matching decision commits the exact reset payload");
        var closed = new RecoveryRecord
        { Kind = RecoveryRecordKind.Closed, ClosedEditorId = Guid.NewGuid(), Stamp = fixture.Store.CreatePublicationStamp(area, reset.Stamp) };
        Check((await fixture.Store.PublishAsync(area, closed)).IsCommitted && fixture.Store.Enumerate(area).ResetIntents.SetEquals(new[] { intent.Address }),
            "enumeration reports a reset intent copy only for the operation that holds one");
        File.WriteAllText(fixture.Store.Paths.CopyPath(intent.Address, "intent"), "damaged intent");
        Check((await fixture.Store.ReadResetIntentAsync(intent.Address)).State == RecoveryReadState.Corrupt, "unreadable intent remains a distinct barrier condition");
        foreach (var point in new[] { RecoveryStorageStep.BeforeRename, RecoveryStorageStep.AfterRename })
        {
            reset.Stamp.Ordinal = fixture.Store.AllocateOrdinal(area);
            reset.Stamp.OperationId = Guid.NewGuid();
            var publication = await fixture.WithFaults(new BoundaryFault(point, "intent")).PublishResetIntentAsync(area, reset);
            Check(publication.State == (point == RecoveryStorageStep.AfterRename ? RecoveryPublicationState.Committed : RecoveryPublicationState.NotCommitted),
                $"intent at {point} reconciles durable barrier identity");
        }
        reset.Stamp.Ordinal = fixture.Store.AllocateOrdinal(area);
        reset.Stamp.OperationId = Guid.NewGuid();
        var rawAddress = Address(area, reset);
        var rawPayload = Encoding.UTF8.GetBytes(JsonSerializer.Serialize(reset, SessionJsonContext.Default.RecoveryRecord)
            .Replace("{\"Kind\"", "{\n \"Kind\"", StringComparison.Ordinal));
        var rawEnvelope = Envelope(rawPayload);
        File.WriteAllBytes(fixture.Store.Paths.CopyPath(rawAddress, "intent"), rawEnvelope);
        File.WriteAllText(fixture.Store.Paths.CopyPath(rawAddress, "a"), "bad final");
        var completed = await fixture.Store.CompleteResetIntentAsync(rawAddress);
        Check(completed.IsCommitted && completed.PayloadSha256 == Convert.ToHexStringLower(SHA256.HashData(rawPayload)) &&
            File.ReadAllBytes(fixture.Store.Paths.CopyPath(rawAddress, "a")).SequenceEqual(rawEnvelope),
            "intent completion preserves original payload whitespace and checksum without reserialization");
        reset.ResetAcceptedLegacySources = [new string('b', 64)];
        var conflicting = RecoveryRecordCodec.Encode(reset, rawAddress).EnvelopeBytes;
        File.WriteAllBytes(fixture.Store.Paths.CopyPath(rawAddress, "b"), conflicting);
        Check((await fixture.Store.CompleteResetIntentAsync(rawAddress)).State == RecoveryPublicationState.Indeterminate &&
            File.ReadAllBytes(fixture.Store.Paths.CopyPath(rawAddress, "b")).SequenceEqual(conflicting),
            "intent repair cannot overwrite a valid conflicting final decision");
    }

    private static async Task TestDeletionAndOrderingAsync()
    {
        using var fixture = new StoreFixture();
        var result = await fixture.Store.PublishAsync(fixture.Area, fixture.Checkpoint());
        var occupied = new RecoveryAddress(fixture.Area, 37, Guid.NewGuid());
        File.WriteAllText(fixture.Store.Paths.CopyPath(occupied, "a"), "damaged root occupies order");
        var temporary = new RecoveryAddress(fixture.Area, 45, Guid.NewGuid());
        File.WriteAllText(fixture.Store.Paths.TemporaryPath(temporary, "a"), "interrupted temporary root");
        Check(fixture.Store.AllocateOrdinal(fixture.Area) == 46, "ordinal exceeds corrupt finals and interrupted temporaries");
        Check(!fixture.Store.Enumerate(fixture.Area).Entries.Contains(temporary), "temporary paths reserve order without becoming restore candidates");
        var duplicateOrdinal = new RecoveryAddress(fixture.Area, occupied.Ordinal, Guid.NewGuid());
        File.WriteAllText(fixture.Store.Paths.CopyPath(duplicateOrdinal, "a"), "competing operation");
        Check(!fixture.Store.Enumerate(fixture.Area).IsComplete, "different operations sharing an ordinal expose an ordering conflict");
        File.Delete(fixture.Store.Paths.CopyPath(duplicateOrdinal, "a"));
        var failing = fixture.WithFaults(new BoundaryFault(RecoveryStorageStep.BeforeDelete, "b"));
        Check(!await failing.DeleteAsync(result.Address), "partial root deletion reports retained graph");
        Check(File.Exists(fixture.Store.Paths.CopyPath(result.Address, "b")), "failed B deletion leaves surviving protected root");
        Check(await fixture.Store.DeleteAsync(result.Address), "catalog-authorized retirement deletes remaining mirrors");
        var unknown = Path.Combine(fixture.Store.Paths.AreaPath(fixture.Area), "unclassifiable.json");
        File.WriteAllText(unknown, "opaque");
        Check(!fixture.Store.Enumerate(fixture.Area).IsComplete, "opaque metadata paths are reported rather than omitted");
        Throws<IOException>(() => fixture.Store.AllocateOrdinal(fixture.Area), "incomplete directory scan cannot allocate authority");
        var scopeUnknown = Path.Combine(fixture.Store.Paths.ScopesPath, "opaque-scope");
        Directory.CreateDirectory(scopeUnknown);
        Check(!fixture.Store.EnumerateScopeIds().IsComplete, "unknown scope directory blocks complete owner enumeration");
        File.WriteAllText(Path.Combine(fixture.Root, "opaque-root.json"), "opaque");
        Check(!fixture.Store.ScanLayout().IsComplete, "opaque global roots block complete recovery-layout interpretation");
        Directory.CreateDirectory(Path.Combine(fixture.Store.Paths.ScopePath(fixture.Owner), "UnexpectedArea"));
        Check(!fixture.Store.ScanLayout(fixture.Owner).IsComplete, "opaque scope-local roots retain their path-proven owner");
    }

    private static void TestCodecBounds()
    {
        using var fixture = new StoreFixture();
        var record = fixture.Checkpoint();
        var address = Address(fixture.Area, record);
        var payload = JsonSerializer.SerializeToUtf8Bytes(record, SessionJsonContext.Default.RecoveryRecord);
        // Leading/trailing payload whitespace is deliberately included in the hash.
        var original = Encoding.UTF8.GetBytes(" \n" + Encoding.UTF8.GetString(payload) + "\n ");
        var envelope = Envelope(original);
        // Only bytes that form the payload value belong to its hash, excluding envelope spacing.
        var spacedPayload = Encoding.UTF8.GetBytes(Encoding.UTF8.GetString(payload).Replace("{\"Kind\"", "{ \n\"Kind\"", StringComparison.Ordinal));
        Check(RecoveryRecordCodec.Decode(Envelope(spacedPayload), address).PayloadSha256 == Convert.ToHexStringLower(SHA256.HashData(spacedPayload)), "checksum uses original payload bytes, including internal whitespace");
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Decode(envelope, address), "envelope whitespace is not ambiguously treated as part of its payload hash");
        var changed = Encoding.UTF8.GetString(Envelope(payload)).Replace("\"FormatVersion\":1", "\"FormatVersion\":2", StringComparison.Ordinal);
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Decode(Encoding.UTF8.GetBytes(changed), address), "unsupported versions are rejected");
        changed = Encoding.UTF8.GetString(Envelope(payload)).Replace("\"CaptureRevision\":5", "\"CaptureRevision\":6", StringComparison.Ordinal);
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Decode(Encoding.UTF8.GetBytes(changed), address), "payload mutation without matching checksum is rejected");
        var duplicate = Encoding.UTF8.GetString(payload).Replace("\"Kind\":0", "\"Kind\":0,\"Kind\":0", StringComparison.Ordinal);
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Decode(Envelope(Encoding.UTF8.GetBytes(duplicate)), address), "duplicate payload properties are rejected even with a valid checksum");
        changed = Encoding.UTF8.GetString(Envelope(payload)).Replace("\"FormatVersion\":1", "\"FormatVersion\":1,\"FormatVersion\":1", StringComparison.Ordinal);
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Decode(Encoding.UTF8.GetBytes(changed), address), "duplicate envelope properties are rejected");
        var nestedDuplicate = "{\"nested\":{\"x\":1,\"\\u0078\":2}}";
        Throws<InvalidDataException>(() => RecoveryRecordCodec.ValidateJson(Encoding.UTF8.GetBytes(nestedDuplicate)), "escaped duplicate property aliases are rejected at every depth");
        Throws<InvalidDataException>(() => RecoveryRecordCodec.ValidateJson(Encoding.UTF8.GetBytes("{\"\\uD800\":1}")),
            "escaped unpaired surrogate property names are normalized as corruption");
        Throws<InvalidDataException>(() => RecoveryRecordCodec.ValidateJson(Encoding.UTF8.GetBytes("{\"value\":\"\\uD800\"}")),
            "escaped unpaired surrogate values are rejected before deserialization");
        Throws<InvalidDataException>(() => RecoveryRecordCodec.ValidateJson([(byte)'{', (byte)'"', 0xff, (byte)'"', (byte)':', (byte)'1', (byte)'}']), "invalid UTF-8 is rejected before deserialization");
        Throws<InvalidDataException>(() => RecoveryRecordCodec.ValidateJson(new byte[RecoveryRecordCodec.MaximumMetadataByteLength + 1]), "read limit is enforced before deserialization");
        Throws<InvalidDataException>(() => RecoveryRecordCodec.ValidateJson(Encoding.UTF8.GetBytes(new string('[', 33) + "0" + new string(']', 33))), "depth limit is enforced before deserialization");
        var huge = fixture.Checkpoint();
        huge.Session.TextEditors[0].EditingFileName = new string('x', RecoveryRecordCodec.MaximumMetadataByteLength);
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Encode(huge, Address(fixture.Area, huge)), "oversized captures fail instead of dropping tabs");
        var exhausted = fixture.Checkpoint();
        exhausted.Session.TextEditors[0].CaptureRevision = long.MaxValue;
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Encode(exhausted, Address(fixture.Area, exhausted)),
            "persisted capture revisions must leave room for a future recovery capture");
        exhausted = fixture.Checkpoint();
        exhausted.Session.SourceRemovalRevision = long.MaxValue;
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Encode(exhausted, Address(fixture.Area, exhausted)),
            "source removal cutoffs cannot exhaust the durable revision floor");
        exhausted.Session.SourceRemovalRevision = 0;
        exhausted.Consumptions.Add(new RecoveryConsumption
        {
            Kind = RecoveryConsumptionKind.Inactive,
            SourceScopeId = fixture.Owner,
            SourceEpochId = fixture.Owner,
            SourceEditorId = Guid.NewGuid(),
            SourceCaptureRevision = long.MaxValue
        });
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Encode(exhausted, Address(fixture.Area, exhausted)),
            "source consumption cutoffs cannot exhaust the durable revision floor");
    }

    private static async Task TestUnreadableAsync()
    {
        using var fixture = new StoreFixture();
        var result = await fixture.Store.PublishAsync(fixture.Area, fixture.Checkpoint());
        using var unavailable = new FileStream(fixture.Store.Paths.CopyPath(result.Address, "b"), FileMode.Open, FileAccess.ReadWrite, FileShare.None);
        var read = await fixture.Store.ReadAsync(result.Address, repair: true);
        Check(read.State == RecoveryReadState.Unreadable && read.Record == null, "inaccessible peer blocks authority even when A validates");
        using var badFixture = new StoreFixture();
        var failed = badFixture.WithFaults(new CallbackFault((point, address, copy) =>
        {
            if (point == RecoveryStorageStep.BeforeRead) throw new UnauthorizedAccessException("injected permissions failure");
        }));
        read = await failed.ReadAsync(new RecoveryAddress(badFixture.Area, 1, Guid.NewGuid()));
        Check(read.State == RecoveryReadState.Unreadable, "permission failure is not reported as a missing root");
    }

    private static void TestTrustedPathBoundary()
    {
        using var fixture = new StoreFixture();
        var probes = new List<string>();
        var paths = new RecoveryStoragePaths(fixture.Root, path =>
        {
            probes.Add(path);
            return File.GetAttributes(path);
        });
        paths.TemporaryPath(new RecoveryAddress(fixture.Area, 1, Guid.NewGuid()), "a");
        Check(probes.SequenceEqual([paths.RootPath]),
            "path safety probes only the trusted root, never its inaccessible ancestors or the descendants it builds");
        Throws<InvalidDataException>(() => new RecoveryStoragePaths(fixture.Root, path => FileAttributes.Directory | FileAttributes.ReparsePoint),
            "the configured recovery root itself cannot be a reparse point");
        var entry = fixture.Store.Paths.ScopePath(Guid.NewGuid());
        Throws<InvalidDataException>(() => fixture.Store.Paths.EnsureSafeEntry(entry, FileAttributes.Directory | FileAttributes.ReparsePoint),
            "an enumerated entry below the trusted root cannot be a reparse point");
        fixture.Store.Paths.EnsureSafeEntry(entry, FileAttributes.Directory);

        using var outside = new StoreFixture();
        var link = Path.Combine(fixture.Store.Paths.ScopesPath, Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(fixture.Store.Paths.ScopesPath);
        try { Directory.CreateSymbolicLink(link, outside.Root); }
        catch (Exception error) when (error is UnauthorizedAccessException or IOException)
        {
            Console.WriteLine($"SKIP: a reparse point below the trusted root (cannot create a directory link: {error.Message})");
            return;
        }
        try
        {
            var scan = fixture.Store.EnumerateScopeIds();
            Check(scan.Error is InvalidDataException && scan.Entries.Count == 0, "a scan refuses a reparse point below the trusted root");
        }
        finally { Directory.Delete(link); }
    }

    private static async Task TestTransferShapeAsync()
    {
        using var fixture = new StoreFixture();
        var source = fixture.Checkpoint().Session.TextEditors[0];
        var sourceStamp = new RecoveryStamp { ScopeId = fixture.Owner, EpochId = fixture.Owner };
        var token = new TransferToken
        {
            TransferId = Guid.NewGuid(),
            SourceInstanceId = fixture.Owner,
            SourceOwnerId = fixture.Owner,
            SourceEpochId = fixture.Owner,
            SourceEditorId = source.Id,
            DescriptorSha256 = source.ComputeSha256(),
            Nonce = Guid.NewGuid()
        };
        var session = new NotepadsSessionDataV2
        {
            Scope = new SessionScopeData { OwnerId = fixture.Owner, InstanceId = fixture.Owner, Kind = SessionScopeData.Secondary },
            SelectedTextEditor = source.Id,
            TextEditors = [source],
            TransferSource = token
        };
        var record = new RecoveryRecord
        {
            Kind = RecoveryRecordKind.TransferOffer,
            Session = session,
            Editor = source,
            SourceStamp = sourceStamp,
            Stamp = new RecoveryStamp { ScopeId = fixture.Owner, EpochId = fixture.Owner, Ordinal = 1, OperationId = Guid.NewGuid() }
        };
        var area = fixture.Store.GetGlobalArea(RecoveryAreaKind.Transfers, token.TransferId);
        Check((await fixture.Store.PublishAsync(area, record)).IsCommitted, "a frozen source transfer offer has a complete protected graph");
        record.SourceStamp = null;
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Encode(record, Address(area, record)), "transfer cannot deserialize as valid without its expected source stamp");
        record.SourceStamp = sourceStamp;
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Encode(record, Address(fixture.Store.GetGlobalArea(RecoveryAreaKind.Transfers, Guid.NewGuid()), record)),
            "logical transfer root must match its transfer identity");
        record.Kind = RecoveryRecordKind.SourceMoved;
        record.TargetStamp = new RecoveryStamp { ScopeId = Guid.NewGuid(), EpochId = Guid.NewGuid() };
        Throws<InvalidDataException>(() => RecoveryRecordCodec.Encode(record, Address(area, record)), "source move requires a complete acknowledgement and durable receipt binding");
    }

    private static byte[] Envelope(byte[] payload)
    {
        var hash = Convert.ToHexStringLower(SHA256.HashData(payload));
        using var stream = new MemoryStream();
        using (var writer = new Utf8JsonWriter(stream))
        {
            writer.WriteStartObject();
            writer.WriteNumber("FormatVersion", 1);
            writer.WriteString("Sha256", hash);
            writer.WritePropertyName("Payload");
            writer.WriteRawValue(payload, skipInputValidation: true);
            writer.WriteEndObject();
        }
        return stream.ToArray();
    }

    private static RecoveryAddress Address(RecoveryArea area, RecoveryRecord record) => new(area, record.Stamp.Ordinal, record.Stamp.OperationId);

    private static TextEditorSessionDataV2 Editor(Guid owner)
    {
        var generation = Guid.NewGuid();
        return new TextEditorSessionDataV2
        {
            Id = Guid.NewGuid(),
            CaptureRevision = 5,
            SavedBaseline = EmptyBaseline(owner),
            RecoveryBaseline = EmptyBaseline(owner),
            Journal = new DocumentJournalData
            {
                OwnerId = owner,
                GenerationId = generation,
                FileName = $"{owner:N}-{generation:N}.npj",
                CommittedByteLength = 48,
                PrefixSha256 = new string('b', 64)
            },
            StateMetaData = new DocumentMetadata { FontZoomFactor = 1 }
        };
    }

    private static DocumentBaselineData EmptyBaseline(Guid owner) => new()
    {
        OwnerId = owner,
        GenerationId = Guid.NewGuid(),
        ByteLength = 0,
        Sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    };

    private static void Check(bool condition, string message)
    {
        assertions++;
        if (!condition) throw new InvalidOperationException(message);
    }

    private static void Throws<T>(Action action, string message) where T : Exception
    {
        try { action(); }
        catch (T) { assertions++; return; }
        throw new InvalidOperationException(message);
    }

    private sealed class StoreFixture : IDisposable
    {
        public readonly string Root = Path.Combine(Path.GetTempPath(), "NotepadsSessionIndexTests", Guid.NewGuid().ToString("N"));
        public readonly Guid Owner = Guid.NewGuid();
        public readonly RecoveryRecordStore Store;
        public readonly RecoveryArea Area;

        public StoreFixture()
        {
            Directory.CreateDirectory(Root);
            Store = new RecoveryRecordStore(Root);
            Area = Store.GetScopeArea(Owner, RecoveryAreaKind.Checkpoints);
        }

        public RecoveryRecordStore WithFaults(IRecoveryStorageFaults faults) => new(Root, faults);

        public RecoveryRecord Checkpoint()
        {
            var editor = Editor(Owner);
            return new RecoveryRecord
            {
                Kind = RecoveryRecordKind.Checkpoint,
                Stamp = new RecoveryStamp { ScopeId = Owner, EpochId = Owner, OperationId = Guid.NewGuid(), Ordinal = Store.AllocateOrdinal(Area) },
                Session = new NotepadsSessionDataV2
                {
                    Scope = new SessionScopeData { OwnerId = Owner, InstanceId = Owner, Kind = SessionScopeData.Secondary },
                    SelectedTextEditor = editor.Id,
                    TextEditors = [editor]
                }
            };
        }

        public void Dispose()
        {
            var parent = Path.GetFullPath(Path.Combine(Path.GetTempPath(), "NotepadsSessionIndexTests")) + Path.DirectorySeparatorChar;
            var target = Path.GetFullPath(Root);
            if (!target.StartsWith(parent, StringComparison.OrdinalIgnoreCase)) throw new InvalidOperationException("Unsafe fixture cleanup.");
            if (Directory.Exists(target)) Directory.Delete(target, recursive: true);
        }
    }

    private sealed class BoundaryFault(RecoveryStorageStep step, string copy) : IRecoveryStorageFaults
    {
        public void OnStep(RecoveryStorageStep current, RecoveryAddress address, string currentCopy)
        {
            if (current == step && currentCopy == copy) throw new IOException($"Injected {step} failure for {copy}");
        }
    }

    private sealed class CallbackFault(Action<RecoveryStorageStep, RecoveryAddress, string> callback) : IRecoveryStorageFaults
    {
        public void OnStep(RecoveryStorageStep step, RecoveryAddress address, string copy) => callback(step, address, copy);
    }
}
