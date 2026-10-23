// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Recovery;
using Notepads.Presentation.Controls.Dialog;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Workspace;
using Windows.ApplicationModel.Resources;
using Windows.Storage;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class DocumentTitleFeatureTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var tab = host.Core.GetSelectedTab();
        var editor = (TextEditor)tab.ReadyEditor;
        var native = editor.TestNativeCore.TestFeatureNative;
        var resources = ResourceLoader.GetForCurrentView();
        var defaultName = resources.GetString("TextEditor_DefaultNewFileName");

        FileRenameDialog RenameDialog() => VisualTreeHelper.GetOpenPopups(Window.Current)
            .SelectMany(popup => Descendants(popup.Child)).OfType<FileRenameDialog>().SingleOrDefault();
        TextBox RenameInput(FileRenameDialog dialog) => Descendants(dialog).OfType<TextBox>().Single();
        TextBlock RenameError(FileRenameDialog dialog) => ((Panel)dialog.Content).Children.OfType<TextBlock>().Single();

        void Expect(string title)
        {
            var item = host.Page.TestContainer(tab);
            Check(editor.DisplayTitle == title && tab.Title == title && item.Header?.ToString() == title &&
                AutomationProperties.GetName(item) == title, "Native text updates the tab and its accessible title: " + title);
            var peer = FrameworkElementAutomationPeer.CreatePeerForElement(item);
            Check(peer?.GetName() == title, "The tab's native automation peer exposes the new title.");
            if (host.Core.GetSelectedTab() == tab)
                Check(ApplicationView.GetForCurrentView().Title == title, "The selected window title follows the tab: " + title);
            Check(editor.FileNamePlaceholder == defaultName, "Automatic titles never overwrite the filename placeholder.");
        }
        void Replace(string text)
        {
            native.SetSel(0, native.Length);
            editor.TypeText(text);
        }

        Expect(defaultName);
        Replace(" \t\r\n中文 😀 first\t title\rsecond");
        Expect("中文 😀 first title");
        Replace(string.Empty);
        Expect(defaultName);
        native.Undo();
        Expect("中文 😀 first title");
        native.Redo();
        Expect(defaultName);
        Replace(" \t\r\n");
        Expect(defaultName);
        Replace("report.cs\r" + new string('x', 100_000));
        Expect("report.cs");
        Check(editor.DocumentLanguage.Id == "plaintext", "A content title resembling an extension does not change syntax.");
        var titleChanges = 0;
        void Changed(object sender, EventArgs args) => titleChanges++;
        editor.DisplayTitleChanged += Changed;
        try
        {
            var cached = editor.DisplayTitle;
            native.SetSel(native.Length, native.Length);
            editor.TypeText("tail");
            Check(titleChanges == 0 && ReferenceEquals(cached, editor.DisplayTitle),
                "Editing beyond the prefix retains the cached title without UI notifications or a new title string.");
        }
        finally { editor.DisplayTitleChanged -= Changed; }

        host.Page.TestController.StopSessionBackup();
        host.Page.TestController.IsBackupEnabled = true;
        Check(await host.Page.TestController.SaveSessionAsync(), "A real checkpoint includes the automatic title.");
        var scope = await SessionRecoveryCatalog.ReadScopeAsync(host.Core.DocumentOwnerId);
        var decision = scope.Selection.Documents.Single(row => row.Id == editor.Id);
        var pending = new WorkspaceTab(new RecoveryDocumentSource(decision, null, true,
            null, null, new RecoverySourceLifetime()));
        Check(pending.ReadyEditor == null && pending.Title == "report.cs" &&
            pending.RecoverySource.Metadata.UseContentBasedTitle == true,
            "A durable index restores the title before any native editor or snapshot is opened.");
        pending.MarkClosed();

        var second = await NewTabAsync(host.Page);
        native.SetSel(0, native.Length);
        editor.TypeText("background draft");
        Check(tab.Title == "background draft" && host.Page.TestContainer(tab).Header?.ToString() == "background draft" &&
            ApplicationView.GetForCurrentView().Title == second.Title,
            "An inactive document changes its own header without overwriting the active window title.");
        host.Core.SwitchTo(tab);
        Expect("background draft");
        TestFileSavePicker.Results.Enqueue(null);
        Check(!await host.Page.TestSaveDocumentAsync(editor, saveAs: true) &&
            TestFileSavePicker.LastCreated.SuggestedFileName == defaultName,
            "Save As cancellation retains the automatic title and uses a safe filename suggestion.");
        Expect("background draft");

        var other = (TextEditor)second.ReadyEditor;
        other.TypeText("other draft");
        var beforeRevert = editor.GetText();
        await ChooseDialogAsync(host.Page.TestRevertAllChangesAsync(), "CloseButton", "background draft");
        Expect("background draft");
        Check(editor.GetText() == beforeRevert && editor.IsModified && other.GetText() == "other draft",
            "Canceling the real revert confirmation preserves both drafts and the automatic title.");
        await ChooseDialogAsync(host.Page.TestRevertAllChangesAsync(), "PrimaryButton", "background draft");
        Expect(defaultName);
        Check(editor.IsDocumentEmpty && !editor.IsModified && other.GetText() == "other draft" &&
            host.Core.GetSelectedTab() == tab,
            "Confirming revert clears only the captured draft and restores its default title and save state.");
        native.Undo();
        Expect("background draft");
        Check(editor.GetText() == beforeRevert && editor.IsModified,
            "Undoing the confirmed revert restores the draft's exact text and automatic title.");

        await ChooseDialogAsync(host.Page.TestRenameDocumentAsync(other), "PrimaryButton", whileDialogOpen: () =>
        {
            var dialog = RenameDialog();
            Check(RenameInput(dialog).Text == "other draft" && dialog.IsPrimaryButtonEnabled &&
                RenameError(dialog).Visibility == Visibility.Collapsed,
                "Rename initially shows the valid content title and allows confirming it unchanged.");
            return Task.CompletedTask;
        });
        other.TypeText(" and more");
        Check(other.DisplayTitle == "other draft" && other.FileNamePlaceholder == "other draft" &&
            !other.UsesContentBasedTitle && other.GetTextEditorStateMetaData().ContentTitle == null,
            "Confirming the displayed title pins exactly that name across later edits.");

        foreach (var invalidTitle in new[] { "notes: todo", "notes/next" })
        {
            Replace(invalidTitle);
            Expect(invalidTitle);
            await ChooseDialogAsync(host.Page.TestRenameDocumentAsync(editor), "CloseButton", whileDialogOpen: () =>
            {
                var dialog = RenameDialog();
                var error = RenameError(dialog);
                Check(RenameInput(dialog).Text == invalidTitle && !dialog.IsPrimaryButtonEnabled &&
                    error.Visibility == Visibility.Visible &&
                    error.Text == resources.GetString("InvalidFilenameError_ContainsInvalidCharacters"),
                    "An invalid content title is preserved and immediately shows the localized filename error.");
                return Task.CompletedTask;
            });
            Expect(invalidTitle);
            Check(editor.UsesContentBasedTitle && editor.GetText() == invalidTitle,
                "Canceling an invalid proposed name preserves automatic naming and document text.");
        }

        var renameToDefault = host.Page.TestRenameDocumentAsync(editor);
        await UntilAsync(() => RenameDialog()?.IsLoaded == true || renameToDefault.IsCompleted,
            "Rename must present an editable dialog for an invalid content title.");
        var renameDialog = RenameDialog();
        Check(renameDialog != null && RenameInput(renameDialog).Text == "notes/next" &&
            !renameDialog.IsPrimaryButtonEnabled, "An invalid prefilled name cannot be confirmed.");
        RenameInput(renameDialog).Text = defaultName;
        await UntilAsync(() => renameDialog.IsPrimaryButtonEnabled && RenameError(renameDialog).Visibility == Visibility.Collapsed,
            "Correcting the proposed filename clears its error and enables confirmation.");
        await ChooseDialogAsync(renameToDefault, "PrimaryButton");
        Expect(defaultName);
        Replace("manual title wins");
        Expect(defaultName);
        var metadata = editor.GetTextEditorStateMetaData();
        Check(metadata.UseContentBasedTitle == false && metadata.ContentTitle == null,
            "Explicitly renaming to the default name pins that name and persists false.");
        editor.ResetEditorState(metadata);
        Expect(defaultName);
        var renameAgain = host.Page.TestRenameDocumentAsync(editor);
        await ChooseDialogAsync(renameAgain, "CloseButton", whileDialogOpen: () =>
        {
            var dialog = RenameDialog();
            Check(RenameInput(dialog).Text == defaultName && !dialog.IsPrimaryButtonEnabled,
                "An already pinned name is prefilled and still requires a real rename.");
            return Task.CompletedTask;
        });

        metadata.UseContentBasedTitle = null;
        editor.ResetEditorState(metadata);
        Expect("manual title wins");
        await editor.RenameAsync("my draft.py", null);
        metadata = editor.GetTextEditorStateMetaData();
        metadata.UseContentBasedTitle = null;
        editor.ResetEditorState(metadata);
        Replace("content must not replace a legacy custom name");
        Check(tab.Title == "my draft.py", "A V1 custom filename remains pinned when its naming flag is absent.");
        await ChooseDialogAsync(host.Page.TestRevertAllChangesAsync(), "CloseButton", "my draft.py");
        Check(tab.Title == "my draft.py" && editor.GetText() == "content must not replace a legacy custom name",
            "The revert confirmation identifies a manually named draft and cancellation preserves it.");

        var destination = await host.Folder.CreateFileAsync("saved-draft.txt", CreationCollisionOption.FailIfExists);
        TestFileSavePicker.Results.Enqueue(destination);
        Check(await host.Page.TestSaveDocumentAsync(editor, saveAs: true), "Save As commits the real file.");
        Replace("different text after saving");
        Check(tab.Title == destination.Name && editor.DisplayTitle == destination.Name &&
            host.Page.TestContainer(tab).Header?.ToString() == destination.Name &&
            ApplicationView.GetForCurrentView().Title == destination.Name,
            "Saved filenames take precedence permanently, including later edits.");
        await ChooseDialogAsync(host.Page.TestRevertAllChangesAsync(), "CloseButton", destination.Name);
        Check(editor.GetText() == "different text after saving" && editor.DisplayTitle == destination.Name,
            "The revert confirmation still identifies saved files by their real filename.");
        await ChooseDialogAsync(host.Page.TestRenameDocumentAsync(editor), "CloseButton", whileDialogOpen: () =>
        {
            var dialog = RenameDialog();
            Check(RenameInput(dialog).Text == destination.Name && !dialog.IsPrimaryButtonEnabled &&
                RenameError(dialog).Visibility == Visibility.Collapsed,
                "An existing file still prefills its real filename and rejects an unchanged rename.");
            return Task.CompletedTask;
        });
        log.AppendLine("PASS: WorkspaceCommands.ContentTitles — native edits/undo, Unicode, safe names, inactive tabs and UIA.");
        log.AppendLine("PASS: WorkspaceCommands.ContentTitles — durable pending cache, V1 inference, pinned names and Save As.");
        log.AppendLine("PASS: WorkspaceCommands.ContentTitles — revert dialogs identify drafts/files; Cancel, Confirm and Undo preserve identity.");
        log.AppendLine("PASS: WorkspaceCommands.ContentTitles — rename prefills the display title, validates immediately and pins the submitted name.");
    }
}
