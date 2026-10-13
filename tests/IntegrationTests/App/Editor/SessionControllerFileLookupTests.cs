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
using Notepads.Composition;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Legacy;
using Notepads.Features.Sessions.Storage;
using Windows.Storage;
using Windows.Storage.AccessCache;

namespace NotepadsEditorTests;

internal static partial class SessionControllerTests
{
    private static async Task VerifyImportedFileLookupAsync(StringBuilder log)
    {
        await VerifyFileLookupDuringDiscoveryAsync();
        await VerifyEarlyRestoreIntentLookupAsync();
        await VerifyCanceledCallerLookupAsync();
        await VerifyFileLookupRetriesDiscoveryAsync();
        await VerifyFailedImportedFileLookupAsync(tokenOnly: false);
        await VerifyFailedImportedFileLookupAsync(tokenOnly: true);
        await VerifyInactiveFallbackFileLookupAsync();
        await VerifyInactiveFallbackFileLookupAsync(owned: true);
        await VerifyInactiveFallbackFileLookupAsync(owned: true, corruptPrimary: false);
        await VerifyInactiveFallbackFileLookupAsync(owned: true, failRecovery: true);
        await VerifyInterruptedFileLookupAsync(dispose: false);
        await VerifyInterruptedFileLookupAsync(dispose: true);
        log.AppendLine("PASS: early restore intent and owned/imported primary/fallback file lookup preserve recovery, actual bindings and cancellation.");
    }

    private static async Task VerifyCanceledCallerLookupAsync()
    {
        var fixture = await FileLookupFixture.CreateAsync();
        try
        {
            await using var host = new ControllerHost(fixture.Owner, new SessionRegistry());
            host.Controller.IsBackupEnabled = false;
            var preparing = NewLookupSignal();
            var release = NewLookupSignal();
            host.Controller.BeforeRestorePreparation = async (source, cancellation) =>
            {
                if (source.Id != fixture.ImportId) return;
                preparing.TrySetResult(null);
                await release.Task.WaitAsync(cancellation);
            };
            await host.Controller.LoadLastSessionAsync().WaitAsync(TimeSpan.FromSeconds(15));
            await preparing.Task.WaitAsync(TimeSpan.FromSeconds(10));
            using var request = new CancellationTokenSource();
            var canceled = host.Controller.ResolveFileAsync(fixture.File, request.Token);
            var surviving = host.Controller.ResolveFileAsync(fixture.File);
            request.Cancel();
            await ExpectLookupFailureAsync<OperationCanceledException>(canceled,
                "The resolver ignored its caller's captured cancellation.");
            SessionTestProtocol.Check(!surviving.IsCompleted,
                "Canceling one request canceled shared recovery or let another waiter report a false miss.");
            release.TrySetResult(null);
            SessionTestProtocol.Check((await surviving.WaitAsync(TimeSpan.FromSeconds(15)))?.Id == fixture.ImportId,
                "A caller's cancellation poisoned another lookup's shared recovery result.");
        }
        finally { await fixture.CleanupAsync(); }
    }

    private static async Task VerifyEarlyRestoreIntentLookupAsync()
    {
        var fixture = await FileLookupFixture.CreateAsync(tokenOnly: true, stalePath: true);
        try
        {
            await using var host = new ControllerHost(fixture.Owner, new SessionRegistry());
            host.Controller.IsBackupEnabled = false;
            var discovering = NewLookupSignal();
            var releaseDiscovery = NewLookupSignal();
            host.Controller.BeforeRecoveryDiscovery = async cancellation =>
            {
                discovering.TrySetResult(null);
                await releaseDiscovery.Task.WaitAsync(cancellation);
            };
            host.Controller.RequestRestoration();
            SessionTestProtocol.Check(!discovering.Task.IsCompleted && host.Core.GetTabCount() == 0,
                "Recording startup restore intent started discovery or created an editor.");
            var lookup = host.Controller.ResolveFileAsync(fixture.File);
            await discovering.Task.WaitAsync(TimeSpan.FromSeconds(10));
            SessionTestProtocol.Check(!lookup.IsCompleted,
                "Activation before LoadLastSession ignored synchronous startup restore intent.");
            releaseDiscovery.SetResult(null);
            var restored = await lookup.WaitAsync(TimeSpan.FromSeconds(15));
            SessionTestProtocol.Check(restored?.Id == fixture.ImportId &&
                restored.ReadyEditor.EditingFile.IsEqual(fixture.File) &&
                ((FakeTextEditor)restored.ReadyEditor).Text == FileLookupFixture.RecoveredText,
                "Early activation lost the moved-file grant or returned a disk miss for requested recovery.");
            await host.Controller.LoadLastSessionAsync().WaitAsync(TimeSpan.FromSeconds(15));
            SessionTestProtocol.Check(host.Core.GetTabs().Count(tab => tab.Id == fixture.ImportId) == 1,
                "Normal Loaded startup duplicated the recovery already requested by activation.");
        }
        finally { await fixture.CleanupAsync(); }
    }

