// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Diagnostics.Metrics;
using System.Linq;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.IO;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Documents.Text;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Infrastructure.Diagnostics;
using Windows.Storage;
using Windows.System;

namespace NotepadsEditorTests;

internal static class SessionPerformanceTests
{
    public static async Task RunAsync(StringBuilder log)
    {
        var results = new List<PerformanceRow>();
        using var observer = new PerformanceObserver();
        foreach (var scenario in new[] { (10, 65536, 0), (10, 4 * 1024 * 1024, 0), (50, 65536, 0), (10, 65536, 20) })
            await RunScenarioAsync(scenario.Item1, scenario.Item2, scenario.Item3, observer, results);
        var file = await ApplicationData.Current.LocalFolder.CreateFileAsync("performance.json", CreationCollisionOption.ReplaceExisting);
        await FileIO.WriteTextAsync(file, JsonSerializer.Serialize(results, PerformanceJsonContext.Default.ListPerformanceRow));
        foreach (var result in results)
            log.AppendLine($"PERF tabs={result.Tabs} bytes={result.DocumentBytes} foreign={result.ForeignScopes} stage={result.Stage} ms={result.Milliseconds:F2} reads={result.MetadataReads} writes={result.MetadataWrites} readBytes={result.MetadataReadBytes} writeBytes={result.MetadataWriteBytes}");
        log.AppendLine("PASS: isolated session/recovery/atomic UTF-8 save workloads completed; timings are observations, not speed or maximum-size guarantees.");
    }

