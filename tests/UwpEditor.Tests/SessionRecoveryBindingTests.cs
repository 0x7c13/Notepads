// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Recovery;
using Notepads.Features.Sessions.Contracts.Legacy;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Windows.Storage;
using Windows.Storage.AccessCache;

namespace NotepadsEditorTests;

internal static class SessionRecoveryBindingTests
{
    private const string RecoveryText = "recovered العربية😀\r日本語";

    public static async Task RunAsync(StringBuilder log)
    {
        await CheckCurrentMissAsync(-1);
        await CheckCurrentMissAsync(0);
        await CheckPreparationFirstAsync();
        await CheckFallbackDetachmentAsync();
        await CheckLegacyMissAsync();
        await SessionRecoveryCancellationTests.RunAsync(log);
        log.AppendLine("PASS: recovery lookup and preparation share primary/fallback file bindings.");
        log.AppendLine("  Revoked grants stay detached; legacy text and rescue detachment are preserved.");
    }

    private static async Task CheckCurrentMissAsync(int fallbackIndex)
    {
        await using var fixture = await BindingFixture.CreateAsync();
        await using var original = await SessionTestDocument.CreateAsync(fixture.Owner, RecoveryText);
        original.Editor.GotoPos(original.Editor.Length);
        original.Editor.PasteText("\rjournal edit");
        using var capture = original.CaptureState();
        await capture.Checkpoint.FlushAsync();
        var descriptor = SessionDocumentStore.CaptureEditor(Guid.NewGuid(), capture, fixture.File.Name, fixture.File.Path);
        descriptor.EditingFileFutureAccessToken = fixture.Token;
        var source = CreateSource(fixture.Owner, descriptor, descriptor);

        StorageApplicationPermissions.FutureAccessList.Remove(fixture.Token);
        var shared = source.ResolveEditingFileAsync(fallbackIndex);
        SessionTestProtocol.Check(await shared == null, "A revoked grant unexpectedly resolved.");
        StorageApplicationPermissions.FutureAccessList.AddOrReplace(fixture.Token, fixture.File);
        SessionTestProtocol.Check(fixture.File.IsEqual(
            await StorageApplicationPermissions.FutureAccessList.GetFileAsync(fixture.Token)),
            "The restored fixture grant did not resolve to the original file.");

        using var prepared = await RecoveryDocumentPreparation.PrepareAsync(source, fixture.Owner,
            new DocumentLoadOptions(), CancellationToken.None, fallbackIndex);
        SessionTestProtocol.Check(prepared.EditingFile == null && prepared.FileAccessBinding == null &&
            ReferenceEquals(shared, source.ResolveEditingFileAsync(fallbackIndex)),
            "Preparation retried a revoked grant after lookup had detached its source.");
        await using var restored = await SessionTestDocument.FromPreparedAsync(fixture.Owner, prepared);
        SessionTestProtocol.Check(restored.Editor.GetText(200) == RecoveryText + "\rjournal edit",
            "A shared missing binding changed the recovered Unicode text or journal edits.");
        var otherIndex = fallbackIndex < 0 ? 0 : -1;
        SessionTestProtocol.Check(fixture.File.IsEqual(await source.ResolveEditingFileAsync(otherIndex)),
            "A missing binding leaked into another effective source descriptor.");
    }

    private static async Task CheckPreparationFirstAsync()
    {
        await using var fixture = await BindingFixture.CreateAsync();
        await using var original = await SessionTestDocument.CreateAsync(fixture.Owner, RecoveryText);
        using var capture = original.CaptureState();
        await capture.Checkpoint.FlushAsync();
        var descriptor = SessionDocumentStore.CaptureEditor(Guid.NewGuid(), capture, fixture.File.Name, fixture.File.Path);
        descriptor.EditingFileFutureAccessToken = fixture.Token;
        var source = CreateSource(fixture.Owner, descriptor);

        using var prepared = await RecoveryDocumentPreparation.PrepareAsync(source, fixture.Owner,
            new DocumentLoadOptions(), CancellationToken.None);
        StorageApplicationPermissions.FutureAccessList.AddOrReplace(fixture.Token, fixture.Backup);
        SessionTestProtocol.Check(fixture.File.IsEqual(prepared.EditingFile) &&
            ReferenceEquals(prepared.EditingFile, await source.ResolveEditingFileAsync()),
            "Lookup changed the file binding already used by preparation.");

        var fresh = CreateSource(fixture.Owner, descriptor);
        var resolutions = new Task<StorageFile>[8];
        await Task.WhenAll(Enumerable.Range(0, resolutions.Length).Select(index => Task.Run(() =>
        {
            resolutions[index] = fresh.ResolveEditingFileAsync();
        })));
        SessionTestProtocol.Check(resolutions.All(task => ReferenceEquals(task, resolutions[0])) &&
            fixture.Backup.IsEqual(await resolutions[0]),
            "Concurrent resolution did not share one task, or a binding escaped its source lifetime.");
    }

