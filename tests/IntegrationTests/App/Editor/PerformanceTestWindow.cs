// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics;
using System.IO;
using System.Threading.Tasks;
using Windows.Storage;
using Windows.UI.Core;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;

namespace NotepadsEditorTests;

// Optional external-driver handshake, before any measured work or temporary
// editor exists. Normal runner invocation does not require desktop input.
internal static class PerformanceTestWindow
{
    internal static async Task WaitForForegroundAsync(string phase)
    {
        var directory = Path.Combine(ApplicationData.Current.LocalFolder.Path, "PerformanceForeground");
        if (!File.Exists(Path.Combine(directory, "requested.txt"))) return;
        var previous = Window.Current.Content;
        try
        {
            var ready = new Button { Content = "Preparing performance measurements", Margin = new Thickness(24) };
            Windows.UI.Xaml.Automation.AutomationProperties.SetAutomationId(ready, "PerformanceReady");
            Window.Current.Content = ready;
            Window.Current.Activate();
            await Window.Current.Dispatcher.RunAsync(CoreDispatcherPriority.Low, () => { });
            File.WriteAllText(Path.Combine(directory, "ready.txt"), phase);
            var watch = Stopwatch.StartNew();
            var start = Path.Combine(directory, "start.txt");
            while (!File.Exists(start) || File.ReadAllText(start) != phase)
            {
                if (watch.Elapsed > TimeSpan.FromSeconds(10))
                    throw new TimeoutException("The external performance driver did not acquire the owned window.");
                await Task.Delay(25);
            }
            if (Window.Current.CoreWindow.ActivationMode != CoreWindowActivationMode.ActivatedInForeground)
                throw new InvalidOperationException("The performance window lost foreground before measurement.");
        }
        finally { Window.Current.Content = previous; }
    }
}
