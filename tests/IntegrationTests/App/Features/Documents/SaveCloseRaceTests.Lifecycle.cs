// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Preferences;
using Notepads.Presentation.Controls.Dialog;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsIntegrationTests;
using Windows.Storage;
using Windows.UI.Xaml.Controls;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static partial class SaveCloseRaceTests
{
    private static async Task ActivePlaceholderSaveAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var original = host.Core.GetTabs().Single();
        var editor = (TextEditor)original.ReadyEditor;
        Check(editor.IsDocumentEmpty && !editor.IsModified && editor.EditingFile == null,
            "The replacement fixture must start with one clean empty untitled tab.");
        var destination = await host.Folder.CreateFileAsync("saving-empty-tab.txt");
        var incoming = await host.Folder.CreateFileAsync("incoming-during-save.txt");
        await FileIO.WriteBytesAsync(incoming, "incoming document"u8.ToArray());
        var entered = Signal();
        var release = Signal();
        editor.TestResolvingSaveTarget = () => { entered.TrySetResult(); return release.Task; };
        Task<bool> saving = null;
        Task<bool> opening = null;
        try
        {
            TestFileSavePicker.Reset();
            TestFileSavePicker.Results.Enqueue(destination);
            saving = host.Page.TestSaveDocumentAsync(editor, saveAs: true);
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(15));
            Check(editor.IsFileSaveActive && editor.EditingFile == null && !editor.IsModified,
                "The empty untitled tab must be actively saving before its new binding is published.");
            opening = host.Page.OpenFileAsync(incoming);
            Check(await opening.WaitAsync(TimeSpan.FromSeconds(15)),
                "An external file open must finish while the empty tab's Save As remains admitted.");
            Check(!saving.IsCompleted && host.Core.ContainsTab(original) && host.Core.GetTabCount() == 2 && host.Page.IsEnabled,
                "Opening another file must not start an automatic close of the actively saving empty tab.");
            release.TrySetResult();
            Check(await saving.WaitAsync(TimeSpan.FromSeconds(15)), "The admitted empty Save As must finish normally.");
            await editor.WaitForDocumentOperationsAsync();
            Check(host.Core.ContainsTab(original) && host.Core.GetTabCount() == 2 &&
                editor.EditingFile?.IsEqual(destination) == true && !editor.IsModified &&
                host.Core.GetTabs().Any(tab => tab.ReadyEditor?.EditingFile?.IsEqual(incoming) == true),
                "Both the newly saved original tab and incoming file must remain open after Save As settles.");
            await BytesAsync(destination, [], "The original tab saves its empty revision");
            await BytesAsync(incoming, "incoming document"u8.ToArray(), "The incoming file remains unchanged");
        }
        finally
        {
            release.TrySetResult();
            try
            {
                await Task.WhenAll(new[] { saving, opening }.Where(work => work != null))
                    .WaitAsync(TimeSpan.FromSeconds(20));
            }
            finally
            {
                editor.TestResolvingSaveTarget = null;
                TestFileSavePicker.Reset();
            }
        }
        log.AppendLine("PASS: opening another file during an empty tab's active Save As retains both the newly saved original tab and incoming document.");
    }

    private static async Task RevertConfirmationDuringCloseAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        Check(!ApplicationPreferences.IsSessionSnapshotEnabled,
            "The confirmation race requires the actual Save/Discard/Cancel window-close reminder.");
        var editor = await OpenAsync(host, "revert-confirmation-close.txt", "base"u8.ToArray());
        var file = editor.EditingFile;
        var tab = host.Core.GetTab(editor);
        editor.TypeText("draft");
        var draft = editor.GetText();
        var baseline = editor.LastSavedSnapshot.Baseline.File;
        Check(baseline != null, "The nonempty saved revision must have an actual baseline file to block.");
        using var blocker = new FileStream(baseline.Path, FileMode.Open, FileAccess.Read, FileShare.None);
        var clicked = Signal();
        RevertAllChangesConfirmationDialog confirmation = null;
        Task reverting = null;
        Task<bool> closing = null;
        void CloseAfterConfirmation(ContentDialog sender, ContentDialogButtonClickEventArgs args)
        {
            // The constructor registered the confirmed action first. Start the
            // real close before ShowAsync can return and admit the revert body.
            closing = host.Page.TestCloseWindowAsync();
            clicked.TrySetResult();
        }
        try
        {
            reverting = host.Page.TestRevertAllChangesAsync();
            await ChooseDialogAsync(clicked.Task, "PrimaryButton", file.Name, whileDialogOpen: () =>
            {
                confirmation = ActiveVisibleDialog() as RevertAllChangesConfirmationDialog;
                Check(confirmation != null, "The real Revert confirmation must own the primary button.");
                confirmation.PrimaryButtonClick += CloseAfterConfirmation;
                return Task.CompletedTask;
            });
            Check(closing != null, "The confirmed Revert button must have requested actual window close.");
            await UntilAsync(() => closing.IsCompleted ||
                ActiveVisibleDialog() is AppCloseSaveReminderDialog or FileOpenErrorDialog,
                "The close request must reach its real reminder without a late revert error.");
            var reminder = ActiveVisibleDialog() as AppCloseSaveReminderDialog;
            Check(!closing.IsCompleted && reminder != null && !host.Page.IsEnabled,
                "Close must retain its real Save/Discard/Cancel reminder after Revert confirmation.");
            await reverting.WaitAsync(TimeSpan.FromSeconds(10));
            Check(ReferenceEquals(reminder, ActiveVisibleDialog()) && !reminder.IsAborted,
                "The confirmed-but-not-admitted revert must not replace or abort the active close reminder.");
            await ChooseDialogAsync(closing, "CloseButton");
            Check(!await closing && host.Page.IsEnabled && host.Core.ContainsTab(tab) &&
                editor.IsModified && editor.GetText() == draft,
                "Canceling close must preserve the draft when close supersedes a pending Revert confirmation.");
            await BytesAsync(file, "base"u8.ToArray(), "The superseded Revert and canceled close leave the user file intact");
        }
        finally
        {
            if (confirmation != null) confirmation.PrimaryButtonClick -= CloseAfterConfirmation;
            var draining = Task.WhenAll(new[] { reverting, closing }.Where(work => work != null));
            await UntilAsync(() =>
            {
                ActiveVisibleDialog()?.Hide();
                return draining.IsCompleted;
            }, "Confirmation-race cleanup must dismiss owned dialogs and drain both commands.");
            await draining;
        }
        log.AppendLine("PASS: actual Revert primary-button confirmation followed by window close preserves the close reminder and dirty draft without a late file error.");
    }
}
