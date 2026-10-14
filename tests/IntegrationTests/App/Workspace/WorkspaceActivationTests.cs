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
using Notepads.Features.Preferences;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Storage;
using Notepads.Infrastructure.Settings;
using Notepads.Presentation.Controls.Dialog;
using Notepads.Presentation.Views.MainPage;
using Notepads.Presentation.Workspace;
using NotepadsEditorTests;
using Windows.Storage;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static class WorkspaceActivationTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        await VerifyOrderingAsync(commandFirst: false);
        await VerifyOrderingAsync(commandFirst: true);
        await VerifyOrderingAsync(commandFirst: false, cancelStartupClose: true);
        await VerifyCloseBeforeLoadedAsync();
        log.AppendLine("PASS: external file and command activations wait for recovery and retain arrival order and final selection.");
        log.AppendLine("PASS: failed activation allows the next file to open; close before Loaded settles queued requests without opening files.");
        log.AppendLine("PASS: canceled startup close resumes activation; actual disposal does not wait for a file-error dialog.");
    }

    private static TaskCompletionSource<bool> Signal() => new(TaskCreationOptions.RunContinuationsAsynchronously);

    private static SessionService CreateService(Guid owner) => new(
        new SessionScopeData { OwnerId = owner, InstanceId = owner, Kind = SessionScopeData.Secondary },
        SessionScopeData.GetSecondaryPrefix(owner) + "NotepadsSessionData.json", () => new[] { owner });

    private static (Frame Frame, NotepadsMainPage Page) CreatePage(Guid owner, SessionService service)
    {
        var registry = new SessionRegistry();
        var context = new WindowContext("Notepads", owner, false, owner, Window.Current.Dispatcher,
            service, registry.Register, () => { });
        var frame = new Frame();
        Check(frame.Navigate(typeof(NotepadsMainPage), new WorkspaceNavigation(context, null)), "Activation page navigation failed.");
        return (frame, (NotepadsMainPage)frame.Content);
    }

    private static async Task<StorageFile> FileAsync(string name)
    {
        var file = await ApplicationData.Current.LocalFolder.CreateFileAsync("activation-" + Guid.NewGuid().ToString("N") + "-" + name);
        await FileIO.WriteTextAsync(file, name + " Ω😀");
        return file;
    }

    private static async Task VerifyOrderingAsync(bool commandFirst, bool cancelStartupClose = false)
    {
        var owner = Guid.NewGuid();
        var recoveredId = Guid.NewGuid();
        var firstFile = await FileAsync("first.txt");
        var secondFile = await FileAsync("second.txt");
        var failedFile = await FileAsync("failed.txt");
        var recoveryStarted = Signal();
        var releaseRecovery = Signal();
        var firstStarted = Signal();
        var releaseFirst = Signal();
        NotepadsMainPage page = null;
        try
        {
            await using (var seed = CreateService(owner))
            {
                seed.IsBackupEnabled = true;
                await seed.InitializeAuthorityAsync();
                await using var document = await SessionTestDocument.CreateAsync(owner, "recovered dirty text");
                using var capture = SessionTestProtocol.CaptureDocuments(new[] { document }, new[] { recoveredId }, seed.CurrentStamp);
                capture.SelectedEditorId = recoveredId;
                Check((await seed.SaveAsync(capture, CancellationToken.None)).Succeeded, "Activation recovery fixture could not be saved.");
            }
            await using var service = CreateService(owner);
            var workspace = CreatePage(owner, service);
            page = workspace.Page;
            var core = (NotepadsCore)page.TestCore;
            Task<bool> closeDuringStartup = null;
            if (cancelStartupClose)
            {
                core.EditorAttached += (_, tab) =>
                {
                    if (tab.Id != recoveredId || closeDuringStartup != null) return;
                    // Emulate another window disabling snapshots without resetting this roster.
                    ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, false);
                    closeDuringStartup = page.TestCloseWindowAsync();
                };
            }
            var preparationCount = 0;
            page.TestController.BeforeRestorePreparation = async (_, cancellation) =>
            {
                recoveryStarted.TrySetResult(true);
                await releaseRecovery.Task.WaitAsync(cancellation);
            };
            core.BeforeFilePreparation = async () =>
            {
                if (++preparationCount != 1) return;
                firstStarted.TrySetResult(true);
                await releaseFirst.Task;
            };

            // Same page entry points used by cold navigation and subsequent OS activation.
            var first = commandFirst
                ? page.ActivateCommandAsync(ApplicationData.Current.LocalFolder.Path, "notepads.exe \"" + firstFile.Path + "\"")
                : page.ActivateFilesAsync(new IStorageItem[] { firstFile });
            var second = page.ActivateFilesAsync(new IStorageItem[] { secondFile });
            Check(!first.IsCompleted && !second.IsCompleted && core.GetTabCount() == 0,
                "External activation ran before the page was loaded.");
            Window.Current.Content = workspace.Frame;
            Window.Current.Activate();
            await recoveryStarted.Task.WaitAsync(TimeSpan.FromSeconds(10));
            Check(preparationCount == 0 && !first.IsCompleted && !second.IsCompleted,
                "External activation bypassed startup recovery.");
            releaseRecovery.SetResult(true);
            if (cancelStartupClose)
            {
                await UntilAsync(() => closeDuringStartup != null, "The restored editor did not initiate the close decision.");
                await ChooseDialogAsync(closeDuringStartup, "CloseButton", whileDialogOpen: async () =>
                {
                    await Task.Yield();
                    Check(!page.TestStartupComplete && preparationCount == 0 && !second.IsCompleted,
                        "Startup or activation bypassed the pending close decision.");
                });
                Check(!await closeDuringStartup && page.IsEnabled, "Cancel did not keep the startup window alive.");
                ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, true);
                ApplicationPreferences.RefreshSessionSnapshotSetting();
            }
            await firstStarted.Task.WaitAsync(TimeSpan.FromSeconds(15));
            Check(core.GetTabCount() == 1 && core.GetSelectedTab()?.Id == recoveredId &&
                preparationCount == 1 && !second.IsCompleted, "A later activation overtook the blocked first file.");
            releaseFirst.SetResult(true);
            await Task.WhenAll(first, second).WaitAsync(TimeSpan.FromSeconds(15));
            await UntilAsync(() => page.TestStartupComplete, "Queued activation startup did not complete.");
            Check(core.GetTabs().Select(tab => tab.ReadyEditor?.EditingFile?.Path ?? "recovery")
                    .SequenceEqual(new[] { "recovery", firstFile.Path, secondFile.Path }) &&
                core.GetSelectedReadyEditor()?.EditingFile?.IsEqual(secondFile) == true &&
                core.GetTabs()[0].ReadyEditor.GetText() == "recovered dirty text" &&
                core.GetTabs()[1].ReadyEditor.GetText() == "first.txt Ω😀" &&
                core.GetTabs()[2].ReadyEditor.GetText() == "second.txt Ω😀",
                "Activation reordered/duplicated documents, lost text, or selected an earlier request.");

            // A real file-open failure displays its dialog; the following request must wait,
            // then succeed after dismissal without a stale queue or duplicated document.
            core.BeforeFilePreparation = () => throw new IOException("Injected external activation failure.");
            var failed = page.ActivateFilesAsync(new IStorageItem[] { failedFile });
            var following = page.ActivateFilesAsync(new IStorageItem[] { firstFile });
            await ChooseDialogAsync(failed, "PrimaryButton", failedFile.Path, beforeInvoke: () =>
            {
                Check(!following.IsCompleted, "A later activation passed the failed request's dialog.");
                core.BeforeFilePreparation = null;
            });
            await following.WaitAsync(TimeSpan.FromSeconds(10));
            Check(core.GetTabCount() == 3 && core.GetSelectedReadyEditor()?.EditingFile?.IsEqual(firstFile) == true,
                "A failed activation poisoned later requests or retained a failed document.");
            if (!commandFirst && !cancelStartupClose)
            {
                core.BeforeFilePreparation = () => throw new IOException("Injected error during window disposal.");
                var dismissedDialog = DialogManager.ActiveDialog;
                var blocked = page.ActivateFilesAsync(new IStorageItem[] { failedFile });
                var queued = page.ActivateFilesAsync(new IStorageItem[] { secondFile });
                await UntilAsync(() => VisualTreeHelper.GetOpenPopups(Window.Current)
                    .SelectMany(popup => Descendants(popup.Child)).OfType<ContentDialog>()
                    .Any(dialog => !ReferenceEquals(dialog, dismissedDialog) &&
                        dialog.Content?.ToString()?.Contains(failedFile.Path, StringComparison.Ordinal) == true),
                    "The disposal fixture did not open its own error dialog.");
                Check(!blocked.IsCompleted && !queued.IsCompleted, "The error-dialog disposal fixture was not blocked.");
                await page.TestDisposeWindowAsync().WaitAsync(TimeSpan.FromSeconds(10));
                await Task.WhenAll(blocked, queued).WaitAsync(TimeSpan.FromSeconds(10));
                Check(core.TestOwnedEditorCount == 0, "Dialog-independent disposal retained native editors.");
            }
        }
        finally
        {
            releaseRecovery.TrySetResult(true);
            releaseFirst.TrySetResult(true);
            DialogManager.ActiveDialog?.Hide();
            if (page != null) await page.TestDisposeWindowAsync();
            ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, true);
            ApplicationPreferences.RefreshSessionSnapshotSetting();
            await SessionTestProtocol.CleanupAsync(owner);
            await firstFile.DeleteAsync();
            await secondFile.DeleteAsync();
            await failedFile.DeleteAsync();
        }
    }

    private static async Task VerifyCloseBeforeLoadedAsync()
    {
        var owner = Guid.NewGuid();
        var file = await FileAsync("closed.txt");
        try
        {
            await using var service = CreateService(owner);
            var (_, page) = CreatePage(owner, service);
            var core = (NotepadsCore)page.TestCore;
            var first = page.ActivateFilesAsync(new IStorageItem[] { file });
            var second = page.ActivateCommandAsync(ApplicationData.Current.LocalFolder.Path, "notepads.exe \"" + file.Path + "\"");
            try
            {
                await page.TestDisposeWindowAsync().WaitAsync(TimeSpan.FromSeconds(10));
                await Task.WhenAll(first, second).WaitAsync(TimeSpan.FromSeconds(10));
                await page.ActivateFilesAsync(new IStorageItem[] { file });
                Check(core.TestOwnedEditorCount == 0 && !page.TestStartupComplete,
                    "Closing an unloaded page admitted queued documents or started the workspace.");
            }
            finally { await page.TestDisposeWindowAsync(); }
        }
        finally
        {
            await SessionTestProtocol.CleanupAsync(owner);
            await file.DeleteAsync();
        }
    }
}