    private static async Task VerifyFileLookupRetriesDiscoveryAsync()
    {
        var fixture = await FileLookupFixture.CreateAsync();
        try
        {
            await using var host = new ControllerHost(fixture.Owner, new SessionRegistry());
            host.Controller.IsBackupEnabled = false;
            host.Controller.BeforeRecoveryDiscovery = _ =>
                throw new IOException("Injected initial recovery discovery failure.");
            await ExpectLookupFailureAsync<IOException>(host.Controller.LoadLastSessionAsync(),
                "The discovery-retry fixture did not reach its injected startup failure.");
            host.Controller.BeforeRecoveryDiscovery = null;
            var restored = NewLookupSignal();
            host.Core.TabOpened += (_, tab) =>
            {
                if (tab.Id == fixture.ImportId) restored.TrySetResult(null);
            };

            await host.Controller.ResolveFileAsync(fixture.UnrelatedFile).WaitAsync(TimeSpan.FromSeconds(10));
            // Do not call WaitForRestorationAsync: it starts a stopped worker itself.
            await restored.Task.WaitAsync(TimeSpan.FromSeconds(15));
            SessionTestProtocol.Check(host.Core.GetTabs().Length == 2 &&
                host.Core.GetTabs().All(tab => tab.ReadyEditor != null) &&
                ((FakeTextEditor)host.Core.GetTabs().Single(tab => tab.Id == fixture.ImportId).ReadyEditor).Text ==
                    FileLookupFixture.RecoveredText,
                "No-match lookup retried discovery but left the newly admitted recovery worker stopped.");
        }
        finally { await fixture.CleanupAsync(); }
    }

    private static async Task VerifyFileLookupDuringDiscoveryAsync()
    {
        var fixture = await FileLookupFixture.CreateAsync();
        try
        {
            await using var host = new ControllerHost(fixture.Owner, new SessionRegistry());
            host.Controller.IsBackupEnabled = false;
            var discovering = NewLookupSignal();
            var releaseDiscovery = NewLookupSignal();
            var preparing = NewLookupSignal();
            var releasePreparation = NewLookupSignal();
            host.Controller.BeforeRecoveryDiscovery = async cancellation =>
            {
                discovering.SetResult(null);
                await releaseDiscovery.Task.WaitAsync(cancellation);
            };
            host.Controller.BeforeRestorePreparation = async (source, cancellation) =>
            {
                if (source.Id != fixture.ImportId) return;
                preparing.SetResult(null);
                await releasePreparation.Task.WaitAsync(cancellation);
            };

            var startup = host.Controller.LoadLastSessionAsync();
            await discovering.Task.WaitAsync(TimeSpan.FromSeconds(10));
            var matching = host.Controller.ResolveFileAsync(fixture.File);
            var unrelated = host.Controller.ResolveFileAsync(fixture.UnrelatedFile);
            SessionTestProtocol.Check(!matching.IsCompleted && !unrelated.IsCompleted,
                "File lookup bypassed requested recovery while its metadata discovery was blocked.");
            releaseDiscovery.SetResult(null);
            SessionTestProtocol.Check(await startup.WaitAsync(TimeSpan.FromSeconds(15)) == 1,
                "The lookup fixture did not return with only its selected import visible.");
            await preparing.Task.WaitAsync(TimeSpan.FromSeconds(10));
            SessionTestProtocol.Check(await unrelated.WaitAsync(TimeSpan.FromSeconds(10)) == null &&
                !matching.IsCompleted && host.Core.GetTabCount() == 1,
                "A matching lookup reported a miss, or an unrelated lookup waited for hidden payload I/O.");

            releasePreparation.SetResult(null);
            SessionTestProtocol.Check((await matching.WaitAsync(TimeSpan.FromSeconds(15)))?.Id == fixture.ImportId,
                "Resolver did not return the actual adopted matching editor.");
            await host.Controller.WaitForRestorationAsync();
            SessionTestProtocol.Check(host.Core.GetTabs().Count(tab => tab.Id == fixture.ImportId) == 1 &&
                ((FakeTextEditor)host.Core.GetTabs().Single(tab => tab.Id == fixture.ImportId).ReadyEditor).Text ==
                    FileLookupFixture.RecoveredText && host.Core.GetSelectedTab()?.Id == fixture.SelectedId,
                "File lookup lost recovered content, duplicated adoption, or stole the selected tab.");
        }
        finally { await fixture.CleanupAsync(); }
    }

