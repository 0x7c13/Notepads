// Copyright (c) 2026, Jiaqi (0x7c13) Liu. See LICENSE.
using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Transfer;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Views.MainPage;
using Notepads.Presentation.Workspace;
using Windows.Storage;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static partial class WorkspaceFileOpeningTests
{
    internal static async Task RunAsync(NotepadsMainPage page, SessionService service, StringBuilder log)
    {
        await UntilAsync(() => page.TestStartupComplete, "File-opening MainPage startup did not finish.");
        page.TestController.StopSessionBackup();
        await page.TestController.WaitForRestorationCheckpointAsync().WaitAsync(TimeSpan.FromSeconds(15));
        page.TestController.IsBackupEnabled = false;
        var core = (NotepadsCore)page.TestCore;
        await VerifyOrdinaryCoalescingAsync(page, core, failure: false);
        await VerifyOrdinaryCoalescingAsync(page, core, failure: true);
        await VerifyOrdinaryBeforeTransferAsync(core, service);
        await VerifyTransferBeforeOrdinaryAsync(page, core, service, reset: false);
        await VerifyReceiptDuringCloseAsync(page, core, service);
        await VerifyGenerationCancellationAsync(page, core, reset: false);
        await VerifyGenerationCancellationAsync(page, core, reset: true);
        await VerifyPreparedTransferResetAsync(page, core, service);
        await VerifyTransferBeforeOrdinaryAsync(page, core, service, reset: true);
        await VerifyReceiptFailureAsync(core, service);
        log.AppendLine("PASS: actual Core admission coalesces results, serializes ordinary/transfer opens and honors close/reset generations.");
        await VerifyPresentationIndependentShutdownAsync(page, core, service);
        log.AppendLine("PASS: close and disposal finish committed transfers before native shutdown; ordinary result presentation remains independent.");
        await VerifySnapshotOffTransferClosesAsync(log);
    }

    private static async Task VerifyOrdinaryCoalescingAsync(NotepadsMainPage page, NotepadsCore core, bool failure)
    {
        var file = await NewFileAsync(failure ? "failure" : "ordinary");
        var preparing = Signal();
        var releasePreparation = Signal();
        var presenting = Signal();
        var releasePresentation = Signal();
        var preparationCount = 0;
        var presentationCount = 0;
        var selectedCount = 0;
        var before = core.TestOwnedEditorCount;
        void Selected(object sender, WorkspaceTab tab)
        {
            if (tab.ReadyEditor?.EditingFile?.IsEqual(file) == true) selectedCount++;
        }
        core.TabSelected += Selected;
        core.BeforeFilePreparation = async () =>
        {
            preparationCount++;
            preparing.TrySetResult(null);
            await releasePreparation.Task;
            if (failure) throw new IOException("Injected ordinary file preparation failure.");
        };
        WorkspaceFileOpenResult outcome = null;
        var leader = core.OpenFileAsync(file, async result =>
        {
            outcome = result;
            presentationCount++;
            presenting.TrySetResult(null);
            await releasePresentation.Task;
        });
        try
        {
            await preparing.Task.WaitAsync(TimeSpan.FromSeconds(10));
            var joined = page.OpenFileAsync(file);
            Check(!joined.IsCompleted && core.FindReadyTab(file) == null,
                "Concurrent MainPage opening bypassed Core's in-flight candidate.");
            releasePreparation.TrySetResult(null);
            await presenting.Task.WaitAsync(TimeSpan.FromSeconds(15));
            var lateJoined = page.OpenFileAsync(file);
            Check(!leader.IsCompleted && !joined.IsCompleted && !lateJoined.IsCompleted &&
                preparationCount == 1 && presentationCount == 1,
                "A result callback released coalescing before its one presentation completed.");
            Check(outcome.Kind == (failure ? WorkspaceFileOpenKind.Failed : WorkspaceFileOpenKind.Created) &&
                core.TestOwnedEditorCount == before + (failure ? 0 : 1) && selectedCount == (failure ? 0 : 1),
                "Coalesced opening duplicated native preparation/selection or lost its typed outcome.");
            releasePresentation.TrySetResult(null);
            Check(await leader.WaitAsync(TimeSpan.FromSeconds(10)) == !failure && !await joined && !await lateJoined,
                "Joined ordinary calls reported that they created the leader's editor.");
            if (failure)
            {
                core.BeforeFilePreparation = null;
                Check(await page.OpenFileAsync(file).WaitAsync(TimeSpan.FromSeconds(15)),
                    "A completed failed request prevented a subsequent explicit retry.");
            }
        }
        finally
        {
            releasePreparation.TrySetResult(null);
            releasePresentation.TrySetResult(null);
            core.BeforeFilePreparation = null;
            core.TabSelected -= Selected;
        }
    }

    private static async Task VerifyOrdinaryBeforeTransferAsync(NotepadsCore core, SessionService service)
    {
        var file = await NewFileAsync("ordinary-first");
        await using var source = await TransferFixture.CreateAsync(core, service, file);
        var preparing = Signal();
        var release = Signal();
        core.BeforeFilePreparation = async () => { preparing.TrySetResult(null); await release.Task; };
        try
        {
            var ordinary = core.OpenFileAsync(file, _ => Task.CompletedTask);
            await preparing.Task.WaitAsync(TimeSpan.FromSeconds(10));
            var transfer = core.OpenTransferredFileAsync(file, source.PrepareAsync, source.PublishAsync, -1, default);
            Check(!transfer.IsCompleted && source.PreparationCount == 0,
                "Transfer prepared while an ordinary open held the same file admission.");
            release.TrySetResult(null);
            Check(await ordinary.WaitAsync(TimeSpan.FromSeconds(15)) &&
                (await transfer.WaitAsync(TimeSpan.FromSeconds(15))).Kind == WorkspaceFileOpenKind.Existing &&
                source.PreparationCount == 0 && source.ReceiptCount == 0,
                "Losing transfer published or materialized after an ordinary open was admitted.");
            Check(!await TransferRecoveryStore.HasReceiptAsync(source.Token) && source.Editor.GetText() == source.Text,
                "Rejected transfer lost its source or created a durable receipt authorizing source removal.");
            VerifyUnique(core, file);
        }
        finally { release.TrySetResult(null); core.BeforeFilePreparation = null; }
    }

    private static async Task VerifyTransferBeforeOrdinaryAsync(NotepadsMainPage page, NotepadsCore core,
        SessionService service, bool reset)
    {
        var file = await NewFileAsync(reset ? "receipt-reset" : "transfer-first");
        await using var source = await TransferFixture.CreateAsync(core, service, file);
        var receiptStarted = Signal();
        var releaseReceipt = Signal();
        var stamp = service.CurrentStamp.EpochId;
        var installedBeforeReset = false;
        void Opened(object sender, WorkspaceTab tab)
        {
            if (tab.ReadyEditor?.EditingFile?.IsEqual(file) == true)
                installedBeforeReset = service.CurrentStamp.EpochId == stamp;
        }
        core.TabOpened += Opened;
        var transfer = core.OpenTransferredFileAsync(file, source.PrepareAsync, async editor =>
        {
            await source.PublishAsync(editor);
            receiptStarted.TrySetResult(null);
            await releaseReceipt.Task;
        }, -1, default);
        try
        {
            await receiptStarted.Task.WaitAsync(TimeSpan.FromSeconds(15));
            Check(await TransferRecoveryStore.HasReceiptAsync(source.Token) && core.FindReadyTab(file) == null,
                "Receipt fixture did not hold the real committed receipt before candidate installation.");
            var ordinary = page.OpenFileAsync(file);
            Check(!ordinary.IsCompleted, "Ordinary open passed a transfer's committed receipt boundary.");
            Task clearing = null;
            if (reset)
            {
                clearing = page.TestController.ClearSessionDataAsync();
                Check(!clearing.IsCompleted && service.CurrentStamp.EpochId == stamp,
                    "Reset retired the target epoch while a committed transfer was awaiting installation.");
            }
            else
            {
                var unrelated = await NewFileAsync("receipt-unrelated");
                Check(await page.OpenFileAsync(unrelated).WaitAsync(TimeSpan.FromSeconds(10)),
                    "A transfer receipt blocked admission of a different file.");
            }
            releaseReceipt.TrySetResult(null);
            var result = await transfer.WaitAsync(TimeSpan.FromSeconds(15));
            Check(result.Kind == WorkspaceFileOpenKind.Created && installedBeforeReset &&
                result.Tab.ReadyEditor.GetText() == source.Text && source.Editor.GetText() == source.Text,
                "Committed transfer was discarded after publication or installed after its epoch retired.");
            Check(!await ordinary.WaitAsync(TimeSpan.FromSeconds(15)),
                "Concurrent ordinary open created an editor beside a committed transfer.");
            if (clearing != null)
            {
                await clearing.WaitAsync(TimeSpan.FromSeconds(15));
                Check(service.CurrentStamp.EpochId != stamp && core.ContainsTab(result.Tab),
                    "Reset failed to retire its epoch while preserving the installed transferred editor.");
            }
            VerifyUnique(core, file);
            Check(await FileIO.ReadTextAsync(file) == DiskText, "Transfer overwrote the original disk text.");
        }
        finally { releaseReceipt.TrySetResult(null); core.TabOpened -= Opened; }
    }

    private static async Task VerifyGenerationCancellationAsync(NotepadsMainPage page, NotepadsCore core, bool reset)
    {
        var file = await NewFileAsync(reset ? "preparation-reset" : "preparation-close");
        var preparing = Signal();
        var release = Signal();
        var preparations = 0;
        var stalePresentations = 0;
        core.BeforeFilePreparation = async () =>
        {
            if (++preparations != 1) return;
            preparing.TrySetResult(null);
            await release.Task;
        };
        try
        {
            var old = core.OpenFileAsync(file, _ => { stalePresentations++; return Task.CompletedTask; });
            await preparing.Task.WaitAsync(TimeSpan.FromSeconds(10));
            if (reset) await page.TestController.ClearSessionDataAsync().WaitAsync(TimeSpan.FromSeconds(15));
            else
            {
                core.IsClosing = true;
                page.TestController.PauseRestoration();
                core.IsClosing = false;
                page.TestController.ResumeRestoration();
            }
            var fresh = page.OpenFileAsync(file);
            Check(!old.IsCompleted && !fresh.IsCompleted && preparations == 1,
                "A renewed generation reused a canceled request or removed its still-held file lease.");
            release.TrySetResult(null);
            Check(!await old.WaitAsync(TimeSpan.FromSeconds(15)) &&
                await fresh.WaitAsync(TimeSpan.FromSeconds(15)) && preparations == 2 && stalePresentations == 0,
                "A stale candidate survived reset/close or prevented a fresh request after unwinding.");
            VerifyUnique(core, file);
        }
        finally { release.TrySetResult(null); core.BeforeFilePreparation = null; }
    }

    private static async Task VerifyReceiptDuringCloseAsync(NotepadsMainPage page, NotepadsCore core, SessionService service)
    {
        var file = await NewFileAsync("receipt-close");
        await using var source = await TransferFixture.CreateAsync(core, service, file);
        var committed = Signal();
        var release = Signal();
        var epoch = service.CurrentStamp.EpochId;
        var selected = core.GetSelectedTab();
        var transfer = core.OpenTransferredFileAsync(file, source.PrepareAsync, async editor =>
        {
            await source.PublishAsync(editor);
            committed.TrySetResult(null);
            await release.Task;
        }, -1, default);
        try
        {
            await committed.Task.WaitAsync(TimeSpan.FromSeconds(15));
            Check(await TransferRecoveryStore.HasReceiptAsync(source.Token) && core.FindReadyTab(file) == null,
                "Close fixture failed to pause after a real receipt committed and before installation.");
            core.IsClosing = true;
            page.TestController.PauseRestoration();
            release.TrySetResult(null);
            var result = await transfer.WaitAsync(TimeSpan.FromSeconds(15));
            Check(result.Kind == WorkspaceFileOpenKind.Created && core.ContainsTab(result.Tab) &&
                result.Tab.ReadyEditor.GetText() == source.Text && source.Editor.GetText() == source.Text &&
                service.CurrentStamp.EpochId == epoch,
                "Close discarded a committed target, retired its epoch or changed the still-owned source.");
            Check(page.TestContentHost(result.Tab).Content is not ITextEditor && !result.Tab.HasBeenPresented &&
                ReferenceEquals(core.GetSelectedTab(), selected),
                "A committed transfer was visually attached or stole selection while close paused presentation.");
            Check(await TransferRecoveryStore.HasReceiptAsync(source.Token) &&
                !await TransferRootReader.IsSourceAcknowledgedAsync(source.Token),
                "Close lost the committed receipt or acknowledged source removal before its transfer adapter did so.");
            core.IsClosing = false;
            page.TestController.ResumeRestoration();
            Check(ReferenceEquals(page.TestContentHost(result.Tab).Content, result.Tab.ReadyEditor),
                "Canceling close did not present the transferred editor installed while closing.");
            VerifyUnique(core, file);
        }
        finally
        {
            release.TrySetResult(null);
            core.IsClosing = false;
            page.TestController.ResumeRestoration();
        }
    }

    private static async Task VerifyReceiptFailureAsync(NotepadsCore core, SessionService service)
    {
        var file = await NewFileAsync("receipt-failure");
        await using var source = await TransferFixture.CreateAsync(core, service, file);
        var before = core.TestOwnedEditorCount;
        var result = await core.OpenTransferredFileAsync(file, source.PrepareAsync,
            _ => throw new IOException("Injected uncommitted receipt failure."), -1, default)
            .WaitAsync(TimeSpan.FromSeconds(15));
        Check(result.Kind == WorkspaceFileOpenKind.Failed && result.Error is IOException &&
            core.TestOwnedEditorCount == before && core.FindReadyTab(file) == null &&
            !await TransferRecoveryStore.HasReceiptAsync(source.Token) && source.Editor.GetText() == source.Text,
            "Uncommitted transfer failure leaked its candidate or authorized source removal.");
        Check(await core.OpenFileAsync(file, _ => Task.CompletedTask).WaitAsync(TimeSpan.FromSeconds(15)),
            "Failed transfer did not release file admission for a subsequent ordinary open.");
    }

    private static async Task VerifyPreparedTransferResetAsync(NotepadsMainPage page, NotepadsCore core, SessionService service)
    {
        var file = await NewFileAsync("prepared-reset");
        await using var source = await TransferFixture.CreateAsync(core, service, file);
        var prepared = Signal();
        var release = Signal();
        var before = core.TestOwnedEditorCount;
        var transfer = core.OpenTransferredFileAsync(file, async () =>
        {
            var candidate = await source.PrepareAsync();
            prepared.TrySetResult(null);
            await release.Task;
            return candidate;
        }, source.PublishAsync, -1, default);
        try
        {
            await prepared.Task.WaitAsync(TimeSpan.FromSeconds(15));
            Check(core.TestOwnedEditorCount == before + 1 && core.FindReadyTab(file) == null,
                "Reset fixture failed to prepare an unpublished native transfer candidate.");
            await page.TestController.ClearSessionDataAsync().WaitAsync(TimeSpan.FromSeconds(15));
            var fresh = page.OpenFileAsync(file);
            Check(!fresh.IsCompleted && !transfer.IsCompleted,
                "Reset abandoned native preparation or cleared a still-held admission lease.");
            release.TrySetResult(null);
            Check((await transfer.WaitAsync(TimeSpan.FromSeconds(15))).Kind == WorkspaceFileOpenKind.Cancelled &&
                await fresh.WaitAsync(TimeSpan.FromSeconds(15)) && source.ReceiptCount == 0 &&
                core.TestOwnedEditorCount == before + 1,
                "Reset published the obsolete candidate or failed to dispose it before new admission.");
            VerifyUnique(core, file);
        }
        finally { release.TrySetResult(null); }
    }

    private static async Task VerifyPresentationIndependentShutdownAsync(NotepadsMainPage page, NotepadsCore core,
        SessionService service)
    {
        var file = await NewFileAsync("shutdown-presentation");
        var transferredFile = await NewFileAsync("shutdown-receipt");
        await using var source = await TransferFixture.CreateAsync(core, service, transferredFile);
        var presenting = Signal();
        var releasePresentation = Signal();
        var publicationEntered = Signal();
        var releasePublication = Signal();
        var receiptCommitted = Signal();
        var releaseInstallation = Signal();
        var epoch = service.CurrentStamp.EpochId;
        var disposing = false;
        var installedDuringDisposal = false;
        WorkspaceTab installedTab = null;
        void Opened(object sender, WorkspaceTab tab)
        {
            if (tab.ReadyEditor?.EditingFile?.IsEqual(transferredFile) == true)
            {
                installedTab = tab;
                installedDuringDisposal = disposing && service.CurrentStamp.EpochId == epoch &&
                    tab.ReadyEditor.GetText() == source.Text && source.Editor.GetText() == source.Text;
            }
        }
        core.TabOpened += Opened;
        var opening = core.OpenFileAsync(file, async result =>
        {
            Check(result.Kind == WorkspaceFileOpenKind.Created, "Shutdown fixture did not install its editor.");
            presenting.TrySetResult(null);
            await releasePresentation.Task;
            throw new IOException("Injected presentation failure after native shutdown.");
        });
        try
        {
            await presenting.Task.WaitAsync(TimeSpan.FromSeconds(15));
            var transfer = core.OpenTransferredFileAsync(transferredFile, source.PrepareAsync, async editor =>
            {
                publicationEntered.TrySetResult(null);
                await releasePublication.Task;
                await source.PublishAsync(editor);
                receiptCommitted.TrySetResult(null);
                await releaseInstallation.Task;
            }, -1, default);
            await publicationEntered.Task.WaitAsync(TimeSpan.FromSeconds(15));
            var owned = core.TestOwnedEditorCount;
            disposing = true;
            var disposal = page.TestDisposeWindowAsync();
            Check(!disposal.IsCompleted && !transfer.IsCompleted && core.TestOwnedEditorCount == owned,
                "MainPage disposed native editors before the entered transfer publication completed.");
            releasePublication.TrySetResult(null);
            await receiptCommitted.Task.WaitAsync(TimeSpan.FromSeconds(15));
            Check(!disposal.IsCompleted && !transfer.IsCompleted && !installedDuringDisposal &&
                await TransferRecoveryStore.HasReceiptAsync(source.Token),
                "Disposal failed to wait between committed receipt publication and target installation.");
            releaseInstallation.TrySetResult(null);
            var result = await transfer.WaitAsync(TimeSpan.FromSeconds(15));
            await disposal.WaitAsync(TimeSpan.FromSeconds(15));
            Check(result.Kind == WorkspaceFileOpenKind.Created && installedDuringDisposal &&
                result.Tab != null && ReferenceEquals(result.Tab, installedTab),
                "A transfer whose publication began before disposal was discarded instead of installed before draining.");
            Check(!opening.IsCompleted && core.TestOwnedEditorCount == 0,
                "Workspace shutdown waited for UI result presentation or retained native editors.");
            // The fixture's source editor belongs to this Core, so both native
            // editors are now disposed. Verify their actual durable roots and
            // assets after shutdown instead of reading their released controls.
            using (await SessionRecoveryTransaction.EnterAsync(CancellationToken.None))
            {
                var sourceDescriptor = await TransferRootReader.ReadSourceTransferAsync(source.Token);
                var receipt = await TransferRootReader.ReadReceiptAsync(source.Token);
                await TransferRootReader.ValidateAssetLengthsAsync(sourceDescriptor, CancellationToken.None);
                await TransferRootReader.ValidateAssetLengthsAsync(receipt.Record.Editor, CancellationToken.None);
                Check(receipt.Record.TargetStamp.EpochId == epoch &&
                    !await TransferRootReader.IsSourceAcknowledgedAsync(source.Token),
                    "Disposal retired receipt ownership or removed the source's durable recovery without acknowledgement.");
            }
            Check(await TransferRecoveryStore.HasReceiptAsync(source.Token),
                "Native shutdown invalidated the transfer's committed durable receipt.");
        }
        finally
        {
            releasePublication.TrySetResult(null);
            releaseInstallation.TrySetResult(null);
            releasePresentation.TrySetResult(null);
            core.TabOpened -= Opened;
        }
        Check(await opening.WaitAsync(TimeSpan.FromSeconds(10)),
            "A presentation failure changed the installed document's successful open result.");
    }

    private const string DiskText = "original disk 世界 e\u0301😀\r";

    private static async Task<StorageFile> NewFileAsync(string name)
    {
        var file = await ApplicationData.Current.LocalFolder.CreateFileAsync("opening-" + name + ".txt",
            CreationCollisionOption.FailIfExists);
        await FileIO.WriteTextAsync(file, DiskText);
        return file;
    }

    private static void VerifyUnique(NotepadsCore core, StorageFile file) =>
        Check(core.GetTabs().Count(tab => tab.ReadyEditor?.EditingFile?.IsEqual(file) == true) == 1,
            "Opening admitted more than one editor for " + file.Name);

    private static TaskCompletionSource<object> Signal() => new(TaskCreationOptions.RunContinuationsAsynchronously);

    private sealed class TransferFixture : IAsyncDisposable
    {
        private readonly NotepadsCore _core;
        private readonly RecoveryStamp _stamp;
        private TextEditorSessionDataV2 _descriptor;
        private TransferFixture(NotepadsCore core, RecoveryStamp stamp) { _core = core; _stamp = stamp; }
        internal ITextEditor Editor { get; private set; }
        internal TransferToken Token { get; private set; }
        internal string Text { get; private set; }
        internal int PreparationCount { get; private set; }
        internal int ReceiptCount { get; private set; }

        internal static async Task<TransferFixture> CreateAsync(NotepadsCore core, SessionService service, StorageFile file)
        {
            var fixture = new TransferFixture(core, service.CurrentStamp);
            try
            {
                fixture.Editor = await core.CreateTextEditorAsync(Guid.NewGuid(), file);
                fixture.Editor.TypeText("transferred unsaved Україна العربية𐐷\r");
                fixture.Text = fixture.Editor.GetText();
                using var capture = fixture.Editor.CaptureRecoveryState();
                await capture.Checkpoint.FlushAsync();
                fixture._descriptor = SessionDocumentStore.CaptureEditor(fixture.Editor.Id, capture, file.Name, file.Path);
                // Match TextEditorTransfer's ownership handoff before publishing
                // the offer, so native disposal cannot remove its source assets.
                capture.PreserveForRecovery();
                fixture.Token = await TransferRecoveryStore.CreateSourceTransferAsync(Guid.NewGuid(), core.DocumentOwnerId,
                    Guid.NewGuid(), fixture._descriptor, file, fixture._stamp);
                return fixture;
            }
            catch { await fixture.DisposeAsync(); throw; }
        }

        internal Task<ITextEditor> PrepareAsync()
        {
            PreparationCount++;
            return _core.TestPrepareTransferredEditorAsync(_descriptor, Editor.EditingFile);
        }

        internal Task PublishAsync(ITextEditor editor)
        {
            ReceiptCount++;
            return _core.TestProtectTransferredEditorAsync(editor, Token, _stamp);
        }

        public async ValueTask DisposeAsync()
        {
            if (Editor != null) await _core.DiscardUnattachedEditorAsync(Editor);
        }
    }
}