    private static async Task RunScenarioAsync(int tabs, int requestedBytes, int foreignScopes,
        PerformanceObserver observer, List<PerformanceRow> results)
    {
        var owner = Guid.NewGuid();
        var identities = Enumerable.Range(0, tabs).Select(_ => Guid.NewGuid()).ToArray();
        var foreignOwners = new List<Guid>();
        var originals = new List<SessionTestDocument>();
        var restored = new List<SessionTestDocument>();
        var service = SessionTestProtocol.CreateService(owner);
        SessionService reopened = null;
        StorageFile output = null;
        var bytesPerDocument = 0L;
        try
        {
            var line = "performance العربية😀 " + new string('x', 160) + "\r";
            var page = Encoding.UTF8.GetBytes(string.Concat(Enumerable.Repeat(line, 300)));
            var pages = Math.Max(1, requestedBytes / page.Length);
            for (var index = 0; index < tabs; index++)
            {
                using var baseline = await DocumentBaseline.CreateAsync(async stream =>
                {
                    for (var block = 0; block < pages; block++) await stream.WriteAsync(page);
                }, ownerId: owner);
                bytesPerDocument = baseline.ByteLength;
                var document = await SessionTestDocument.CreateAsync(owner, baseline);
                document.Metadata.FileNamePlaceholder = $"Performance {index + 1}.txt";
                originals.Add(document);
            }
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var store = RecoveryRootStore.CreateStore();
                for (var index = 0; index < foreignScopes; index++)
                {
                    var foreign = Guid.NewGuid(); foreignOwners.Add(foreign);
                    var area = store.GetScopeArea(foreign, RecoveryAreaKind.Decisions);
                    for (var decision = 0; decision < 5; decision++)
                    {
                        RecoveryRootStore.RequireCommitted(await store.PublishAsync(area, new RecoveryRecord
                        {
                            Kind = RecoveryRecordKind.Closed,
                            ClosedEditorId = Guid.NewGuid(),
                            Stamp = store.CreatePublicationStamp(area, new RecoveryStamp { ScopeId = foreign, EpochId = foreign })
                        }));
                    }
                }
            }

            Task Measure(string stage, Func<Task> work) => MeasureAsync(stage, tabs, bytesPerDocument, foreignScopes, work, observer, results);
            await Measure("authority", () => service.InitializeAuthorityAsync());
            await Measure("first-publication", async () =>
            {
                using var capture = SessionTestProtocol.CaptureDocuments(originals, identities, service.CurrentStamp);
                var result = await service.SaveAsync(capture, CancellationToken.None);
                SessionTestProtocol.Check(result.Succeeded && result.Changed, "Initial performance fixture did not commit.");
            });
            await Measure("unchanged-publication", async () =>
            {
                using var capture = SessionTestProtocol.CaptureDocuments(originals, identities, service.CurrentStamp);
                var result = await service.SaveAsync(capture, CancellationToken.None);
                SessionTestProtocol.Check(result.Succeeded && !result.Changed, "An unchanged snapshot unnecessarily committed.");
            });
            originals[0].Editor.InsertText(0, "edited\r");
            await Measure("one-document-publication", async () =>
            {
                using var capture = SessionTestProtocol.CaptureDocuments(originals, identities, service.CurrentStamp);
                var result = await service.SaveAsync(capture, CancellationToken.None);
                SessionTestProtocol.Check(result.Succeeded && result.Changed, "The changed snapshot did not commit.");
            });
            foreach (var document in originals) await document.DisposeAsync();
            originals.Clear();
            await service.DisposeAsync();
            reopened = SessionTestProtocol.CreateService(owner);
            await Measure("metadata", () => reopened.EnsureMetadataRetainedAsync(CancellationToken.None));
            PreparedRecoveryBatch batch = null;
            try
            {
                await Measure("prepare", async () => batch = await reopened.PrepareRecoveryAsync(new DocumentLoadOptions(), CancellationToken.None));
                SessionTestProtocol.Check(batch.Documents.Count == tabs, "The workload lost a recovery row.");
                await Measure("native-restore", async () =>
                {
                    var completed = new SessionTestDocument[batch.Documents.Count];
                    try
                    {
                        await RecoveryRestorePipeline.RunAsync(batch.Documents,
                            async (index, prepared) => completed[index] = await SessionTestDocument.FromPreparedAsync(owner, prepared), CancellationToken.None);
                    }
                    finally { restored.AddRange(completed.Where(document => document != null)); }
                });
                var restoredIds = batch.Documents.Select(document => document.Id).ToArray();
                await Measure("restore-publication", async () =>
                {
                    using var capture = SessionTestProtocol.CaptureDocuments(restored, restoredIds, batch.ExpectedStamp);
                    await reopened.CommitAttachedAsync(capture, restoredIds, CancellationToken.None);
                });
            }
            finally { batch?.Dispose(); }

            output = await ApplicationData.Current.LocalFolder.CreateFileAsync($"performance-{owner:N}.txt", CreationCollisionOption.FailIfExists);
            DocumentBaseline saved = null;
            try
            {
                await Measure("atomic-utf8-save", async () =>
                {
                    using var reader = restored[0].Editor.AcquireUtf8Reader();
                    await DocumentFileWriter.WriteAsync(output, async destination =>
                    {
                        saved?.Dispose(); saved = null;
                        using var source = new EditorDocumentStream(reader, ownsReader: false);
                        saved = await DocumentBaseline.CreateAsync(canonical => DocumentTextCodec.CopyFromNativeAsync(
                            source, canonical, destination, new UTF8Encoding(false, true), LineEnding.Crlf), ownerId: owner);
                    });
                });
                using var decoded = await DocumentTextPipeline.DecodeFileAsync(output, new DocumentLoadOptions(new UTF8Encoding(false, true)), owner);
                SessionTestProtocol.Check(decoded.Baseline.Sha256 == saved.Sha256, "Atomic-save bytes differ from the canonical candidate.");
            }
            finally { saved?.Dispose(); }
            await Measure("maintenance", () => reopened.FinishPublicationAsync(new SessionSaveResult(true, true), CancellationToken.None));
        }
        finally
        {
            foreach (var document in originals.Concat(restored)) await document.DisposeAsync();
            await service.DisposeAsync();
            if (reopened != null) await reopened.DisposeAsync();
            if (output != null) await output.DeleteAsync();
            await SessionTestProtocol.CleanupAsync(owner);
            foreach (var foreign in foreignOwners) await SessionTestProtocol.CleanupAsync(foreign);
        }
    }

    private static async Task MeasureAsync(string stage, int tabs, long bytes, int foreignScopes, Func<Task> work,
        PerformanceObserver observer, List<PerformanceRow> results)
    {
        var before = observer.Snapshot();
        var started = Stopwatch.GetTimestamp();
        await work();
        var elapsed = Stopwatch.GetElapsedTime(started).TotalMilliseconds;
        var after = observer.Snapshot();
        results.Add(new PerformanceRow
        {
            Stage = stage,
            Tabs = tabs,
            DocumentBytes = bytes,
            ForeignScopes = foreignScopes,
            Milliseconds = elapsed,
            MetadataReads = after.Reads - before.Reads,
            MetadataReadBytes = after.ReadBytes - before.ReadBytes,
            MetadataWrites = after.Writes - before.Writes,
            MetadataWriteBytes = after.WriteBytes - before.WriteBytes,
            ManagedBytes = GC.GetTotalMemory(false),
            AppMemoryBytes = MemoryManager.AppMemoryUsage
        });
    }

    private sealed class PerformanceObserver : IDisposable
    {
        private readonly MeterListener _listener = new();
        private long _reads, _readBytes, _writes, _writeBytes;
        public PerformanceObserver()
        {
            _listener.InstrumentPublished = (instrument, listener) =>
            {
                if (instrument.Meter.Name == OperationMetrics.MeterName) listener.EnableMeasurementEvents(instrument);
            };
            _listener.SetMeasurementEventCallback<long>((instrument, value, tags, state) =>
            {
                switch (instrument.Name)
                {
                    case "notepads.session.metadata.reads": Interlocked.Add(ref _reads, value); break;
                    case "notepads.session.metadata.read.bytes": Interlocked.Add(ref _readBytes, value); break;
                    case "notepads.session.metadata.writes": Interlocked.Add(ref _writes, value); break;
                    case "notepads.session.metadata.write.bytes": Interlocked.Add(ref _writeBytes, value); break;
                }
            });
            _listener.SetMeasurementEventCallback<double>((instrument, value, tags, state) => { });
            _listener.Start();
        }
        public (long Reads, long ReadBytes, long Writes, long WriteBytes) Snapshot() =>
            (Interlocked.Read(ref _reads), Interlocked.Read(ref _readBytes), Interlocked.Read(ref _writes), Interlocked.Read(ref _writeBytes));
        public void Dispose() => _listener.Dispose();
    }
}

internal sealed class PerformanceRow
{
    public string Stage { get; set; }
    public int Tabs { get; set; }
    public long DocumentBytes { get; set; }
    public int ForeignScopes { get; set; }
    public double Milliseconds { get; set; }
    public long MetadataReads { get; set; }
    public long MetadataReadBytes { get; set; }
    public long MetadataWrites { get; set; }
    public long MetadataWriteBytes { get; set; }
    public long ManagedBytes { get; set; }
    public ulong AppMemoryBytes { get; set; }
}

[JsonSerializable(typeof(List<PerformanceRow>))]
internal sealed partial class PerformanceJsonContext : JsonSerializerContext { }
