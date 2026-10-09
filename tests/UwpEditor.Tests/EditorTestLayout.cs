// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading.Tasks;

namespace NotepadsEditorTests;

internal static class EditorTestLayout
{
    internal static async Task AfterPaintAsync(WinUIEditor.Editor native, Action action)
    {
        var painted = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        void OnPainted(WinUIEditor.Editor sender, WinUIEditor.PaintedEventArgs args) => painted.TrySetResult(true);
        native.Painted += OnPainted;
        try
        {
            action();
            // PointYFromPosition and WrapCount can still reflect the previous
            // rendered width until Scintilla has painted the requested layout.
            await Task.WhenAny(painted.Task, Task.Delay(5000));
            if (!painted.Task.IsCompleted)
                throw new InvalidOperationException("Native paint did not complete after the layout change.");
        }
        finally
        {
            native.Painted -= OnPainted;
        }
    }
}
