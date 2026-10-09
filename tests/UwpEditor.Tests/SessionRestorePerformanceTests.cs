// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Composition;
using Notepads.Features.Documents.IO;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Windows.ApplicationModel;
using Windows.Storage;
using Windows.System;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using WinUIEditor;

namespace NotepadsEditorTests;

internal static partial class SessionControllerTests
{
    private const int SmallRestoreBytes = 57600;
    private const int LargeRestoreBytes = SmallRestoreBytes * 72;
    private static readonly Guid RestoreProcessInstance = Guid.NewGuid();

    // Each phase is one test-package process. Restoring publishes recovery state, so a fixture is
    // measured once; baseline/candidate repetitions need separate setup invocations and owner IDs.
    public static async Task RunRestorePerformanceAsync(string arguments, StringBuilder log)
    {
        SessionTestProtocol.Check(Package.Current.Id.Name == "Notepads.Editor.Tests",
            "Restore fixtures may only be used in the isolated editor test package.");
        var parts = arguments.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        SessionTestProtocol.Check(parts.Length >= 2 && Guid.TryParseExact(parts[1], "N", out _),
            "Restore performance arguments require a private owner GUID in N format.");
        var owner = Guid.ParseExact(parts[1], "N");
        SessionTestProtocol.Check(owner != Guid.Empty && !SessionScopeData.IsReservedOwner(owner),
            "Restore performance requires a nonreserved private owner.");
        var folder = Path.Combine(ApplicationData.Current.LocalFolder.Path, "RestorePerformance");
        Directory.CreateDirectory(folder);
        var prefix = Path.Combine(folder, owner.ToString("N"));
        if (parts[0] == "--restore-perf-setup")
        {
            SessionTestProtocol.Check(parts.Length == 3, "Setup requires a small, large or mixed shape.");
            await SetupRestoreFixtureAsync(owner, parts[2], prefix);
            log.AppendLine($"PASS: restore fixture setup owner={owner:N} shape={parts[2]} manifest={prefix}.json");
            return;
        }
        SessionTestProtocol.Check(parts.Length == 2, "This restore phase takes only its owner GUID.");
        var fixture = JsonSerializer.Deserialize(File.ReadAllText(prefix + ".json"),
            RestorePerformanceJsonContext.Default.RestoreFixture);
        ValidateRestoreFixture(fixture, owner, requireComplete: parts[0] != "--restore-perf-cleanup");
        switch (parts[0])
        {
            case "--restore-perf-measure":
                RequireFreshRestoreProcess(fixture.SetupProcessInstance);
                ReserveRestorePhase(prefix, "measure");
                var result = await MeasureControllerRestoreAsync(fixture);
                WriteRestoreJson(prefix + ".measure.json", JsonSerializer.Serialize(result,
                    RestorePerformanceJsonContext.Default.RestoreMeasurement));
                log.AppendLine($"PERF shape={fixture.Shape} attachMs={result.BatchAttachedMilliseconds:F2} " +
                    $"selectedPaintMs={result.SelectedPaintedMilliseconds:F2} controllerMs={result.ControllerReturnMilliseconds:F2} " +
                    $"sampledAppPeakDelta={result.Memory.SampledPeakAppDeltaBytes} results={prefix}.measure.json");
                log.AppendLine("PASS: isolated actual-controller/native restore; fresh process, warm/uncontrolled OS cache; " +
                    "test editor wrapper and tab host, not main-app activation or production tab-strip latency.");
                break;
            case "--restore-perf-verify":
            case "--restore-perf-verify-final":
                var measured = JsonSerializer.Deserialize(File.ReadAllText(prefix + ".measure.json"),
                    RestorePerformanceJsonContext.Default.RestoreMeasurement);
                SessionTestProtocol.Check(measured.OwnerId == owner && measured.SelectedEditorId == fixture.SelectedEditorId,
                    "Measurement owner or selection differs from the fixture.");
                RequireFreshRestoreProcess(fixture.SetupProcessInstance, measured.ProcessInstance);
                var final = parts[0] == "--restore-perf-verify-final";
                var expectedJournals = measured.CommittedJournals;
                if (final)
                {
                    var previous = JsonSerializer.Deserialize(File.ReadAllText(prefix + ".verify.json"),
                        RestorePerformanceJsonContext.Default.RestoreVerification);
                    SessionTestProtocol.Check(previous.OwnerId == owner && previous.ClosedEditorId == fixture.Documents[0].Id &&
                        previous.SelectedEditorId == fixture.SelectedEditorId,
                        "The previous restart did not durably close the expected nonselected tab.");
                    RequireFreshRestoreProcess(previous.ProcessInstance);
                    expectedJournals = previous.CommittedJournals?.Skip(1).ToArray();
                }
                var phase = final ? "verify-final" : "verify";
                ReserveRestorePhase(prefix, phase);
                var verification = await VerifyControllerRestoreAsync(fixture, final, expectedJournals);
                WriteRestoreJson(prefix + "." + phase + ".json", JsonSerializer.Serialize(verification,
                    RestorePerformanceJsonContext.Default.RestoreVerification));
                log.AppendLine($"PASS: {phase} owner={owner:N}; committed journal generations, ordered IDs, selected ID and Unicode hashes verified " +
                    $"without SaveSessionAsync; result={prefix}.{phase}.json");
                break;
            case "--restore-perf-cleanup":
                await CleanupRestoreFixtureAsync(owner);
                File.Delete(prefix + ".setup-complete.tmp");
                foreach (var completedPhase in new[] { "measure", "verify", "verify-final" })
                    File.Delete(prefix + "." + completedPhase + ".started");
                File.Delete(prefix + ".json");
                log.AppendLine($"PASS: cleaned only fixture owner={owner:N}; measurement/proof JSON retained in {folder}");
                break;
            default:
                throw new ArgumentException("Unknown restore performance phase.");
        }
    }

