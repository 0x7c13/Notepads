// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.Diagnostics.Metrics;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsIntegrationTests;
using Windows.Storage;
using Windows.System;

namespace NotepadsEditorTests;

internal static partial class SessionPerformanceTests
{
    public static async Task RunAsync(StringBuilder log)
    {
        var results = new List<PerformanceRow>();
        using var observer = new PerformanceObserver();
        foreach (var scenario in new[] { (10, 65536, 0), (10, 4 * 1024 * 1024, 0), (50, 65536, 0), (10, 65536, 20) })
            await RunScenarioAsync(scenario.Item1, scenario.Item2, scenario.Item3, observer, results);
        await using (var host = await FeatureTestHost.CreateAsync())
        {
            host.Page.TestController.StopSessionBackup();
            await host.Page.TestController.WaitForRestorationCheckpointAsync();
            host.Page.TestController.IsBackupEnabled = false;
            foreach (var mebibytes in new[] { 1, 8, 64 })
                await MeasureEditorSaveAsync(host, mebibytes, editDuringCapture: false, observer, results);
            await MeasureEditorSaveAsync(host, 8, editDuringCapture: true, observer, results);
        }
        var file = await ApplicationData.Current.LocalFolder.CreateFileAsync("performance.json", CreationCollisionOption.ReplaceExisting);
        await FileIO.WriteTextAsync(file, JsonSerializer.Serialize(results, PerformanceJsonContext.Default.ListPerformanceRow));
        foreach (var result in results)
        {
            var phases = result.SaveMillisecondsByPhase == null ? string.Empty :
                " " + string.Join(" ", result.SaveMillisecondsByPhase.Select(item => $"{item.Key}Ms={item.Value:F2}"));
            log.AppendLine($"PERF tabs={result.Tabs} bytes={result.DocumentBytes} foreign={result.ForeignScopes} stage={result.Stage} ms={result.Milliseconds:F2} reads={result.MetadataReads} writes={result.MetadataWrites} readBytes={result.MetadataReadBytes} writeBytes={result.MetadataWriteBytes}{phases}");
        }
        log.AppendLine("PASS: isolated session/recovery/editor save workloads completed; timings are observations, not speed or maximum-size guarantees.");
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
            SessionRecoveryPlan plan = null;
            await Measure("metadata", async () => plan = await reopened.DiscoverRecoveryAsync(CancellationToken.None));
            var restoredIds = Array.Empty<Guid>();
            SessionTestRecoveryBatch batch = null;
            try
            {
                await Measure("prepare", async () => batch = await SessionTestProtocol.MaterializeAsync(reopened, new DocumentLoadOptions(), CancellationToken.None, plan));
                SessionTestProtocol.Check(batch.Documents.Count == tabs, "The workload lost a recovery row.");
                await Measure("native-restore", async () =>
                {
                    foreach (var prepared in batch.Documents)
                        restored.Add(await SessionTestDocument.FromPreparedAsync(owner, prepared));
                });
                restoredIds = batch.Documents.Select(document => document.Id).ToArray();
                await Measure("restore-publication", async () =>
                {
                    using var capture = SessionTestProtocol.CaptureDocuments(restored, restoredIds, batch.ExpectedStamp);
                    await SessionTestProtocol.PublishRecoveredAsync(reopened, batch, capture);
                });
            }
            finally { batch?.Dispose(); plan?.Dispose(); }

            await Measure("maintenance", () => reopened.RunMaintenanceAsync(CancellationToken.None));
            await Measure("tab-close", async () => SessionTestProtocol.Check(
                await reopened.PrepareExplicitCloseAsync(new[] { restoredIds[0] }), "The measured tab close did not commit."));
            if (foreignScopes != 0)
            {
                // A primary window reads every inactive scope at startup, all within one admitted catalog pass.
                var primaryOwner = Guid.NewGuid();
                foreignOwners.Add(primaryOwner);
                var primary = SessionTestProtocol.CreateService(primaryOwner, primary: true);
                try { await Measure("inactive-startup", async () => { using var inactive = await primary.DiscoverRecoveryAsync(); }); }
                finally { await primary.DisposeAsync(); }
            }
        }
        finally
        {
            foreach (var document in originals.Concat(restored)) await document.DisposeAsync();
            await service.DisposeAsync();
            if (reopened != null) await reopened.DisposeAsync();
            await SessionTestProtocol.CleanupAsync(owner);
            foreach (var foreign in foreignOwners) await SessionTestProtocol.CleanupAsync(foreign);
        }
    }

    private static async Task MeasureEditorSaveAsync(FeatureTestHost host, int mebibytes, bool editDuringCapture,
        PerformanceObserver observer, List<PerformanceRow> results)
    {
        var file = await host.Folder.CreateFileAsync($"editor-{mebibytes}-{editDuringCapture}.txt", CreationCollisionOption.FailIfExists);
        const int blockSize = 65536;
        var block = new byte[blockSize];
        Array.Fill(block, (byte)'a');
        for (var index = 255; index < block.Length; index += 256) block[index] = (byte)'\n';
        using var expected = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        expected.AppendData("!"u8);
        using (var output = await file.OpenStreamForWriteAsync())
        {
            for (var remaining = mebibytes * 1024 * 1024; remaining > 0; remaining -= blockSize)
            {
                expected.AppendData(block);
                await output.WriteAsync(block);
            }
        }
        var expectedHash = Convert.ToHexString(expected.GetHashAndReset());
        SessionTestProtocol.Check(await host.Page.OpenFileAsync(file), "The performance document must open through MainPage.");
        var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
        await NotepadsWorkspaceTests.WorkspaceTestUI.UntilAsync(() => editor.TestNativeCore.IsLoaded && host.Page.IsEnabled,
            $"The save benchmark must start from an interactive page (initially enabled={host.Page.IsEnabled}).");
        await editor.TestStopFileChecksAsync();
        editor.TestNativeCore.SetTextSelectionPosition(0, 0);
        editor.TypeText("!");
        if (editDuringCapture)
        {
            editor.TestSaveReaderCaptured = () =>
            {
                editor.TestNativeCore.SetTextSelectionPosition(1, 4 * 1024 * 1024 + 1);
                using (OperationMetrics.Measure("document.save.edit-during-capture")) editor.TypeText("replacement");
                using (OperationMetrics.Measure("document.save.undo-during-capture")) editor.TestNativeCore.Undo();
                return Task.CompletedTask;
            };
        }
        try
        {
            await MeasureAsync(editDuringCapture ? "editor-save-with-edit" : "editor-save", 1, mebibytes * 1024L * 1024L + 1, 0, async () =>
                SessionTestProtocol.Check(await host.Page.TestSaveDocumentAsync(editor), "The real dirty editor save must verify."),
                observer, results, includeSavePhases: true);
        }
        finally { editor.TestSaveReaderCaptured = null; }

        using (var input = await file.OpenStreamForReadAsync())
            SessionTestProtocol.Check(Convert.ToHexString(await SHA256.HashDataAsync(input)) == expectedHash, "The editor save must persist the admitted bytes.");
        await editor.WaitForDocumentOperationsAsync();
        SessionTestProtocol.Check(!editor.IsModified, "Undo must return the live editor to the saved bytes.");
        await host.Page.TestCloseTabAsync(host.Core.GetTab(editor));
        await editor.DisposalCompletion;
        await file.DeleteAsync();
    }

    private static async Task MeasureAsync(string stage, int tabs, long bytes, int foreignScopes, Func<Task> work,
        PerformanceObserver observer, List<PerformanceRow> results, bool includeSavePhases = false)
    {
        if (includeSavePhases) observer.ClearSavePhases();
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
            AppMemoryBytes = MemoryManager.AppMemoryUsage,
            SaveMillisecondsByPhase = includeSavePhases ? observer.CopySavePhases() : null
        });
    }

    private sealed partial class PerformanceObserver : IDisposable
    {
        private readonly MeterListener _listener = new();
        private long _reads, _readBytes, _writes, _writeBytes;
        private readonly ConcurrentDictionary<string, double> _savePhases = new(StringComparer.Ordinal);
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
            _listener.SetMeasurementEventCallback<double>((instrument, value, tags, state) =>
            {
                if (instrument.Name != "notepads.operation.duration") return;
                foreach (var tag in tags)
                    if (tag.Key == "operation" && tag.Value is string name && name.StartsWith("document.save", StringComparison.Ordinal))
                        _savePhases.AddOrUpdate(name, value, (_, total) => total + value);
            });
            _listener.Start();
        }
        public (long Reads, long ReadBytes, long Writes, long WriteBytes) Snapshot() =>
            (Interlocked.Read(ref _reads), Interlocked.Read(ref _readBytes), Interlocked.Read(ref _writes), Interlocked.Read(ref _writeBytes));
        public void ClearSavePhases() => _savePhases.Clear();
        public Dictionary<string, double> CopySavePhases() => new(_savePhases, StringComparer.Ordinal);
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
    public Dictionary<string, double> SaveMillisecondsByPhase { get; set; }
}

[JsonSerializable(typeof(List<PerformanceRow>))]
internal sealed partial class PerformanceJsonContext : JsonSerializerContext { }
