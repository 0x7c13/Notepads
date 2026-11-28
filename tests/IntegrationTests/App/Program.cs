// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading;
using NotepadsTestIsolation;
using Windows.UI.Xaml;

namespace Notepads;

public static class Program
{
    public static void Main(string[] args)
    {
        TestApplicationData.InitializeFromActivation();
        try
        {
            Application.Start(_ =>
            {
                SynchronizationContext.SetSynchronizationContext(new Windows.System.DispatcherQueueSynchronizationContext(
                    Windows.System.DispatcherQueue.GetForCurrentThread()));
                new App();
            });
        }
        catch (Exception error)
        {
            File.WriteAllText(Path.Combine(ApplicationData.Current.LocalFolder.Path, "results.txt"), "FAIL: startup " + error);
            throw;
        }
    }
}
