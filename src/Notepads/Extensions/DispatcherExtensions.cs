// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Extensions
{
    using System;
    using System.Threading.Tasks;
    using Windows.UI.Core;

    public static class DispatcherExtensions
    {
        public static async Task CallOnUIThreadAsync(this CoreDispatcher dispatcher, DispatchedHandler handler)
        {
            await dispatcher.RunAsync(CoreDispatcherPriority.Normal, handler);
        }
    }
}