    private static async Task VerifyFailedImportedFileLookupAsync(bool tokenOnly)
    {
        var fixture = await FileLookupFixture.CreateAsync(tokenOnly);
        var registry = new SessionRegistry();
        try
        {
            await using (var host = new ControllerHost(fixture.Owner, registry))
            {
                host.Controller.IsBackupEnabled = false;
                var preparing = NewLookupSignal();
                var failPreparation = NewLookupSignal();
                host.Controller.BeforeRestorePreparation = async (source, cancellation) =>
                {
                    if (source.Id != fixture.ImportId) return;
                    preparing.SetResult(null);
                    await failPreparation.Task.WaitAsync(cancellation);
                    throw new IOException("Injected hidden-import failure during file lookup.");
                };
                await host.Controller.LoadLastSessionAsync().WaitAsync(TimeSpan.FromSeconds(15));
                await preparing.Task.WaitAsync(TimeSpan.FromSeconds(10));
                var lookup = host.Controller.ResolveFileAsync(fixture.File);
                await host.Controller.ResolveFileAsync(fixture.UnrelatedFile)
                    .WaitAsync(TimeSpan.FromSeconds(10));
                SessionTestProtocol.Check(!lookup.IsCompleted,
                    "File lookup ignored a hidden import identified by its path or retained file grant.");
                failPreparation.SetResult(null);
                await ExpectLookupFailureAsync<IOException>(lookup,
                    "A failed matching import was treated as a safe file-open miss.");
                await host.Controller.WaitForRestorationAsync();
                await ExpectLookupFailureAsync<IOException>(host.Controller.ResolveFileAsync(fixture.File),
                    "A repeated lookup bypassed a retained failed import.");
                await host.Controller.ResolveFileAsync(fixture.UnrelatedFile)
                    .WaitAsync(TimeSpan.FromSeconds(10));
                SessionTestProtocol.Check(host.Controller.UnrecoveredEditorCount == 1 &&
                    host.Core.GetTabCount() == 1 &&
                    await FileIO.ReadTextAsync(fixture.Backup) == FileLookupFixture.RecoveredText,
                    "Failed lookup consumed its recovery source, exposed a second editor, or blocked unrelated files.");
            }

            {
                await using var restarted = new ControllerHost(fixture.Owner, registry);
                restarted.Controller.IsBackupEnabled = false;
                await restarted.Controller.LoadLastSessionAsync().WaitAsync(TimeSpan.FromSeconds(15));
                await restarted.Controller.WaitForRestorationAsync().WaitAsync(TimeSpan.FromSeconds(15));
                SessionTestProtocol.Check(restarted.Core.GetTabs().Length == 2 &&
                    ((FakeTextEditor)restarted.Core.GetTabs().Single(tab => tab.Id == fixture.ImportId).ReadyEditor)
                        .Text == FileLookupFixture.RecoveredText,
                    "A failed file lookup prevented its preserved import from recovering on restart.");
            }
        }
        finally { await fixture.CleanupAsync(); }
    }