    private static int RestoreTabCount(string shape) => shape switch
    {
        "small" or "large" => 10,
        "mixed" => 12,
        _ => throw new ArgumentException("Restore shape must be small, large or mixed.")
    };

    private static int RestoreBaselineBytes(string shape, int index) =>
        shape == "large" || (shape == "mixed" && index % 2 == 0) ? LargeRestoreBytes : SmallRestoreBytes;

    private static byte[] RestorePage(int index)
    {
        var text = $"restore[{index:D2}] العربية😀 中文 café e\u0301 ";
        // Canonical CR-only UTF-8, 192 bytes per line and 300 lines per page.
        var line = text + new string('x', 191 - Encoding.UTF8.GetByteCount(text)) + "\r";
        return Encoding.UTF8.GetBytes(string.Concat(Enumerable.Repeat(line, 300)));
    }

    private static string RestorePrefix(int index) => $"journal[{index:D2}] Ελληνικά😀\r";
    private const string RestoreSuffix = "尾 العربية e\u0301 😀\r";

    private static async Task SetupRestoreFixtureAsync(Guid owner, string shape, string prefix)
    {
        var count = RestoreTabCount(shape);
        SessionTestProtocol.Check(!Directory.Exists(RecoveryRootStore.CreateStore().Paths.ScopePath(owner)) &&
            !File.Exists(Path.Combine(ApplicationData.Current.LocalFolder.Path, SessionScopeData.GetManifestFileName(owner))) &&
            !File.Exists(prefix + ".measure.json"), "Setup cannot overwrite an existing fixture or recovery scope.");
        var rows = new RestoreFixtureDocument[count];
        for (var index = 0; index < count; index++)
        {
            var page = RestorePage(index);
            var before = Encoding.UTF8.GetBytes(RestorePrefix(index));
            var after = Encoding.UTF8.GetBytes(RestoreSuffix);
            var bytes = RestoreBaselineBytes(shape, index);
            using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
            hash.AppendData(before);
            for (var block = 0; block < bytes / page.Length; block++) hash.AppendData(page);
            hash.AppendData(after);
            rows[index] = new RestoreFixtureDocument
            {
                Id = Guid.NewGuid(), BaselineBytes = bytes, FinalBytes = bytes + before.Length + after.Length,
                Sha256 = Convert.ToHexString(hash.GetHashAndReset())
            };
        }
        var fixture = new RestoreFixture
        {
            Version = 1, OwnerId = owner, Shape = shape, SelectedEditorId = rows[^2].Id,
            SetupProcessInstance = RestoreProcessInstance, SetupProcessId = Environment.ProcessId,
            SetupComplete = false, Documents = rows
        };
        // This initial manifest is the cleanup ownership receipt, including after interrupted setup.
        WriteRestoreJson(prefix + ".json", JsonSerializer.Serialize(fixture, RestorePerformanceJsonContext.Default.RestoreFixture));
        var completionCandidate = prefix + ".setup-complete.tmp";
        var documents = new List<SessionTestDocument>();
        try
        {
            await using (var service = SessionTestProtocol.CreateService(owner))
            {
                try
                {
                    await service.InitializeAuthorityAsync();
                    for (var index = 0; index < rows.Length; index++)
                    {
                        var page = RestorePage(index);
                        var bytes = rows[index].BaselineBytes;
                        using var baseline = await DocumentBaseline.CreateAsync(async destination =>
                        {
                            for (var block = 0; block < bytes / page.Length; block++) await destination.WriteAsync(page);
                        }, ownerId: owner);
                        var document = await SessionTestDocument.CreateAsync(owner, baseline);
                        documents.Add(document);
                        document.Metadata.FileNamePlaceholder = $"Restore {index:D2}.txt";
                        document.Editor.InsertText(0, RestorePrefix(index));
                        document.Editor.AppendText(Encoding.UTF8.GetByteCount(RestoreSuffix), RestoreSuffix);
                        await ValidateRestoreDocumentAsync(document, rows[index]);
                    }
                    using var capture = SessionTestProtocol.CaptureDocuments(documents, rows.Select(row => row.Id).ToArray(), service.CurrentStamp);
                    capture.SelectedEditorId = fixture.SelectedEditorId;
                    var saved = await service.SaveAsync(capture, CancellationToken.None);
                    SessionTestProtocol.Check(saved.Succeeded && saved.Changed, "Fixture publication did not commit.");
                }
                finally
                {
                    foreach (var document in documents) await document.DisposeAsync();
                }
            }
            fixture.SetupComplete = true;
            // Keep the ownership receipt readable until the complete, flushed same-directory
            // candidate replaces it in one rename. A crash can leave only this exact candidate.
            WriteRestoreJson(completionCandidate, JsonSerializer.Serialize(fixture, RestorePerformanceJsonContext.Default.RestoreFixture));
            File.Move(completionCandidate, prefix + ".json", overwrite: true);
        }
        catch
        {
            // A failed setup owns this receipt and scope; leave the receipt if cleanup itself fails.
            await CleanupRestoreFixtureAsync(owner);
            File.Delete(completionCandidate);
            File.Delete(prefix + ".json");
            throw;
        }
    }

