// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Transfer;
using Notepads.Presentation.Controls.TextEditor;

namespace Notepads.Presentation.Workspace;

public sealed partial class NotepadsCore
{
    private async Task ProtectTransferredEditorAsync(ITextEditor editor, TransferToken token, RecoveryStamp expectedStamp)
    {
        var rootId = Guid.NewGuid();
        using (var recovery = editor.CaptureRecoveryState())
        {
            await recovery.Checkpoint.FlushAsync();
            var descriptor = SessionDocumentStore.CaptureEditor(editor.Id, recovery,
                editor.EditingFile?.Name, editor.EditingFilePath);
            await TransferRecoveryStore.CreateReceiptAsync(token, rootId, _context.InstanceId,
                descriptor, recovery, editor.EditingFile, expectedStamp);
        }
    }
}
