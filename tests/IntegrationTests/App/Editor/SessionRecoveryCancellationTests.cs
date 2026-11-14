// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Recovery;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Windows.Foundation;
using Windows.Storage;
using WinUIEditor;

namespace NotepadsEditorTests;

internal static class SessionRecoveryCancellationTests
{
    public static async Task RunAsync(StringBuilder log)
    {
        await CheckReadCancellationAsync(fork: false, recoveryBaseline: false);
        await CheckReadCancellationAsync(fork: false, recoveryBaseline: true);
        await CheckReadCancellationAsync(fork: true, recoveryBaseline: false);
        await CheckReadCancellationAsync(fork: true, recoveryBaseline: true);
        await CheckCheckpointCancellationAsync(afterCompletion: false);
        await CheckCheckpointCancellationAsync(afterCompletion: true);
        log.AppendLine("PASS: recovery cancellation stops baseline/fork I/O and drains native validation.");
        log.AppendLine("  Late results are disposed; leases unwind and exact Unicode text remains retryable.");
    }

    private static async Task CheckReadCancellationAsync(bool fork, bool recoveryBaseline)
    {
        await using var fixture = await CancellationFixture.CreateAsync();
        using var cancellation = new CancellationTokenSource();
        var watched = recoveryBaseline ? fixture.RecoveryPath : fixture.SavedPath;
        var owner = fork ? fixture.TargetOwner : fixture.SourceOwner;
        var reads = 0;
        var bytesRead = 0;
        var forwarded = false;
        var protectedDuringRead = false;
        var candidateCreated = false;
        void AfterRead(string path, int count, CancellationToken token)
        {
            if (!string.Equals(path, watched, StringComparison.OrdinalIgnoreCase)) return;
            reads++;
            bytesRead += count;
            if (reads != 1) return;
            forwarded = token == cancellation.Token;
            protectedDuringRead = DocumentAssetLease.TestRecoveryAssets(fixture.SourceOwner)
                .Any(asset => asset.Path == watched && asset.Leases > 0);
            candidateCreated = DocumentAssetLease.TestRecoveryAssets(fixture.TargetOwner).Any(asset => asset.Leases > 0);
            cancellation.Cancel();
        }

        try
        {
            if (fork) SessionRecoveryCancellationHooks.LeasedRead = AfterRead;
            else SessionRecoveryCancellationHooks.BaselineRead = AfterRead;
            await ExpectCanceledAsync(() => fixture.PrepareAsync(owner, cancellation.Token));
        }
        finally { SessionRecoveryCancellationHooks.Reset(); }

        Check(forwarded && reads == 1 && bytesRead > 0 && bytesRead < fixture.Descriptor.RecoveryBaseline.ByteLength,
            "Recovery ignored cancellation after its first real payload read or continued reading the document.");
        Check(protectedDuringRead && (!fork || candidateCreated),
            "Cancellation did not exercise leased payload I/O and a real fork candidate.");
        await fixture.CheckUnwoundAsync();
        await fixture.CheckRetryAsync(owner);
    }

    private static async Task CheckCheckpointCancellationAsync(bool afterCompletion)
    {
        await using var fixture = await CancellationFixture.CreateAsync();
        using var cancellation = new CancellationTokenSource();
        IAsyncOperation<EditorJournalCheckpoint> operation = null;
        EditorJournalCheckpoint result = null;
        var forwarded = false;
        var journalProtected = false;
        SessionRecoveryCancellationHooks.CheckpointOpening = (opened, token) =>
        {
            operation = opened;
            forwarded = token == cancellation.Token;
            journalProtected = DocumentAssetLease.TestRecoveryAssets(fixture.SourceOwner)
                .Any(asset => asset.Path == fixture.JournalPath && asset.Leases >= 2);
            if (!afterCompletion) cancellation.Cancel();
        };
        SessionRecoveryCancellationHooks.CheckpointOpened = (checkpoint, token) =>
        {
            result = checkpoint;
            if (afterCompletion) cancellation.Cancel();
        };
        try { await ExpectCanceledAsync(() => fixture.PrepareAsync(fixture.SourceOwner, cancellation.Token)); }
        finally { SessionRecoveryCancellationHooks.Reset(); }

        Check(forwarded && journalProtected && operation != null && operation.Status != AsyncStatus.Started,
            "Canceled checkpoint validation returned before native completion or without its retained journal lease.");
        if (afterCompletion) Check(result != null, "The late cancellation did not follow successful native validation.");
        if (result != null)
        {
            var closed = false;
            try { _ = result.CommittedByteLength; }
            catch (Exception ex) when (ex is ObjectDisposedException || ex.HResult == unchecked((int)0x80000013)) { closed = true; }
            Check(closed, "A checkpoint returned during cancellation was not disposed.");
        }
        await fixture.CheckUnwoundAsync();
        await fixture.CheckRetryAsync(fixture.SourceOwner);
    }

    private static async Task ExpectCanceledAsync(Func<Task<PreparedRecoveryDocument>> prepare)
    {
        // Always await the owned operation before fixture cleanup can retire its assets.
        try { using var unexpected = await prepare(); }
        catch (OperationCanceledException) { return; }
        throw new InvalidOperationException("Canceled payload preparation unexpectedly returned an owned document.");
    }

