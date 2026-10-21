// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Notepads;
using Notepads.Features.Preferences;
using Notepads.Composition;
using Notepads.Infrastructure.Settings;
using NotepadsTestIsolation;
using Windows.ApplicationModel.Core;
using Windows.UI.Core;
using Windows.UI.Xaml;

namespace NotepadsIntegrationTests;

// Storage is isolated before the production instance router. Its application
// factory adds observations to the real App; no lifecycle handler is overridden.
internal static partial class LifecycleProgram
{
    internal static string Phase => TestApplicationData.Arguments;
    internal static long EntryTicks { get; private set; }
    internal static long IsolationReadyTicks { get; private set; }
    internal static string SettingsBefore { get; private set; }

    public static void Main(string[] args)
    {
        try { Run(args); }
        catch (Exception error)
        {
            File.WriteAllText(Path.Combine(ApplicationData.Current.LocalFolder.Path, $"lifecycle-start-error-{Environment.ProcessId}.txt"), error.ToString());
            throw;
        }
    }

    private static void Run(string[] args)
    {
        EntryTicks = Stopwatch.GetTimestamp();
        TestApplicationData.InitializeFromActivation();
        IsolationReadyTicks = Stopwatch.GetTimestamp();
        SettingsBefore = LifecycleProbe.SettingsFingerprint();
        if (Phase == "--test-cleanup")
        {
            // Cleanup has no workspace/XAML lifecycle. Keep a broker-dispatched
            // CoreApplication alive until the runner retains its exact handle.
            CoreApplication.Run(new CleanupView());
            return;
        }
        if (Phase == "--lifecycle-seed")
        {
            ApplicationSettingsStore.Write(SettingsKey.RequestedThemeStr, "Dark");
            ApplicationSettingsStore.Write(SettingsKey.UseWindowsThemeBool, false);
            ApplicationSettingsStore.Write(SettingsKey.EditorFontSizeInt, 14);
            ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, true);
            ApplicationSettingsStore.Write(SettingsKey.ExitWhenLastTabClosed, false);
            ApplicationSettingsStore.Write(SettingsKey.AlwaysOpenNewWindowBool, false);
        }
        if (Phase == "--lifecycle-exit-control") StartApplication(new BootstrapContext());
        else new InstanceRouter(StartApplication).Activate();
    }

    private static void StartApplication(BootstrapContext bootstrap)
    {
        Application.Start(initialization =>
        {
            SynchronizationContext.SetSynchronizationContext(new Windows.System.DispatcherQueueSynchronizationContext(
                Windows.System.DispatcherQueue.GetForCurrentThread()));
            new App(bootstrap);
            CoreApplication.Exiting += (_, _) =>
            {
                using var process = Process.GetCurrentProcess();
                File.WriteAllText(Path.Combine(ApplicationData.Current.LocalFolder.Path, $"framework-exiting-{process.Id}.txt"),
                    process.StartTime.ToUniversalTime().Ticks.ToString(System.Globalization.CultureInfo.InvariantCulture));
            };
            CoreApplication.GetCurrentView().Activated += (view, activation) =>
            {
                if (Phase is "--lifecycle-declined" or "--lifecycle-exit-control")
                {
                    if (activation is not Windows.ApplicationModel.Activation.LaunchActivatedEventArgs { PrelaunchActivated: true })
                        throw new InvalidOperationException("The declined-prelaunch fixture received a real activation.");
                    var control = Phase == "--lifecycle-exit-control";
                    LifecycleProbe.Declined(control ? "exit-control" : "declined");
                    // Hold only the observer until the runner retains the process.
                    // The control independently measures framework Exit's result;
                    // the real case must use the product's queued Exit request.
                    WaitForRetentionAsync().GetAwaiter().GetResult();
                    if (control) _ = Window.Current.Dispatcher.RunAsync(CoreDispatcherPriority.Normal, Application.Current.Exit);
                }
                else LifecycleProbe.Activated(activation);
            };
        });
    }

    internal static async Task WaitForRetentionAsync()
    {
        using var process = Process.GetCurrentProcess();
        var marker = Path.Combine(ApplicationData.Current.LocalFolder.Path, $"retained-process-{process.Id}.txt");
        var expected = process.StartTime.ToUniversalTime().Ticks.ToString(System.Globalization.CultureInfo.InvariantCulture);
        var watch = Stopwatch.StartNew();
        while (true)
        {
            try { if (File.Exists(marker) && File.ReadAllText(marker) == expected) return; }
            catch (IOException) { }
            if (watch.Elapsed > TimeSpan.FromSeconds(15)) throw new TimeoutException("The runner never retained this process.");
            await Task.Delay(10).ConfigureAwait(false);
        }
    }

    private static async Task CleanupAsync()
    {
        var passed = false;
        try
        {
            CoreApplication.EnablePrelaunch(false);
            TestApplicationData.RemoveOwnedSettings();
            TestReport.Write(Phase, true, 0);
            File.WriteAllText(Path.Combine(ApplicationData.Current.LocalFolder.Path, "results.txt"), "PASS: lifecycle cleanup.");
            await WaitForRetentionAsync();
            passed = true;
        }
        catch (Exception error)
        {
            File.WriteAllText(Path.Combine(ApplicationData.Current.LocalFolder.Path, "results.txt"), "FAIL: " + error);
        }
        finally { _ = Task.Run(() => Environment.Exit(passed ? 0 : 1)); }
    }

    private sealed partial class CleanupView : IFrameworkViewSource, IFrameworkView
    {
        public IFrameworkView CreateView() => this;
        public void Initialize(CoreApplicationView view) => view.Activated += async (_, _) => await CleanupAsync();
        public void SetWindow(CoreWindow window) { }
        public void Load(string entryPoint) { }
        public void Run() => CoreWindow.GetForCurrentThread().Dispatcher.ProcessEvents(CoreProcessEventsOption.ProcessUntilQuit);
        public void Uninitialize() { }
    }
}
