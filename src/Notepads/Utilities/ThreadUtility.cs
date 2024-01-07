// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Utilities
{
    using Windows.UI.Core;

    internal static class ThreadUtility
    {
        public static bool IsOnUIThread()
        {
            CoreWindow coreWindow = CoreWindow.GetForCurrentThread();
            return coreWindow != null && coreWindow.Dispatcher.HasThreadAccess;
        }
    }
}