    private static async Task VerifyInterruptedFileLookupAsync(bool dispose)
    {
        var fixture = await FileLookupFixture.CreateAsync();
        try
        {
            await using var host = new ControllerHost(fixture.Owner, new SessionRegistry());
            host.Controller.IsBackupEnabled = false;
            var preparing = NewLookupSignal();
            var releasePreparation = NewLookupSignal();
            host.Controller.BeforeRestorePreparation = async (source, cancellation) =>
            {
                if (source.Id != fixture.ImportId) return;
                preparing.TrySetResult(null);
                await releasePreparation.Task.WaitAsync(cancellation);
            };
            await host.Controller.LoadLastSessionAsync().WaitAsync(TimeSpan.FromSeconds(15));
            await preparing.Task.WaitAsync(TimeSpan.FromSeconds(10));
            var lookup = host.Controller.ResolveFileAsync(fixture.File);
            SessionTestProtocol.Check(!lookup.IsCompleted, "The closing fixture failed to block its matching lookup.");
            if (dispose)
            {
                await host.DisposeAsync();
                await ExpectLookupFailureAsync<OperationCanceledException>(lookup,
                    "Disposal left a hidden-import file lookup waiting indefinitely.");
                return;
            }

            host.Core.IsClosing = true;
            host.Controller.PauseRestoration();
            await ExpectLookupFailureAsync<OperationCanceledException>(lookup,
                "Close paused restoration without releasing the in-flight file lookup.");
            SessionTestProtocol.Check(host.Core.GetTabCount() == 1,
                "A canceled lookup promoted its hidden import while the workspace was closing.");
            host.Core.IsClosing = false;
            host.Controller.ResumeRestoration();
            var resumed = host.Controller.ResolveFileAsync(fixture.File);
            releasePreparation.SetResult(null);
            await resumed.WaitAsync(TimeSpan.FromSeconds(15));
            SessionTestProtocol.Check(host.Core.GetTabs().Count(tab => tab.Id == fixture.ImportId) == 1 &&
                ((FakeTextEditor)host.Core.GetTabs().Single(tab => tab.Id == fixture.ImportId).ReadyEditor).Text ==
                    FileLookupFixture.RecoveredText,
                "Canceling window close did not resume the retained import and a fresh lookup exactly once.");
        }
        finally { await fixture.CleanupAsync(); }
    }

