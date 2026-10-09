// Copyright (c) 2026, Jiaqi (0x7c13) Liu. See LICENSE.
using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Composition;
using Notepads.Features.Preferences;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Storage;
using Notepads.Infrastructure.Settings;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Theming;
using Notepads.Presentation.Views.MainPage;
using Notepads.Presentation.Workspace;
using NotepadsTestIsolation;
using NotepadsWorkspaceTests;
using Windows.ApplicationModel.Activation;
using Windows.Storage.AccessCache;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;

namespace Notepads;

// Uses the production App.xaml dictionaries, but never bootstraps a named app instance.
public sealed partial class App : Application
{
    public App()
    {
        UnhandledException += (_, args) => WriteResult("FAIL: unhandled " + args.Exception);
        InitializeComponent();
        File.WriteAllText(Path.Combine(ApplicationData.Current.LocalFolder.Path, "diagnostic.txt"), "Production resources initialized.");
    }

    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        var log = new StringBuilder();
        NotepadsMainPage page = null;
        var owner = Guid.Empty;
        try
        {
            await LoggingService.InitializeAsync(new WorkspaceLogSink());
            var phase = TestApplicationData.Arguments;
            if (phase is not ("--workspace-seed" or "--workspace" or "--workspace-window-close" or "--workspace-restart" or
                "--workspace-clean-seed" or "--workspace-clean-close" or "--workspace-clean-restart"))
                throw new ArgumentException("Unknown workspace lifecycle phase.");
            ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, true);
            ApplicationSettingsStore.Write(SettingsKey.ExitWhenLastTabClosed, false);
            ApplicationSettingsStore.Write(SettingsKey.EditorShowStatusBarBool, true);
            ThemeSettingsService.Initialize();
            ApplicationPreferences.Initialize();
            var ownerPath = Path.Combine(ApplicationData.Current.LocalFolder.Path, "workspace-owner.txt");
            var seeding = phase.EndsWith("-seed", StringComparison.Ordinal);
            owner = seeding ? Guid.NewGuid() : Guid.ParseExact(File.ReadAllText(ownerPath), "N");
            if (seeding) File.WriteAllText(ownerPath, owner.ToString("N"));
            var ownedPrefix = SessionDocumentStore.GetFutureAccessTokenPrefix(owner);
            var priorTokens = StorageApplicationPermissions.FutureAccessList.Entries
                .Where(entry => !entry.Token.StartsWith(ownedPrefix, StringComparison.Ordinal))
                .ToDictionary(entry => entry.Token, entry => entry.Metadata, StringComparer.Ordinal);
            // Secondary-scope format requires owner and instance identity to match.
            // This GUID is private to the run and is never registered with AppInstance.
            var instance = owner;
            var registry = new SessionRegistry();
            var scope = new SessionScopeData { OwnerId = owner, InstanceId = instance, Kind = SessionScopeData.Secondary };
            var service = new SessionService(scope, SessionScopeData.GetSecondaryPrefix(owner) + "NotepadsSessionData.json", () => new Guid[] { instance });
            var context = new WindowContext("Workspace Tests", instance, false, owner, Window.Current.Dispatcher,
                service, registry.Register, () => { });
            var frame = new Frame();
            if (!frame.Navigate(typeof(NotepadsMainPage), new WorkspaceNavigation(context, null)))
                throw new InvalidOperationException("Production MainPage navigation failed.");
            page = (NotepadsMainPage)frame.Content;
            var gate = new RestorePreparationGate(phase is "--workspace" or "--workspace-window-close" or "--workspace-clean-close", WorkspaceLifecycleTests.ReadInitialSelection());
            page.TestController.BeforeRestorePreparation = gate.BeforePreparationAsync;
            page.TestController.BeforeRestorePromotion = gate.BeforePromotionAsync;
            Window.Current.Content = frame;
            Window.Current.Activate();
            await WorkspaceLifecycleTests.RunAsync(page, service, registry, gate, phase, owner, log);
            await page.TestDisposeWindowAsync();
            page = null;
            var currentTokens = StorageApplicationPermissions.FutureAccessList.Entries.ToDictionary(entry => entry.Token, entry => entry.Metadata, StringComparer.Ordinal);
            foreach (var prior in priorTokens)
                if (!currentTokens.TryGetValue(prior.Key, out var metadata) || metadata != prior.Value)
                    throw new InvalidOperationException("An unrelated future-access token changed: " + prior.Key);
            if (phase.EndsWith("-restart", StringComparison.Ordinal))
                RemoveOwnedFileGrants(owner);
            log.AppendLine("PASS: real workspace lifecycle phase " + phase + "; isolated root " + ApplicationData.Current.LocalFolder.Path);
        }
        catch (Exception error)
        {
            log.AppendLine("FAIL: " + error);
            File.WriteAllText(Path.Combine(ApplicationData.Current.LocalFolder.Path, "failure.txt"), log.ToString());
            if (page != null)
            {
                try { await page.TestDisposeWindowAsync(); }
                catch (Exception shutdown) { log.AppendLine("FAIL: shutdown " + shutdown); }
            }
            if (owner != Guid.Empty) RemoveOwnedFileGrants(owner);
        }
        WriteResult(log.ToString());
        await LoggingService.FlushAsync();
        Exit();
    }

    private static void WriteResult(string text) => File.WriteAllText(Path.Combine(ApplicationData.Current.LocalFolder.Path, "results.txt"), text);

    private static void RemoveOwnedFileGrants(Guid owner)
    {
        var prefix = SessionDocumentStore.GetFutureAccessTokenPrefix(owner);
        foreach (var token in StorageApplicationPermissions.FutureAccessList.Entries.Select(entry => entry.Token)
            .Where(token => token.StartsWith(prefix, StringComparison.Ordinal)).ToArray())
            StorageApplicationPermissions.FutureAccessList.Remove(token);
    }
}
