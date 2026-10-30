// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading.Tasks;
using Notepads.Features.Documents.Text;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Workspace;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditor
{
    internal async Task TransformSelectedLinesAsync(SelectedLineOperation operation)
    {
        try
        {
            await RunDocumentOperationAsync(async cancellation =>
            {
                if (!CanChangeIndentation) return;
                if (!await TextEditorCore.TransformSelectedLinesAsync(operation, cancellation))
                    NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("EditorTools_LineLimit"), 3000);
            });
        }
        catch (OperationCanceledException) { }
        catch (Exception error)
        {
            LoggingService.LogError($"[{nameof(TextEditor)}] Line operation failed: {error.Message}");
            if (!_disposed) NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("EditorTools_OperationFailed"), 3000);
        }
    }
}