    private static async Task VerifyInactiveFallbackFileLookupAsync(bool owned = false, bool corruptPrimary = true,
        bool failRecovery = false)
    {
        var targetOwner = Guid.NewGuid();
        var sourceOwner = owned ? targetOwner : Guid.NewGuid();
        var sourceId = Guid.NewGuid();
        var registry = new SessionRegistry();
        const string recovered = "older dirty content bound to A العربية😀\r";
        try
        {
            // Seed the target before the inactive source exists, so seeding cannot archive it.
            if (!owned)
            {
                await using var target = new ControllerHost(targetOwner, registry, primary: true);
                await target.Core.OpenAsync("selected primary document 日本語\r");
                SessionTestProtocol.Check(await target.Controller.SaveSessionAsync(),
                    "The primary fallback-lookup fixture did not publish its selected tab.");
            }
            var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync(
                SessionScopeData.GetSecondaryPrefix(sourceOwner) + "BackupFiles",
                CreationCollisionOption.FailIfExists);
            var fileA = await folder.CreateFileAsync("A.txt", CreationCollisionOption.FailIfExists);
            var fileB = await folder.CreateFileAsync("B.txt", CreationCollisionOption.FailIfExists);
            var unrelated = await folder.CreateFileAsync("unrelated.txt", CreationCollisionOption.FailIfExists);
            await FileIO.WriteTextAsync(fileA, "saved A text\r");
            await FileIO.WriteTextAsync(fileB, "saved B text\r");
            await using (var source = SessionTestProtocol.CreateService(sourceOwner, primary: owned))
            {
                await source.InitializeAuthorityAsync();
                await using var document = await SessionTestDocument.CreateAsync(sourceOwner, recovered);
                document.Metadata.HasEditingFile = true;
                document.Metadata.RequiresSaveAs = false;
                document.Metadata.IsModified = true;
                long goodLength;
                using (var earlier = document.Capture(sourceId, source.CurrentStamp))
                {
                    earlier.Documents[0].EditingFile = fileA;
                    earlier.Documents[0].EditingFileName = fileA.Name;
                    earlier.Documents[0].EditingFilePath = fileA.Path;
                    SessionTestProtocol.Check((await source.SaveAsync(earlier, CancellationToken.None)).Succeeded,
                        "The inactive source did not preserve its earlier A checkpoint.");
                    goodLength = checked((long)earlier.Documents[0].Recovery.Checkpoint.CommittedByteLength);
                }
                document.Editor.InsertText(document.Editor.Length, "newer text after SaveAs B\r");
                using (var latest = document.Capture(sourceId, source.CurrentStamp))
                {
                    latest.Documents[0].EditingFile = fileB;
                    latest.Documents[0].EditingFileName = fileB.Name;
                    latest.Documents[0].EditingFilePath = fileB.Path;
                    SessionTestProtocol.Check((await source.SaveAsync(latest, CancellationToken.None)).Succeeded,
                        "The inactive source did not publish its newer B checkpoint.");
                }
                await document.Editor.StopJournalAsync();
                if (corruptPrimary)
                {
                    using var stream = new FileStream(document.Journal.File.Path,
                        FileMode.Open, FileAccess.Write, FileShare.Read);
                    stream.SetLength(goodLength);
                    stream.Flush(true);
                }
            }

            await using var host = new ControllerHost(targetOwner, registry, primary: true);
            host.Controller.IsBackupEnabled = false;
            var preparing = NewLookupSignal();
            var releasePreparation = NewLookupSignal();
            host.Controller.BeforeRestorePreparation = async (source, cancellation) =>
            {
                if (source.Id != sourceId) return;
                SessionTestProtocol.Check(source.IsOwned == owned && source.Descriptor.EditingFilePath == fileB.Path &&
                    Enumerable.Range(0, source.FallbackCount)
                        .Any(index => source.GetDescriptor(index).EditingFilePath == fileA.Path),
                    "Discovery did not retain the newer B descriptor and its effective A fallback.");
                preparing.SetResult(null);
                await releasePreparation.Task.WaitAsync(cancellation);
                if (failRecovery) throw new IOException("Injected owned recovery failure.");
            };
            var startup = host.Controller.LoadLastSessionAsync();
            await preparing.Task.WaitAsync(TimeSpan.FromSeconds(10));
            if (!owned) await startup.WaitAsync(TimeSpan.FromSeconds(15));
            var lookup = host.Controller.ResolveFileAsync(fileA);
            var primaryLookup = host.Controller.ResolveFileAsync(fileB);
            await host.Controller.ResolveFileAsync(unrelated).WaitAsync(TimeSpan.FromSeconds(10));
            SessionTestProtocol.Check(!lookup.IsCompleted && !primaryLookup.IsCompleted && host.Core.GetTabCount() == 1,
                "File lookup skipped a pending primary or its effective fallback association.");
            releasePreparation.SetResult(null);
            if (failRecovery)
            {
                await ExpectLookupFailureAsync<IOException>(lookup,
                    "A failed owned fallback was treated as a disk miss.");
                await ExpectLookupFailureAsync<IOException>(primaryLookup,
                    "A failed owned primary was treated as a disk miss.");
                await startup.WaitAsync(TimeSpan.FromSeconds(15));
                await ExpectLookupFailureAsync<IOException>(host.Controller.ResolveFileAsync(fileA),
                    "A repeated lookup forgot a retained failed owned tab.");
                SessionTestProtocol.Check(host.Core.GetTabs().Single().State ==
                    Notepads.Presentation.Workspace.WorkspaceTabState.Failed,
                    "Failed owned recovery removed its visible retained tab.");
                return;
            }
            var matchedA = await lookup.WaitAsync(TimeSpan.FromSeconds(15));
            var matchedB = await primaryLookup.WaitAsync(TimeSpan.FromSeconds(15));
            await startup.WaitAsync(TimeSpan.FromSeconds(15));
            SessionTestProtocol.Check(corruptPrimary
                    ? matchedA?.Id == sourceId && matchedB == null
                    : matchedA == null && matchedB?.Id == sourceId,
                "Resolver returned the descriptor hint instead of rechecking the actual recovered file binding.");
            SessionTestProtocol.Check(host.Core.GetTabs().Count(tab => tab.Id == sourceId) == 1 &&
                ((FakeTextEditor)host.Core.GetTabs().Single(tab => tab.Id == sourceId).ReadyEditor).Text ==
                    (corruptPrimary ? recovered : recovered + "newer text after SaveAs B\r"),
                "Lookup changed the primary/fallback text or adopted it more than once.");
        }
        finally
        {
            await SessionTestProtocol.CleanupAsync(sourceOwner, targetOwner);
        }
    }

