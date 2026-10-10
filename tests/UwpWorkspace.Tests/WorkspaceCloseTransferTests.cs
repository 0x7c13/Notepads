// Copyright (c) 2026, Jiaqi (0x7c13) Liu. See LICENSE.
using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Composition;
using Notepads.Features.Preferences;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Transfer;
using Notepads.Features.Sessions.Validation;
using Notepads.Infrastructure.Settings;
using Notepads.Presentation.Controls.Dialog;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Views.MainPage;
using Notepads.Presentation.Workspace;
using Windows.Storage;
using Windows.Storage.AccessCache;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static partial class WorkspaceFileOpeningTests
{
    private static async Task VerifySnapshotOffTransferClosesAsync(StringBuilder log)
    {
        var snapshots = ApplicationPreferences.IsSessionSnapshotEnabled;
        try
        {
            // Change the isolated preference before creating each page; this is not a live reset.
            ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, false);
            ApplicationPreferences.RefreshSessionSnapshotSetting();
            foreach (var button in new[] { "CloseButton", "PrimaryButton", "SecondaryButton" })
                await VerifySnapshotOffTransferCloseAsync(button);
        }
        finally
        {
            ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, snapshots);
            ApplicationPreferences.RefreshSessionSnapshotSetting();
        }
        log.AppendLine("PASS: snapshot-off close waits for transfer installation before Cancel/Save/Discard.");
        log.AppendLine("PASS: Cancel preserves edits; Save/Discard persist Closed and reject later source ACK.");
    }

    private static async Task VerifySnapshotOffTransferCloseAsync(string button)
    {
        var owner = Guid.NewGuid();
        var scope = new SessionScopeData { OwnerId = owner, InstanceId = owner, Kind = SessionScopeData.Secondary };
        var service = new SessionService(scope, SessionScopeData.GetSecondaryPrefix(owner) + "NotepadsSessionData.json",
            () => new[] { owner });
        var registry = new SessionRegistry();
        var context = new WindowContext("Workspace Tests", owner, false, owner, Window.Current.Dispatcher,
            service, registry.Register, () => { });
        NotepadsMainPage page = null;
        TransferFixture source = null;
        Task<WorkspaceFileOpenResult> transfer = null;
        Task<bool> close = null;
        var receiptCommitted = Signal();
        var releaseInstallation = Signal();
        try
        {
            var frame = new Frame();
            Check(frame.Navigate(typeof(NotepadsMainPage), new WorkspaceNavigation(context, null)),
                "Could not create the snapshot-off close workspace.");
            page = (NotepadsMainPage)frame.Content;
            Window.Current.Content = frame;
            await UntilAsync(() => page.TestStartupComplete, "Snapshot-off close startup did not finish.");
            var core = (NotepadsCore)page.TestCore;
            Check(!page.TestController.IsBackupEnabled && core.GetTabCount() == 1 && !core.HasTabsRequiringSave(),
                "The close fixture must begin with only an unmodified untitled tab and snapshots off.");
            var file = await NewFileAsync("snapshot-off-" + button);
            source = await TransferFixture.CreateAsync(core, service, file);
            transfer = core.OpenTransferredFileAsync(file, source.PrepareAsync, async editor =>
            {
                await source.PublishAsync(editor);
                receiptCommitted.TrySetResult(null);
                await releaseInstallation.Task;
            }, -1, default);
            await receiptCommitted.Task.WaitAsync(TimeSpan.FromSeconds(15));
            Check(await TransferRecoveryStore.HasReceiptAsync(source.Token) && core.FindReadyTab(file) == null,
                "The close fixture did not hold a committed receipt before target installation.");

            close = page.TestCloseWindowAsync();
            Check(core.IsClosing && !page.IsEnabled && !close.IsCompleted,
                "Window close did not wait while an entered receipt held the session coordinator.");
            releaseInstallation.TrySetResult(null);
            var received = await transfer.WaitAsync(TimeSpan.FromSeconds(15));
            Check(received.Kind == WorkspaceFileOpenKind.Created,
                "Closing discarded a transfer whose receipt had already committed.");

            // Before the barrier fix, close completes without a dialog after installing this dirty tab.
            await ChooseDialogAsync(close, button, whileDialogOpen: () =>
            {
                Check(core.ContainsTab(received.Tab) && received.Tab.IsModified &&
                    received.Tab.ReadyEditor.GetText() == source.Text && !page.IsEnabled &&
                    page.TestContentHost(received.Tab).Content is not ITextEditor,
                    "The close dialog did not include the dirty transferred editor parked offscreen.");
                return Task.CompletedTask;
            });
            var approved = await close;
            var canceled = button == "CloseButton";
            Check(approved != canceled, "The snapshot-off close decision returned the wrong approval result.");
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var catalog = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                var targetClosed = SessionRecoveryAuthority.IsClosed(service.CurrentStamp, received.Tab.Id,
                    catalog.Decisions.Select(read => read.Record));
                Check(targetClosed != canceled, "The close decision did not persist the expected target Closed state.");
                Check(!await TransferRootReader.IsSourceAcknowledgedAsync(source.Token),
                    "The fixture unexpectedly acknowledged the source before testing target closure.");
            }

            using var sourceCapture = source.Editor.CaptureRecoveryState();
            if (canceled)
            {
                Check(page.IsEnabled && !core.IsClosing && core.ContainsTab(received.Tab) && received.Tab.IsModified &&
                    ReferenceEquals(page.TestContentHost(received.Tab).Content, received.Tab.ReadyEditor) &&
                    received.Tab.ReadyEditor.GetText() == source.Text,
                    "Cancel did not resume the transferred editor with its unsaved text intact.");
                await TransferRecoveryStore.MarkSourceAcknowledgedAsync(source.Token, sourceCapture.CaptureRevision);
                using (await SessionRecoveryTransaction.EnterAsync())
                    Check(await TransferRootReader.IsSourceAcknowledgedAsync(source.Token),
                        "Cancel prevented the still-open target from accepting source acknowledgement.");
            }
            else
            {
                Check(!core.ContainsTab(received.Tab) && !await TransferRecoveryStore.HasReceiptAsync(source.Token),
                    "An explicitly closed target still authorized source removal.");
                var rejected = false;
                try { await TransferRecoveryStore.MarkSourceAcknowledgedAsync(source.Token, sourceCapture.CaptureRevision); }
                catch (InvalidDataException) { rejected = true; }
                Check(rejected && source.Editor.GetText() == source.Text,
                    "A late source acknowledgement succeeded after target closure or lost the source text.");
            }

            var written = (await FileIO.ReadTextAsync(file)).Replace("\r\n", "\r").Replace("\n", "\r");
            Check(written == (button == "PrimaryButton" ? source.Text : DiskText),
                "Save failed to write the transferred text, or Cancel/Discard unexpectedly changed the file.");
        }
        finally
        {
            releaseInstallation.TrySetResult(null);
            if (transfer != null) await transfer.WaitAsync(TimeSpan.FromSeconds(15));
            if (source != null) await source.DisposeAsync();
            if (page != null) await page.TestDisposeWindowAsync();
            else await service.DisposeAsync();
            if (close is { IsCompleted: false }) DialogManager.ActiveDialog?.Hide();
            if (close != null) await close.WaitAsync(TimeSpan.FromSeconds(15));
            Window.Current.Content = null;
            var prefix = SessionDocumentStore.GetFutureAccessTokenPrefix(owner);
            foreach (var token in StorageApplicationPermissions.FutureAccessList.Entries.Select(entry => entry.Token)
                .Where(token => token.StartsWith(prefix, StringComparison.Ordinal)).ToArray())
                StorageApplicationPermissions.FutureAccessList.Remove(token);
        }
    }
}