    private static void Check(bool condition, string message) => SessionTestProtocol.Check(condition, message);

    private sealed class CancellationFixture : IAsyncDisposable
    {
        // Three read chunks plus Unicode, large enough for a second read but bounded well below 1 MiB.
        private static readonly string SavedText = new string('s', 192 * 1024) + "\r保存 العربية😀";
        private static readonly string RecoveryText = new string('r', 192 * 1024) + "\r恢復 日本語😀";
        private const string JournalText = "\rjournal Ωالعربية😀";
        private readonly RecoverySourceLifetime _lifetime = new();
        private IDisposable _sourceWriter;
        private IDisposable _targetWriter;
        public Guid SourceOwner { get; } = Guid.NewGuid();
        public Guid TargetOwner { get; } = Guid.NewGuid();
        public TextEditorSessionDataV2 Descriptor { get; private set; }
        private RecoveryDocumentSource Source { get; set; }
        public string SavedPath => AssetPath("DocumentBaselines", Descriptor.SavedBaseline.FileName);
        public string RecoveryPath => AssetPath("DocumentBaselines", Descriptor.RecoveryBaseline.FileName);
        public string JournalPath => AssetPath("DocumentJournals", Descriptor.Journal.FileName);

        public static async Task<CancellationFixture> CreateAsync()
        {
            var fixture = new CancellationFixture();
            try
            {
                fixture._sourceWriter = await SessionScopeLease.AcquireWriterAsync(fixture.SourceOwner);
                fixture._targetWriter = await SessionScopeLease.AcquireWriterAsync(fixture.TargetOwner);
                using var saved = await DocumentBaseline.FromTextAsync(SavedText, fixture.SourceOwner);
                await using var original = await SessionTestDocument.CreateAsync(fixture.SourceOwner, RecoveryText);
                original.Editor.GotoPos(original.Editor.Length);
                original.Editor.PasteText(JournalText);
                using var capture = original.CaptureState();
                await capture.Checkpoint.FlushAsync();
                capture.PreserveForRecovery();
                saved.PreserveForRecovery();
                fixture.Descriptor = SessionDocumentStore.CaptureEditor(Guid.NewGuid(), capture, "cancellation", null);
                fixture.Descriptor.SavedBaseline = SessionDocumentStore.CaptureBaseline(saved);
                var decision = new RecoveryDocumentDecision(RecoveryMembership.Owned, new(fixture.Descriptor),
                    Array.Empty<RecoveryDescriptor>(), Array.Empty<RecoveryDescriptor>(),
                    fixture.Descriptor.CaptureRevision, null, null, true);
                fixture.Source = new RecoveryDocumentSource(decision, null, true,
                    new RecoveryStamp { ScopeId = fixture.SourceOwner, EpochId = fixture.SourceOwner },
                    null, fixture._lifetime);
                return fixture;
            }
            catch { await fixture.DisposeAsync(); throw; }
        }

        public Task<PreparedRecoveryDocument> PrepareAsync(Guid owner, CancellationToken cancellation) =>
            Task.Run(() => RecoveryDocumentPreparation.PrepareAsync(Source, owner, new DocumentLoadOptions(), cancellation));

        public async Task CheckUnwoundAsync()
        {
            foreach (var owner in new[] { SourceOwner, TargetOwner })
            {
                var assets = DocumentAssetLease.TestRecoveryAssets(owner);
                Check(assets.All(asset => asset.Leases == 0), "Canceled preparation leaked a document asset lease.");
                await Task.WhenAll(assets.Select(asset => asset.Retirement)).WaitAsync(TimeSpan.FromSeconds(15));
            }
            Check(File.Exists(SavedPath) && File.Exists(RecoveryPath) && File.Exists(JournalPath),
                "Canceled preparation deleted an original recovery asset.");
            foreach (var folder in new[] { "DocumentBaselines", "DocumentJournals" })
                Check(!Directory.EnumerateFiles(Path.Combine(ApplicationData.Current.LocalFolder.Path, folder),
                    TargetOwner.ToString("N") + "-*").Any(), "Canceled preparation retained an unpublished target candidate.");
        }

        public async Task CheckRetryAsync(Guid owner)
        {
            using var prepared = await PrepareAsync(owner, CancellationToken.None);
            Check(await prepared.SavedSnapshot.ReadTextAsync() == SavedText, "Cancellation changed the saved baseline.");
            await using var restored = await SessionTestDocument.FromPreparedAsync(owner, prepared);
            Check(restored.Editor.GetText(1024 * 1024) == RecoveryText + JournalText,
                "An uncanceled retry did not restore the original Unicode baseline and committed journal prefix.");
        }

        public async ValueTask DisposeAsync()
        {
            SessionRecoveryCancellationHooks.Reset();
            _lifetime.Disposed = true;
            await Task.WhenAll(DocumentAssetLease.TestRecoveryAssets(SourceOwner).Concat(
                DocumentAssetLease.TestRecoveryAssets(TargetOwner)).Select(asset => asset.Retirement));
            _targetWriter?.Dispose();
            _sourceWriter?.Dispose();
            await SessionTestProtocol.CleanupAsync(SourceOwner, TargetOwner);
        }

        private static string AssetPath(string folder, string file) =>
            Path.Combine(ApplicationData.Current.LocalFolder.Path, folder, file);
    }
}