    private static async Task CleanupRestoreFixtureAsync(Guid owner)
    {
        await SessionTestProtocol.CleanupAsync(owner);
        var localFolder = ApplicationData.Current.LocalFolder.Path;
        foreach (var path in new[]
        {
            RecoveryRootStore.CreateStore().Paths.ScopePath(owner),
            Path.Combine(localFolder, SessionScopeData.GetManifestFileName(owner)),
            Path.Combine(localFolder, SessionScopeData.GetSecondaryPrefix(owner) + "BackupFiles")
        })
            SessionTestProtocol.Check(!Path.Exists(path), "Fixture cleanup left an owned recovery path: " + path);
        var ownerPrefix = owner.ToString("N") + "-";
        foreach (var folderName in new[] { "DocumentBaselines", "DocumentJournals" })
        {
            var folder = Path.Combine(localFolder, folderName);
            if (!Directory.Exists(folder)) continue;
            // The shared helper intentionally tolerates pinned assets. A performance fixture must
            // keep its cleanup receipt until every asset it owns has actually been removed.
            var remaining = Directory.EnumerateFileSystemEntries(folder).FirstOrDefault(path =>
                Path.GetFileName(path).StartsWith(ownerPrefix, StringComparison.Ordinal));
            SessionTestProtocol.Check(remaining == null, "Fixture cleanup left an owned document asset: " + remaining);
        }
    }

