// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Documents.IO;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Workspace;
using NotepadsIntegrationTests;
using Windows.Storage;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static partial class SaveCloseRaceTests
{
    private static async Task CurrentPickedConflictAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(host, "picked-external-change.txt", "base"u8.ToArray());
        editor.TypeText("confirmed");
        var file = editor.EditingFile;
        await editor.SaveContentToFileAndUpdateEditorStateAsync(file);
        var picked = await editor.ReadPickedFileVersionAsync(file, default);
        editor.TypeText("new draft");
        await FileIO.WriteBytesAsync(file, "external"u8.ToArray());
        try
        {
            await editor.SaveContentToFileAndUpdateEditorStateAsync(file, picked);
            throw new InvalidOperationException("A genuine external revision must reject the picked save.");
        }
        catch (DocumentSaveException error) when (error.Stage == DocumentSaveStage.Conflict) { }
        Check(editor.IsModified && editor.FileModificationState == FileModificationState.Modified,
            "Preserving a confirmed revision on stale picker rejection must not hide genuine external changes.");
        await BytesAsync(file, "external"u8.ToArray(), "A real external change remains untouched");
        log.AppendLine("PASS: an actual external change still marks the bound file modified and preserves its bytes.");
    }

    private static async Task StaleConfirmedPickerAsync(StringBuilder log, bool reload)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(host, $"stale-confirmed-{reload}.txt", "base"u8.ToArray());
        var file = editor.EditingFile;
        var picked = await editor.ReadPickedFileVersionAsync(file, default);
        var entered = Signal();
        var release = Signal();
        Task first = null;
        Task queued = null;
        var held = editor.TestHoldDocumentOperationAsync(() => { entered.SetResult(); return release.Task; });
        try
        {
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(10));
            byte[] expected;
            if (reload)
            {
                expected = "external-reload"u8.ToArray();
                await FileIO.WriteBytesAsync(file, expected);
                first = editor.ReloadFromEditingFileAsync();
            }
            else
            {
                editor.TypeText("confirmed-draft");
                expected = editor.GetEncoding().GetBytes(editor.GetText());
                first = editor.SaveContentToFileAndUpdateEditorStateAsync(file);
            }
            queued = editor.SaveContentToFileAndUpdateEditorStateAsync(file, picked);
            release.SetResult();
            await held;
            await first;
            try
            {
                await queued;
                throw new InvalidOperationException("A picked version predating the confirmed revision must still conflict.");
            }
            catch (DocumentSaveException error) when (error.Stage == DocumentSaveStage.Conflict) { }
            Check(editor.FileModificationState == FileModificationState.Untouched && !editor.IsModified,
                "Rejecting stale picker intent must preserve the newer confirmed save/reload classification.");
            await editor.TestCheckFileStatusAsync();
            Check(editor.FileModificationState == FileModificationState.Untouched,
                "A stale picker cannot leave the confirmed file permanently marked as externally modified.");
            await BytesAsync(file, expected, "The confirmed revision survives a stale picker");
        }
        finally
        {
            release.TrySetResult();
            await held;
            if (first != null) await first;
            if (queued != null) try { await queued; } catch (DocumentSaveException) { }
        }
        log.AppendLine($"PASS: stale picked save preserves a confirmed revision and clean file status; reload={reload}.");
    }

    private static async Task ActiveSaveTransferAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(host, "drag-during-save.txt", "base"u8.ToArray());
        var core = (NotepadsCore)host.Core;
        editor.TypeText("draft");
        var entered = Signal();
        var release = Signal();
        editor.TestResolvingSaveTarget = () => { entered.TrySetResult(); return release.Task; };
        var tab = host.Core.GetTab(editor);
        var operation = Guid.NewGuid();
        var save = editor.SaveContentToFileAndUpdateEditorStateAsync(editor.EditingFile);
        try
        {
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(10));
            using var recovery = editor.CaptureRecoveryState();
            Check(recovery.MetaData.HasUnconfirmedFileSave,
                "Crash checkpoints must remain conservative while a bound save is active.");
            var drag = core.TestBeginTabTransfer(operation, tab);
            Check(drag.Cancel && core.TestTabTransferOperation(tab) == null,
                "An active save must reject drag capture instead of publishing a fabricated unconfirmed transfer.");
        }
        finally
        {
            core.TestEndTabTransfer(operation, tab);
            release.TrySetResult();
            editor.TestResolvingSaveTarget = null;
            await save;
        }
        operation = Guid.NewGuid();
        try
        {
            Check(!core.TestBeginTabTransfer(operation, tab).Cancel,
                "The settled editor remains draggable after the save completes.");
        }
        finally { core.TestEndTabTransfer(operation, tab); }
        log.AppendLine("PASS: active saves keep crash recovery conservative and reject drag capture; settled saves remain draggable.");
    }
}
