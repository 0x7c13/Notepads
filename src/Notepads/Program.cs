// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Threading;
using System.Threading.Tasks;
using Notepads.Composition;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Storage;

namespace Notepads;

public static class Program
{
    static void Main(string[] args)
    {
#if DEBUG
        Task.Run(() => LoggingService.InitializeAsync(new FileLogSink()));
#endif

        new InstanceRouter(StartApplication).Activate();
    }

    private static void StartApplication(BootstrapContext bootstrap)
    {
        Windows.UI.Xaml.Application.Start(_ =>
        {
            SynchronizationContext.SetSynchronizationContext(new Windows.System.DispatcherQueueSynchronizationContext(
                Windows.System.DispatcherQueue.GetForCurrentThread()));
            new App(bootstrap);
        });
    }
}