    private static async Task<RestoreMeasurement> MeasureControllerRestoreAsync(RestoreFixture fixture)
    {
        var result = new RestoreMeasurement
        {
            OwnerId = fixture.OwnerId, Shape = fixture.Shape, ProcessId = Environment.ProcessId,
            ProcessInstance = RestoreProcessInstance, UtcStarted = DateTimeOffset.UtcNow,
            SelectedEditorId = fixture.SelectedEditorId,
            Documents = fixture.Documents.Select(row => new RestoreDocumentTiming
                { Id = row.Id, BaselineBytes = row.BaselineBytes, FinalBytes = row.FinalBytes }).ToArray()
        };
        var timings = result.Documents.ToDictionary(row => row.Id);
        var painted = new TaskCompletionSource<long>(TaskCreationOptions.RunContinuationsAsynchronously);
        var root = new Grid { Width = 640, Height = 360 };
        Window.Current.Content = root;
        Window.Current.Activate();
        root.UpdateLayout();
        var started = 0L;
        var attachmentStarted = false;
        EditorBaseControl selectedView = null;
        void OnSelectedPainted(Editor sender, PaintedEventArgs args)
        {
            var timestamp = Stopwatch.GetTimestamp();
            if (attachmentStarted && selectedView.ActualWidth > 0 && selectedView.ActualHeight > 0)
                painted.TrySetResult(timestamp);
        }
        await using (var host = new ControllerHost(fixture.OwnerId, new SessionRegistry()))
        {
            host.Core.RestoreStarting = id => timings[id].StartMilliseconds = ElapsedRestoreMilliseconds(started);
            host.Core.NativeRestoreReturned = id => timings[id].NativeRestoreReturnedMilliseconds = ElapsedRestoreMilliseconds(started);
            host.Core.RestoreReady = editor =>
            {
                timings[editor.Id].RestoreReadyMilliseconds = ElapsedRestoreMilliseconds(started);
                if (editor.Id != fixture.SelectedEditorId) return;
                selectedView = editor.Document.View;
                selectedView.Editor.Painted += OnSelectedPainted;
            };
            host.Core.SelectedAttached = () =>
            {
                SessionTestProtocol.Check(host.Core.GetSelectedReadyEditor()?.Id == fixture.SelectedEditorId && selectedView != null,
                    "The selected editor was not attached first.");
                attachmentStarted = true;
                root.Children.Add(selectedView);
                root.UpdateLayout();
                result.BatchAttachedMilliseconds = ElapsedRestoreMilliseconds(started);
                // Native presentation and the first paint do not wait for the background tabs.
            };
            await using var memory = new RestoreMemorySampler();
            await memory.StartAsync();
            try
            {
                try
                {
                    started = Stopwatch.GetTimestamp();
                    var restored = await host.Controller.LoadLastSessionAsync();
                    result.ControllerReturnMilliseconds = ElapsedRestoreMilliseconds(started);
                    var paintedAt = await painted.Task.WaitAsync(TimeSpan.FromSeconds(15));
                    result.SelectedPaintedMilliseconds = Stopwatch.GetElapsedTime(started, paintedAt).TotalMilliseconds;
                    await host.Controller.WaitForRestorationCheckpointAsync();
                    result.IntervalMilliseconds = ElapsedRestoreMilliseconds(started);
                    SessionTestProtocol.Check(restored == fixture.Documents.Length, "Measurement lost a restored tab.");
                }
                finally
                {
                    try
                    {
                        result.Memory = await memory.StopAsync();
                        result.Memory.LastSampleMilliseconds = ElapsedRestoreMilliseconds(started, memory.LastSampleTimestamp);
                    }
                    finally
                    {
                        if (selectedView != null) selectedView.Editor.Painted -= OnSelectedPainted;
                    }
                }
                // Hash readers and validation allocate only after the timed/sampled interval.
                SessionTestProtocol.Check(selectedView != null && selectedView.IsEnabled && !selectedView.Editor.ReadOnly &&
                    selectedView.ActualWidth > 0 && selectedView.ActualHeight > 0 && Window.Current.Visible &&
                    root.Children.Count == 1, "The selected native view is not visible, laid out and editable.");
                result.SelectedWidth = selectedView.ActualWidth;
                result.SelectedHeight = selectedView.ActualHeight;
                SessionTestProtocol.Check(result.BatchAttachedMilliseconds >= 0 && result.Documents.All(row =>
                    row.StartMilliseconds >= 0 && row.NativeRestoreReturnedMilliseconds >= row.StartMilliseconds &&
                    row.RestoreReadyMilliseconds >= row.NativeRestoreReturnedMilliseconds), "A restore timing endpoint was not observed.");
                await ValidateRestoredTabsAsync(host.Core, fixture, closedFirst: false);
                result.CommittedJournals = CaptureRestoredJournals(host.Core);
                await ValidateCommittedRestoreJournalsAsync(fixture, result.CommittedJournals, closedFirst: false);
            }
            finally { root.Children.Clear(); }
        }
        return result;
    }

