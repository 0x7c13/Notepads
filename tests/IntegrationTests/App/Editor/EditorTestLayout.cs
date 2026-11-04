// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading.Tasks;

namespace NotepadsEditorTests;

internal static class EditorTestLayout
{
    internal static async Task AfterPaintAsync(WinUIEditor.Editor native, Action action, TimeSpan? timeout = null)
    {
        var painted = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        void OnPainted(WinUIEditor.Editor sender, WinUIEditor.PaintedEventArgs args) => painted.TrySetResult(true);
        native.Painted += OnPainted;
        try
        {
            action();
            // PointYFromPosition and WrapCount can still reflect the previous
            // rendered width until Scintilla has painted the requested layout.
            try { await painted.Task.WaitAsync(timeout ?? TimeSpan.FromSeconds(5)); }
            catch (TimeoutException error)
            {
                throw new InvalidOperationException("Native paint did not complete after the layout change.", error);
            }
        }
        finally
        {
            native.Painted -= OnPainted;
        }
    }
}
