// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Settings;
using Notepads.Presentation.Theming;
using NotepadsEditorTests;
using NotepadsIntegrationTests;
using NotepadsTestIsolation;
using NotepadsWorkspaceTests;
using Windows.ApplicationModel.Activation;
using Windows.UI.Xaml;

namespace Notepads;

// Actual product resources and services; only bootstrap and declared outbound
// boundaries differ. Every ordinary phase gets a fresh process and test run ID.
public sealed partial class App : Application
{
    public App()
    {
        UnhandledException += (_, args) => WriteResult("FAIL: unhandled " + args.Exception);
        InitializeComponent();
    }

    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        var log = new StringBuilder();
        var phase = TestApplicationData.Arguments;
        var watch = Stopwatch.StartNew();
        var success = false;
        try
        {
            await LoggingService.InitializeAsync(new WorkspaceLogSink());
            if (phase == "--test-cleanup")
            {
                TestApplicationData.RemoveOwnedSettings();
                log.AppendLine("PASS: owned settings container removed.");
            }
            else
            {
                InitializePreferences(phase);
                if (phase.StartsWith("--workspace-", StringComparison.Ordinal) && phase != "--workspace-commands" || phase == "--workspace")
                    await WorkspaceSuite.RunAsync(phase, log);
                else await RunFeatureAsync(phase, log);
                if (phase.StartsWith("--interactive-test ", StringComparison.Ordinal)) return;
            }
            TestApplicationData.ValidatePreservedFileGrants();
            success = true;
            log.AppendLine($"PASS: {phase} ({watch.ElapsedMilliseconds} ms).");
        }
        catch (Exception error) { log.AppendLine("FAIL: " + error); }
        try { await LoggingService.FlushAsync(); }
        catch (Exception error) { success = false; log.AppendLine("FAIL: diagnostics cleanup " + error); }
        TestReport.Write(phase, success, watch.ElapsedMilliseconds);
        WriteResult(log.ToString());
        // The test host owns its process status after awaited fixture cleanup.
        // UWP Application.Exit does not specify a numeric success exit code.
        // Keep the UI dispatcher available while CLR/COM shutdown releases
        // apartment-bound resources; blocking this thread can deadlock teardown.
        _ = Task.Run(() => Environment.Exit(success ? 0 : 1));
    }

    private static void InitializePreferences(string phase)
    {
        // Explicit restart phases retain values written by the seed process.
        if (phase is not ("--preferences-seed" or "--preferences-restart"))
        {
            ApplicationSettingsStore.Write(SettingsKey.RequestedThemeStr, "Dark");
            ApplicationSettingsStore.Write(SettingsKey.UseWindowsThemeBool, false);
            ApplicationSettingsStore.Write(SettingsKey.UseWindowsAccentColorBool, false);
            ApplicationSettingsStore.Write(SettingsKey.AppAccentColorHexStr, "#FF1E90FF");
            ApplicationSettingsStore.Write(SettingsKey.EditorDefaultDisplayLineNumbersBool, true);
            ApplicationSettingsStore.Write(SettingsKey.EditorDefaultLineHighlighterViewStateBool, true);
            ApplicationSettingsStore.Write(SettingsKey.EditorDefaultTextWrappingStr, "NoWrap");
            ApplicationSettingsStore.Write(SettingsKey.EditorDefaultTabIndentsInt, 4);
            ApplicationSettingsStore.Write(SettingsKey.EditorShowStatusBarBool, true);
            ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool,
                phase.StartsWith("--workspace", StringComparison.Ordinal) && phase != "--workspace-commands");
            ApplicationSettingsStore.Write(SettingsKey.ExitWhenLastTabClosed, false);
        }
        ThemeSettingsService.Initialize();
        ApplicationPreferences.Initialize();
    }

    private static Task RunFeatureAsync(string phase, StringBuilder log) => phase switch
    {
        "--documents" => DocumentWorkflowsTests.RunAsync(log),
        "--activation" => ShellWorkflowTests.RunActivationAsync(log),
        "--open-picker" => ShellWorkflowTests.RunOpenPickerAsync(log),
        "--outbound" => ShellWorkflowTests.RunOutboundAsync(log),
        "--printing" => ShellWorkflowTests.RunPrintingAsync(log),
        "--editing-ui" => EditingUiTests.RunAsync(log),
        "--clipboard" => ClipboardContractTests.RunAsync(log),
        "--clipboard-rejected-cut" => ClipboardContractTests.RunRejectedCutAsync(log),
        "--workspace-commands" => WorkspaceCommandFeatureTests.RunAsync(log),
        "--print-state" => WorkspaceCommandFeatureTests.RunPrintStateAsync(log),
        "--preferences" => PreferenceFeatureTests.RunAsync(log),
        "--preferences-store" => PreferenceFeatureTests.RunStoreAsync(log),
        "--preferences-corrupt-encoding" => PreferenceFeatureTests.RunCorruptEncodingAsync(log),
        "--preferences-seed" => PreferenceFeatureTests.SeedAsync(log),
        "--preferences-restart" => PreferenceFeatureTests.VerifyRestartAsync(log),
        "--appearance" => AppearanceFeatureTests.RunAsync(log),
        "--zoom-language" => AppearanceFeatureTests.RunZoomAndLanguageAsync(log),
        "--view-modes" => AppearanceFeatureTests.RunViewModeAsync(log),
        "--editor" => EditorSuite.RunAsync("", log),
        "--scroll-restoration" => ScrollRestorationTests.RunAsync(log),
        "--persistence" or "--session-resilience" or "--syntax" or "--diff" or "--search" or
        "--automation-navigation" or "--native-storage" or "--native-faults" or "--regex-memory" or "--performance" or
        "--preview" or "--bidi-preview" => EditorSuite.RunAsync(phase, log),
        _ when phase.StartsWith("--lease-", StringComparison.Ordinal) || phase.StartsWith("--restore-perf-", StringComparison.Ordinal) ||
            phase.StartsWith("--interactive-test ", StringComparison.Ordinal) => EditorSuite.RunAsync(phase, log),
        _ => throw new ArgumentException("Unknown integration phase: " + phase)
    };

    private static void WriteResult(string text) => File.WriteAllText(Path.Combine(ApplicationData.Current.LocalFolder.Path, "results.txt"), text);
}