    private static async Task<RestoreVerification> VerifyControllerRestoreAsync(RestoreFixture fixture, bool final,
        RestoreCommittedJournal[] expectedJournals)
    {
        var expected = final ? fixture.Documents.Skip(1).ToArray() : fixture.Documents;
        // Inspect the previous process's publication before this controller can replace its journals.
        await ValidateCommittedRestoreJournalsAsync(fixture, expectedJournals, closedFirst: final);
        RestoreCommittedJournal[] committedJournals;
        await using (var host = new ControllerHost(fixture.OwnerId, new SessionRegistry()))
        {
            SessionTestProtocol.Check(await host.Controller.LoadLastSessionAsync() == expected.Length,
                "The restart restored an unexpected tab count.");
            await host.Controller.WaitForRestorationCheckpointAsync();
            await ValidateRestoredTabsAsync(host.Core, fixture, closedFirst: final);
            committedJournals = CaptureRestoredJournals(host.Core);
            await ValidateCommittedRestoreJournalsAsync(fixture, committedJournals, closedFirst: final);
            if (!final)
            {
                var closing = (FakeTextEditor)host.Core.GetTabs()[0].ReadyEditor;
                SessionTestProtocol.Check(closing.Id != fixture.SelectedEditorId, "The verification tab must not be selected.");
                var closed = new HashSet<Guid>();
                SessionTestProtocol.Check(await host.Controller.PrepareExplicitCloseAsync(new[] { closing.Id }, closed) &&
                    closed.SetEquals(new[] { closing.Id }), "Verification could not commit the explicit close.");
                host.Core.Close(closing);
                await ValidateCommittedRestoreJournalsAsync(fixture, committedJournals.Skip(1).ToArray(), closedFirst: true);
            }
            // No explicit SaveSessionAsync: the controller's coalesced restoration checkpoint must suffice.
        }
        return new RestoreVerification
        {
            OwnerId = fixture.OwnerId, ProcessInstance = RestoreProcessInstance, ProcessId = Environment.ProcessId,
            ClosedEditorId = fixture.Documents[0].Id, SelectedEditorId = fixture.SelectedEditorId,
            VerifiedOrderedIds = expected.Select(row => row.Id).ToArray(), CommittedJournals = committedJournals
        };
    }

    private static RestoreCommittedJournal[] CaptureRestoredJournals(FakeNotepadsCore core) =>
        core.GetTabs().Select(tab => (FakeTextEditor)tab.ReadyEditor).Select(editor => new RestoreCommittedJournal
        {
            Id = editor.Id, JournalGenerationId = editor.Document.Journal.GenerationId
        }).ToArray();

