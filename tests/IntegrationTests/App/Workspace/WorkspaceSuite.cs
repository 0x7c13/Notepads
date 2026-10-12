// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

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

namespace NotepadsIntegrationTests;

internal static class WorkspaceSuite
{
    internal static async Task RunAsync(string phase, StringBuilder log)
    {
        NotepadsMainPage page = null;
        var owner = Guid.Empty;
        try
        {
            if (phase is not ("--workspace-seed" or "--workspace" or "--workspace-window-close" or "--workspace-restart" or
                "--workspace-clean-seed" or "--workspace-clean-close" or "--workspace-clean-restart" or
                "--workspace-import-seed" or "--workspace-import" or "--workspace-import-restart" or
                "--workspace-import-failed" or "--workspace-opening"))
                throw new ArgumentException("Unknown workspace lifecycle phase.");
            ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, true);
            ApplicationSettingsStore.Write(SettingsKey.ExitWhenLastTabClosed, false);
            ApplicationSettingsStore.Write(SettingsKey.EditorShowStatusBarBool, true);
            var ownerPath = Path.Combine(ApplicationData.Current.LocalFolder.Path, "workspace-owner.txt");
            var seeding = phase.EndsWith("-seed", StringComparison.Ordinal) || phase == "--workspace-opening";
            owner = seeding ? Guid.NewGuid() : Guid.ParseExact(File.ReadAllText(ownerPath), "N");
            if (seeding) File.WriteAllText(ownerPath, owner.ToString("N"));
            var ownedPrefix = SessionDocumentStore.GetFutureAccessTokenPrefix(owner);
            var priorTokens = StorageApplicationPermissions.FutureAccessList.Entries
                .Where(entry => !entry.Token.StartsWith(ownedPrefix, StringComparison.Ordinal))
                .ToDictionary(entry => entry.Token, entry => entry.Metadata, StringComparer.Ordinal);
            if (phase == "--workspace-import-seed")
            {
                await WorkspaceImportTests.SeedAsync(owner, log);
            }
            else
            {
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
                var importing = phase.StartsWith("--workspace-import", StringComparison.Ordinal);
                var initialSelection = phase == "--workspace-opening" ? Guid.Empty : importing ? WorkspaceImportTests.ReadInitialSelection() :
                    WorkspaceLifecycleTests.ReadInitialSelection();
                var gated = phase is "--workspace" or "--workspace-window-close" or "--workspace-clean-close" or
                    "--workspace-import" or "--workspace-import-failed";
                var gate = new RestorePreparationGate(gated, initialSelection);
                page.TestController.BeforeRestorePreparation = gate.BeforePreparationAsync;
                page.TestController.BeforeRestorePromotion = gate.BeforePromotionAsync;
                if (phase == "--workspace-import") await WorkspaceImportTests.VerifyEarlyActivationAsync(page, log);
                Window.Current.Content = frame;
                Window.Current.Activate();
                if (phase == "--workspace-opening") await WorkspaceFileOpeningTests.RunAsync(page, service, log);
                else if (importing) await WorkspaceImportTests.RunAsync(page, gate, phase, log);
                else await WorkspaceLifecycleTests.RunAsync(page, service, registry, gate, phase, owner, log);
                await page.TestDisposeWindowAsync();
                page = null;
            }
            var currentTokens = StorageApplicationPermissions.FutureAccessList.Entries.ToDictionary(entry => entry.Token, entry => entry.Metadata, StringComparer.Ordinal);
            foreach (var prior in priorTokens)
                if (!currentTokens.TryGetValue(prior.Key, out var metadata) || metadata != prior.Value)
                    throw new InvalidOperationException("An unrelated future-access token changed: " + prior.Key);
            if (phase.EndsWith("-restart", StringComparison.Ordinal) || phase is "--workspace-import-failed" or "--workspace-opening")
                RemoveOwnedFileGrants(owner);
            log.AppendLine("PASS: real workspace lifecycle phase " + phase);
            log.AppendLine("  Isolated root: " + ApplicationData.Current.LocalFolder.Path);
        }
        catch (Exception)
        {
            if (page != null)
            {
                try { await page.TestDisposeWindowAsync(); }
                catch (Exception shutdown) { log.AppendLine("FAIL: shutdown " + shutdown); }
            }
            if (owner != Guid.Empty) RemoveOwnedFileGrants(owner);
            throw;
        }
    }

    private static void RemoveOwnedFileGrants(Guid owner)
    {
        var prefix = SessionDocumentStore.GetFutureAccessTokenPrefix(owner);
        foreach (var token in StorageApplicationPermissions.FutureAccessList.Entries.Select(entry => entry.Token)
            .Where(token => token.StartsWith(prefix, StringComparison.Ordinal)).ToArray())
            StorageApplicationPermissions.FutureAccessList.Remove(token);
    }
}
