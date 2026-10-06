// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading.Tasks;
using Windows.UI.Core;

namespace Notepads.Presentation.Helpers;

public static class DispatcherExtensions
{
    public static async Task CallOnUIThreadAsync(this CoreDispatcher dispatcher, DispatchedHandler handler)
    {
        await dispatcher.RunAsync(CoreDispatcherPriority.Normal, handler);
    }

    public static async Task CallOnUIThreadAsync(this CoreDispatcher dispatcher, Func<Task> handler)
    {
        var completion = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
        await dispatcher.RunAsync(CoreDispatcherPriority.Normal, async () =>
        {
            try { await handler(); completion.TrySetResult(null); }
            catch (Exception ex) { completion.TrySetException(ex); }
        });
        await completion.Task;
    }
}