    private static async Task ValidateCommittedRestoreJournalsAsync(RestoreFixture fixture,
        RestoreCommittedJournal[] expected, bool closedFirst)
    {
        var orderedIds = fixture.Documents.Skip(closedFirst ? 1 : 0).Select(row => row.Id).ToArray();
        SessionTestProtocol.Check(expected != null && expected.All(row => row != null && row.JournalGenerationId != Guid.Empty) &&
            expected.Select(row => row.Id).SequenceEqual(orderedIds) &&
            expected.Select(row => row.JournalGenerationId).Distinct().Count() == expected.Length,
            "Committed-journal proof has invalid generations or ordered IDs.");
        // Never use the service's cached snapshot for this proof. Every check takes fresh admission
        // and reads durable catalog authority; measurement calls this only after timing/sampling.
        await Task.Run(async () =>
        {
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var catalog = await SessionRecoveryCatalog.ReadScopeAsync(fixture.OwnerId);
                SessionTestProtocol.Check(!catalog.Blocked && catalog.SelectedCheckpoint != null &&
                    catalog.Session.Scope?.OwnerId == fixture.OwnerId &&
                    catalog.Session.SelectedTextEditor == fixture.SelectedEditorId &&
                    catalog.Session.TextEditors.Select(row => row.Id).SequenceEqual(orderedIds),
                    "Fresh durable catalog changed the fixture owner, ordered IDs or selected ID.");
                for (var index = 0; index < expected.Length; index++)
                {
                    var journal = catalog.Session.TextEditors[index].Journal;
                    SessionTestProtocol.Check(journal != null && journal.OwnerId == fixture.OwnerId &&
                        journal.GenerationId == expected[index].JournalGenerationId,
                        "Attachment did not publish a restored journal generation to the durable catalog.");
                }
            }
        });
    }

    private static async Task ValidateRestoredTabsAsync(FakeNotepadsCore core, RestoreFixture fixture, bool closedFirst)
    {
        var expected = closedFirst ? fixture.Documents.Skip(1).ToArray() : fixture.Documents;
        var actual = core.GetTabs().Select(tab => (FakeTextEditor)tab.ReadyEditor).ToArray();
        SessionTestProtocol.Check(actual.Select(editor => editor.Id).SequenceEqual(expected.Select(row => row.Id)) &&
            core.GetSelectedReadyEditor()?.Id == fixture.SelectedEditorId, "Restoration changed ordered IDs or the selected ID.");
        for (var index = 0; index < actual.Length; index++)
            await ValidateRestoreDocumentAsync(actual[index].Document, expected[index]);
    }

    private static async Task ValidateRestoreDocumentAsync(SessionTestDocument document, RestoreFixtureDocument expected)
    {
        SessionTestProtocol.Check(document.Editor.Length == expected.FinalBytes, "Restored UTF-8 byte count differs from the fixture.");
        using var stream = new EditorDocumentStream(document.Editor.AcquireUtf8Reader());
        var digest = Convert.ToHexString(await SHA256.HashDataAsync(stream));
        SessionTestProtocol.Check(digest == expected.Sha256, "Restored Unicode content differs from the canonical fixture digest.");
    }

    private static void ValidateRestoreFixture(RestoreFixture fixture, Guid owner, bool requireComplete)
    {
        SessionTestProtocol.Check(fixture != null && fixture.Version == 1 && fixture.OwnerId == owner &&
            fixture.SetupProcessInstance != Guid.Empty && (!requireComplete || fixture.SetupComplete),
            "Fixture ownership/version/setup is invalid.");
        SessionTestProtocol.Check(fixture.Documents != null && fixture.Documents.Length == RestoreTabCount(fixture.Shape) &&
            fixture.Documents.All(row => row != null && row.Id != Guid.Empty && row.Sha256?.Length == 64) &&
            fixture.Documents.Select(row => row.Id).Distinct().Count() == fixture.Documents.Length &&
            fixture.SelectedEditorId == fixture.Documents[^2].Id, "The fixture's ordered identities or selection are invalid.");
        for (var index = 0; index < fixture.Documents.Length; index++)
        {
            var row = fixture.Documents[index];
            SessionTestProtocol.Check(row.BaselineBytes == RestoreBaselineBytes(fixture.Shape, index) &&
                row.FinalBytes == row.BaselineBytes + Encoding.UTF8.GetByteCount(RestorePrefix(index) + RestoreSuffix) &&
                row.Sha256.All(character => char.IsAsciiHexDigit(character)), "Fixture byte counts or digest are invalid.");
        }
    }

    private static void RequireFreshRestoreProcess(params Guid[] previous) =>
        SessionTestProtocol.Check(previous.All(process => process != Guid.Empty && process != RestoreProcessInstance),
            "Each fixture phase must run in a fresh process.");

    private static void ReserveRestorePhase(string prefix, string phase) =>
        WriteRestoreJson(prefix + "." + phase + ".started", RestoreProcessInstance.ToString("N"));

    private static void WriteRestoreJson(string path, string text)
    {
        using var stream = new FileStream(path, FileMode.CreateNew, FileAccess.Write, FileShare.None);
        using var writer = new StreamWriter(stream, new UTF8Encoding(false));
        writer.Write(text);
        writer.Flush();
        stream.Flush(flushToDisk: true);
    }

    private static double ElapsedRestoreMilliseconds(long started, long? ended = null) =>
        Stopwatch.GetElapsedTime(started, ended ?? Stopwatch.GetTimestamp()).TotalMilliseconds;

    // MemoryManager and managed sampling run on the pool even while native construction occupies
    // the UI thread. This is a 20ms sampled peak, not a guaranteed process maximum.
    private sealed class RestoreMemorySampler : IAsyncDisposable
    {
        private readonly CancellationTokenSource _stop = new();
        private readonly TaskCompletionSource<bool> _ready = new(TaskCreationOptions.RunContinuationsAsynchronously);
        private readonly RestoreMemoryObservation _result = new();
        private Task _worker;
        public long LastSampleTimestamp { get; private set; }

        public async Task StartAsync()
        {
            _worker = Task.Run(async () =>
            {
                try
                {
                    Sample(initial: true);
                    _ready.SetResult(true);
                    try
                    {
                        while (true)
                        {
                            await Task.Delay(_result.SamplingIntervalMilliseconds, _stop.Token).ConfigureAwait(false);
                            Sample(initial: false);
                        }
                    }
                    catch (OperationCanceledException) when (_stop.IsCancellationRequested) { }
                    Sample(initial: false);
                }
                catch (Exception exception) { _ready.TrySetException(exception); throw; }
            });
            await _ready.Task;
        }

        private void Sample(bool initial)
        {
            var app = MemoryManager.AppMemoryUsage;
            var managed = GC.GetTotalMemory(false);
            var pressure = MemoryManager.AppMemoryUsageLevel;
            LastSampleTimestamp = Stopwatch.GetTimestamp();
            if (initial)
            {
                _result.InitialAppBytes = app;
                _result.InitialManagedBytes = managed;
                _result.InitialPressure = pressure.ToString();
            }
            _result.EndAppBytes = app;
            _result.EndManagedBytes = managed;
            _result.EndPressure = pressure.ToString();
            _result.HighestPressureValue = Math.Max(_result.HighestPressureValue, (int)pressure);
            _result.SampledPeakAppBytes = Math.Max(_result.SampledPeakAppBytes, app);
            _result.SampledPeakManagedBytes = Math.Max(_result.SampledPeakManagedBytes, managed);
            _result.SampleCount++;
        }

        public async Task<RestoreMemoryObservation> StopAsync()
        {
            _stop.Cancel();
            if (_worker != null) await _worker;
            return _result;
        }

        public async ValueTask DisposeAsync()
        {
            try { await StopAsync(); }
            finally { _stop.Dispose(); }
        }
    }
}

