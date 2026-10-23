// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

// Source-linked access to the exact native preparation and receipt publication used by DropAsync.
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Notepads.Presentation.Controls.TextEditor;
using Windows.Storage;

namespace Notepads.Presentation.Workspace;

public sealed partial class NotepadsCore
{
    internal Task<ITextEditor> TestPrepareTransferredEditorAsync(TextEditorSessionDataV2 data, StorageFile file) =>
        PrepareTransferredEditorAsync(data, file);

    internal Task TestProtectTransferredEditorAsync(ITextEditor editor, TransferToken token, RecoveryStamp stamp) =>
        ProtectTransferredEditorAsync(editor, token, stamp);
}
