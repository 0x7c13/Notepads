// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Windows.UI.Core;

namespace Notepads.Infrastructure.Threading;

internal static class ThreadUtility
{
    public static bool IsOnUIThread()
    {
        CoreWindow coreWindow = CoreWindow.GetForCurrentThread();
        return coreWindow != null && coreWindow.Dispatcher.HasThreadAccess;
    }
}