internal sealed class RestoreFixture
{
    public required int Version { get; set; }
    public required Guid OwnerId { get; set; }
    public required string Shape { get; set; }
    public required Guid SetupProcessInstance { get; set; }
    public required int SetupProcessId { get; set; }
    public required bool SetupComplete { get; set; }
    public required Guid SelectedEditorId { get; set; }
    public required RestoreFixtureDocument[] Documents { get; set; }
}

internal sealed class RestoreFixtureDocument
{
    public required Guid Id { get; set; }
    public required int BaselineBytes { get; set; }
    public required int FinalBytes { get; set; }
    public required string Sha256 { get; set; }
}

internal sealed class RestoreMeasurement
{
    public Guid OwnerId { get; set; }
    public string Shape { get; set; }
    public int ProcessId { get; set; }
    public Guid ProcessInstance { get; set; }
    public DateTimeOffset UtcStarted { get; set; }
    public string Scope { get; set; } = "Actual SessionController/native restore with test wrapper and tab host; not main-app launch.";
    public string CacheConditions { get; set; } = "Fresh process after separate fixture setup; OS cache warm/uncontrolled.";
    public Guid SelectedEditorId { get; set; }
    public double BatchAttachedMilliseconds { get; set; } = -1;
    public double SelectedPaintedMilliseconds { get; set; }
    public double ControllerReturnMilliseconds { get; set; }
    public double IntervalMilliseconds { get; set; }
    public double SelectedWidth { get; set; }
    public double SelectedHeight { get; set; }
    public RestoreDocumentTiming[] Documents { get; set; }
    public RestoreCommittedJournal[] CommittedJournals { get; set; }
    public RestoreMemoryObservation Memory { get; set; }
}

