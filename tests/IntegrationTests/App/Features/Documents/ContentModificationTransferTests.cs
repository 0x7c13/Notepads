// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Text;
using System.Text.Json;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Transfer;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Workspace;
using Notepads.Presentation.Workspace.Activation;
using NotepadsIntegrationTests;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static class ContentModificationTransferTests
{
    private const string Original = "A\rΩ😀\0Z";

    internal static async Task RunAsync(StringBuilder log)
    {
        foreach (var settled in new[] { false, true }) await WindowMoveAsync(settled);
        await DragReceiptAsync();
        log.AppendLine("PASS: pending/settled content equality survives new-window moves with original undo history; drag receipts retain frozen classification and reject changed content or save settings.");
    }

    private static void ReplaceFirst(TextEditor editor, string value)
    {
        editor.TestNativeCore.SetTextSelectionPosition(0, 1);
        editor.TypeText(value);
    }

    private static async Task WindowMoveAsync(bool settled)
    {
        await using var source = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(source, "equal-window.txt", Encoding.UTF8.GetBytes(Original));
        var tab = source.Core.GetTab(editor);
        source.Core.OpenTextEditor(await source.Core.CreateNewTextEditorAsync("remaining source tab"));
        ReplaceFirst(editor, "B");
        ReplaceFirst(editor, "A");
        var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        editor.TestComparisonPause = _ => { entered.TrySetResult(); return release.Task; };
        if (settled) release.TrySetResult();
        var comparison = editor.TestCompareContentAsync();
        await entered.Task.WaitAsync(TimeSpan.FromSeconds(10));
        if (settled) await comparison;
        Check(editor.IsModified != settled && editor.TestNativeCore.IsDocumentModified,
            "The move starts with the intended pending/settled classification and a non-savepoint undo position.");
        FeatureTestHost receiver = null;
        TestLauncher.DataLaunch = async (_, _, data) =>
        {
            var token = JsonSerializer.Deserialize((string)data[WindowMoveActivation.DataKey], SessionJsonContext.Default.TransferToken);
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var frozen = await TransferRootReader.ReadSourceTransferAsync(token);
                Check(frozen.TextDirty != settled && frozen.StateMetaData.IsModified != settled,
                    "The source offer retains the classification at capture, before the pending comparison completes.");
            }
            release.TrySetResult();
            receiver = await FeatureTestHost.CreateAsync(token);
            return true;
        };
        try
        {
            Check(await source.Core.MoveTabToNewWindowAsync(tab).WaitAsync(TimeSpan.FromSeconds(40)),
                "A manually reverted file moves successfully regardless of source comparison timing.");
            Check(!source.Core.ContainsTab(tab), "A verified content-equivalent move retires the source.");
            var received = (TextEditor)receiver.Core.GetSelectedReadyEditor();
            await received.TestCompareContentAsync();
            Check(received.GetText() == Original && !received.IsModified && received.TestNativeCore.IsDocumentModified,
                "The receiver independently proves equality without relocating the undo save point.");
            received.TestNativeCore.Undo();
            Check(received.GetText()[0] == 'B' && received.IsModified, "Transferred undo restores the intermediate dirty edit.");
            received.TestNativeCore.Undo();
            Check(received.GetText() == Original && !received.IsModified && !received.TestNativeCore.IsDocumentModified,
                "Transferred undo still reaches the original native save point.");
        }
        finally
        {
            release.TrySetResult();
            await comparison;
            if (receiver != null) await receiver.DisposeAsync();
        }
    }

    private static async Task DragReceiptAsync()
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var core = (NotepadsCore)host.Core;
        var editor = await OpenAsync(host, "equal-drag.txt", Encoding.UTF8.GetBytes(Original));
        ReplaceFirst(editor, "B");
        ReplaceFirst(editor, "A");
        var tab = core.GetTab(editor);
        var operation = Guid.NewGuid();
        var admission = core.TestBeginTabTransfer(operation, tab); // Synchronous capture, before debounce.
        Check(!admission.Cancel && editor.IsModified, "Drag captures a still-pending equality decision.");
        try
        {
            var serialized = (string)await admission.Data.GetView().GetDataAsync(TextEditorTransfer.DataFormat);
            var token = JsonSerializer.Deserialize(serialized, SessionJsonContext.Default.TransferToken);
            TextEditorSessionDataV2 frozen;
            using (await SessionRecoveryTransaction.EnterAsync()) frozen = await TransferRootReader.ReadSourceTransferAsync(token);
            var stamp = new RecoveryStamp { ScopeId = token.SourceOwnerId, EpochId = token.SourceEpochId };
            Check(frozen.TextDirty && frozen.StateMetaData.IsModified, "The drag offer stores its conservative source classification.");
            var received = (TextEditor)await core.TestPrepareTransferredEditorAsync(frozen, editor.EditingFile);
            try
            {
                await received.TestCompareContentAsync();
                Check(!received.IsModified, "Target equality completes before receipt publication in this regression fixture.");
                await core.TestProtectTransferredEditorAsync(received, token, stamp, frozen);
                Check(!received.IsModified && await TransferRecoveryStore.HasReceiptAsync(token),
                    "Receipt publication preserves both the frozen offer and the receiver's independent live state.");
                using (await SessionRecoveryTransaction.EnterAsync())
                {
                    var receipt = await TransferRootReader.ReadReceiptAsync(token);
                    Check(receipt.Record.Editor.TextDirty && receipt.Record.Editor.StateMetaData.IsModified,
                        "The durable receipt acknowledges the conservative source flags.");
                }
            }
            finally { await core.DiscardUnattachedEditorAsync(received); }

            foreach (var changeText in new[] { true, false })
            {
                var invalid = (TextEditor)await core.TestPrepareTransferredEditorAsync(frozen, editor.EditingFile);
                try
                {
                    if (changeText) ReplaceFirst(invalid, "B");
                    else invalid.TryChangeEncoding(Encoding.Unicode);
                    bool rejected = false;
                    try { await core.TestProtectTransferredEditorAsync(invalid, token, stamp, frozen); }
                    catch (InvalidDataException error)
                    {
                        rejected = error.Message == "The transfer target is not an independent, equivalent recovery generation.";
                    }
                    Check(rejected, "Frozen classification cannot authorize different target content or encoding settings.");
                }
                finally { await core.DiscardUnattachedEditorAsync(invalid); }
            }
        }
        finally
        {
            core.TestEndTabTransfer(operation, tab);
            await UntilAsync(() => core.TestTabTransferOperation(tab) == null, "The fixture's drag reservation retires.");
        }
    }
}