    private static TaskCompletionSource<object> NewLookupSignal() =>
        new(TaskCreationOptions.RunContinuationsAsynchronously);

    private static async Task ExpectLookupFailureAsync<TException>(Task lookup, string message)
        where TException : Exception
    {
        try { await lookup.WaitAsync(TimeSpan.FromSeconds(10)); }
        catch (TException) { return; }
        throw new InvalidOperationException(message);
    }

    private sealed class FileLookupFixture
    {
        public const string RecoveredText = "recovered hidden edits العربية😀\r";
        public Guid Owner { get; } = Guid.NewGuid();
        public Guid SelectedId { get; } = Guid.NewGuid();
        public Guid ImportId { get; } = Guid.NewGuid();
        public StorageFile File { get; private set; }
        public StorageFile UnrelatedFile { get; private set; }
        public StorageFile Backup { get; private set; }
        private string _token;

        public static async Task<FileLookupFixture> CreateAsync(bool tokenOnly = false, bool stalePath = false)
        {
            var fixture = new FileLookupFixture();
            try
            {
                var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync(
                    SessionScopeData.GetSecondaryPrefix(fixture.Owner) + "BackupFiles",
                    CreationCollisionOption.FailIfExists);
                fixture.File = await folder.CreateFileAsync("editing.txt", CreationCollisionOption.FailIfExists);
                await FileIO.WriteTextAsync(fixture.File, "older disk text\r");
                fixture.UnrelatedFile = await folder.CreateFileAsync("unrelated.txt", CreationCollisionOption.FailIfExists);
                fixture.Backup = await folder.CreateFileAsync("hidden-backup.txt", CreationCollisionOption.FailIfExists);
                await FileIO.WriteTextAsync(fixture.Backup, RecoveredText);
                var selected = await folder.CreateFileAsync("selected-backup.txt", CreationCollisionOption.FailIfExists);
                await FileIO.WriteTextAsync(selected, "selected ready 日本語\r");
                fixture._token = "lookup-" + fixture.Owner.ToString("N");
                StorageApplicationPermissions.FutureAccessList.AddOrReplace(fixture._token, fixture.File);
                if (tokenOnly)
                {
                    await fixture.File.RenameAsync("moved-editing.txt", NameCollisionOption.FailIfExists);
                    fixture.File = await folder.GetFileAsync("moved-editing.txt");
                    var resolved = await StorageApplicationPermissions.FutureAccessList.GetFileAsync(fixture._token);
                    SessionTestProtocol.Check(fixture.File.IsEqual(resolved),
                        "The isolated file grant did not track its renamed fixture.");
                }
                var legacy = new NotepadsSessionDataV1 { SelectedTextEditor = fixture.SelectedId };
                legacy.TextEditors.Add(new TextEditorSessionDataV1
                {
                    Id = fixture.SelectedId,
                    LastSavedBackupFilePath = selected.Path,
                    StateMetaData = Metadata("selected", hasFile: false)
                });
                legacy.TextEditors.Add(new TextEditorSessionDataV1
                {
                    Id = fixture.ImportId,
                    LastSavedBackupFilePath = fixture.Backup.Path,
                    EditingFileName = fixture.File.Name,
                    EditingFilePath = stalePath ? Path.Combine(folder.Path, "editing.txt") : tokenOnly ? null : fixture.File.Path,
                    EditingFileFutureAccessToken = fixture._token,
                    StateMetaData = Metadata("hidden", hasFile: true)
                });
                await SessionTestProtocol.WriteLegacySourceAsync(fixture.Owner, legacy);
                return fixture;
            }
            catch { await fixture.CleanupAsync(); throw; }
        }

        private static DocumentMetadata Metadata(string title, bool hasFile) => new()
        {
            LastSavedEncoding = "UTF-8",
            LastSavedLineEnding = "CRLF",
            FontZoomFactor = 1,
            RequiresSaveAs = !hasFile,
            HasEditingFile = hasFile,
            IsModified = hasFile,
            FileNamePlaceholder = title
        };

        public async Task CleanupAsync()
        {
            if (_token != null && StorageApplicationPermissions.FutureAccessList.ContainsItem(_token))
                StorageApplicationPermissions.FutureAccessList.Remove(_token);
            await SessionTestProtocol.CleanupAsync(Owner);
        }
    }
}