internal sealed class RestoreDocumentTiming
{
    public Guid Id { get; set; }
    public int BaselineBytes { get; set; }
    public int FinalBytes { get; set; }
    public double StartMilliseconds { get; set; } = -1;
    public double NativeRestoreReturnedMilliseconds { get; set; } = -1;
    public double RestoreReadyMilliseconds { get; set; } = -1;
}

internal sealed class RestoreMemoryObservation
{
    public int SamplingIntervalMilliseconds { get; set; } = 20;
    public string Description { get; set; } = "Off-UI MemoryManager.AppMemoryUsage and GC.GetTotalMemory(false); sampled peaks only.";
    public int SampleCount { get; set; }
    public ulong InitialAppBytes { get; set; }
    public ulong SampledPeakAppBytes { get; set; }
    public ulong EndAppBytes { get; set; }
    public long SampledPeakAppDeltaBytes => (long)SampledPeakAppBytes - (long)InitialAppBytes;
    public long EndAppDeltaBytes => (long)EndAppBytes - (long)InitialAppBytes;
    public long InitialManagedBytes { get; set; }
    public long SampledPeakManagedBytes { get; set; }
    public long EndManagedBytes { get; set; }
    public long SampledPeakManagedDeltaBytes => SampledPeakManagedBytes - InitialManagedBytes;
    public long EndManagedDeltaBytes => EndManagedBytes - InitialManagedBytes;
    public string InitialPressure { get; set; }
    public string EndPressure { get; set; }
    public int HighestPressureValue { get; set; }
    public double LastSampleMilliseconds { get; set; }
}

internal sealed class RestoreVerification
{
    public Guid OwnerId { get; set; }
    public Guid ProcessInstance { get; set; }
    public int ProcessId { get; set; }
    public Guid ClosedEditorId { get; set; }
    public Guid SelectedEditorId { get; set; }
    public Guid[] VerifiedOrderedIds { get; set; }
    public RestoreCommittedJournal[] CommittedJournals { get; set; }
}

internal sealed class RestoreCommittedJournal
{
    public required Guid Id { get; set; }
    public required Guid JournalGenerationId { get; set; }
}

[JsonSourceGenerationOptions(WriteIndented = true, UnmappedMemberHandling = JsonUnmappedMemberHandling.Disallow)]
[JsonSerializable(typeof(RestoreFixture))]
[JsonSerializable(typeof(RestoreMeasurement))]
[JsonSerializable(typeof(RestoreVerification))]
internal sealed partial class RestorePerformanceJsonContext : JsonSerializerContext { }