    private static async Task CheckFallbackDetachmentAsync()
    {
        await using var fixture = await BindingFixture.CreateAsync();
        await using var original = await SessionTestDocument.CreateAsync(fixture.Owner, RecoveryText);
        using var capture = original.CaptureState();
        await capture.Checkpoint.FlushAsync();
        var primary = SessionDocumentStore.CaptureEditor(Guid.NewGuid(), capture, fixture.File.Name, fixture.File.Path);
        primary.EditingFileFutureAccessToken = fixture.Token;
        var fallback = SessionRecoveryCatalog.CloneEditor(primary);
        fallback.Id = Guid.NewGuid();
        var lifetime = new RecoverySourceLifetime();
        var decision = new RecoveryDocumentDecision(RecoveryMembership.Owned, new(primary),
            new[] { new RecoveryDescriptor(fallback) }, Array.Empty<RecoveryDescriptor>(),
            primary.CaptureRevision, null, null, true);
        var source = new RecoveryDocumentSource(decision, null, true, new RecoveryStamp { ScopeId = fixture.Owner },
            null, lifetime);
        var originalHash = fallback.ComputeSha256();
        var effective = source.GetDescriptor(0);
        effective.EditingFileFutureAccessToken = fixture.Token;
        effective.EditingFilePath = fixture.File.Path;
        effective.StateMetaData.RequiresSaveAs = false;

        SessionTestProtocol.Check(source.GetEditingFilePath(0) == null &&
            await source.ResolveEditingFileAsync(0) == null && fallback.ComputeSha256() == originalHash,
            "A rescue fallback retained original-file authority or exposed mutable descriptor state.");
        using var prepared = await RecoveryDocumentPreparation.PrepareAsync(source, fixture.Owner,
            new DocumentLoadOptions(), CancellationToken.None, 0);
        SessionTestProtocol.Check(prepared.EditingFile == null && prepared.Metadata.RequiresSaveAs &&
            prepared.Metadata.IsModified && prepared.Id == primary.Id,
            "Preparation did not preserve the effective fallback's untitled identity.");

        lifetime.Disposed = true;
        try { _ = source.ResolveEditingFileAsync(0); }
        catch (ObjectDisposedException) { return; }
        throw new InvalidOperationException("A disposed recovery source returned its shared file binding.");
    }

    private static async Task CheckLegacyMissAsync()
    {
        await using var fixture = await BindingFixture.CreateAsync();
        var legacy = new TextEditorSessionDataV1
        {
            Id = Guid.NewGuid(),
            LastSavedBackupFilePath = fixture.Backup.Path,
            EditingFileName = fixture.File.Name,
            EditingFilePath = fixture.File.Path,
            EditingFileFutureAccessToken = fixture.Token,
            StateMetaData = new DocumentMetadata
            {
                HasEditingFile = true,
                IsModified = true,
                LastSavedEncoding = "UTF-8",
                LastSavedLineEnding = "CRLF",
                FontZoomFactor = 1
            }
        };
        var source = new RecoveryDocumentSource(null, legacy, false,
            new RecoveryStamp { ScopeId = fixture.Owner }, null, new RecoverySourceLifetime());
        StorageApplicationPermissions.FutureAccessList.Remove(fixture.Token);
        SessionTestProtocol.Check(await source.ResolveEditingFileAsync() == null,
            "A revoked legacy grant unexpectedly resolved.");
        StorageApplicationPermissions.FutureAccessList.AddOrReplace(fixture.Token, fixture.File);

        using var prepared = await RecoveryDocumentPreparation.PrepareAsync(source, fixture.Owner,
            new DocumentLoadOptions(), CancellationToken.None);
        using var stream = await prepared.SavedSnapshot.Baseline.OpenReadStreamAsync();
        using var reader = new StreamReader(stream, Encoding.UTF8);
        SessionTestProtocol.Check(prepared.EditingFile == null && prepared.FileAccessBinding == null &&
            await reader.ReadToEndAsync() == RecoveryText,
            "Legacy import retried the shared missing grant or changed its saved recovery text.");
    }

    private static RecoveryDocumentSource CreateSource(Guid owner, TextEditorSessionDataV2 primary,
        params TextEditorSessionDataV2[] fallbacks)
    {
        var decision = new RecoveryDocumentDecision(RecoveryMembership.Owned, new(primary),
            fallbacks.Select(row => new RecoveryDescriptor(row)), Array.Empty<RecoveryDescriptor>(),
            primary.CaptureRevision, null, null, true);
        return new(decision, null, true, new RecoveryStamp { ScopeId = owner }, null, new RecoverySourceLifetime());
    }

    private sealed class BindingFixture : IAsyncDisposable
    {
        public Guid Owner { get; } = Guid.NewGuid();
        public string Token => "recovery-binding-" + Owner.ToString("N");
        public StorageFolder Folder { get; private set; }
        public StorageFile File { get; private set; }
        public StorageFile Backup { get; private set; }

        public static async Task<BindingFixture> CreateAsync()
        {
            var fixture = new BindingFixture();
            try
            {
                fixture.Folder = await ApplicationData.Current.TemporaryFolder.CreateFolderAsync(
                    fixture.Token, CreationCollisionOption.FailIfExists);
                fixture.File = await fixture.Folder.CreateFileAsync("editing.txt");
                fixture.Backup = await fixture.Folder.CreateFileAsync("backup.txt");
                await FileIO.WriteTextAsync(fixture.File, "older disk text");
                await FileIO.WriteTextAsync(fixture.Backup, RecoveryText);
                StorageApplicationPermissions.FutureAccessList.AddOrReplace(fixture.Token, fixture.File);
                return fixture;
            }
            catch { await fixture.DisposeAsync(); throw; }
        }

        public async ValueTask DisposeAsync()
        {
            if (StorageApplicationPermissions.FutureAccessList.ContainsItem(Token))
                StorageApplicationPermissions.FutureAccessList.Remove(Token);
            if (Folder != null) await Folder.DeleteAsync(StorageDeleteOption.PermanentDelete);
            await SessionTestProtocol.CleanupAsync(Owner);
        }
    }
}
