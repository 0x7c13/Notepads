// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Notepads.Composition;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Storage;
using Notepads.Presentation.Views.MainPage;
using Notepads.Presentation.Workspace;
using NotepadsWorkspaceTests;
using Windows.Storage;
using Windows.Storage.AccessCache;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;

namespace NotepadsIntegrationTests;

internal sealed class FeatureTestHost : IAsyncDisposable
{
    private readonly Guid _owner;
    private FeatureTestHost(NotepadsMainPage page, StorageFolder folder, Guid owner)
    { Page = page; Folder = folder; _owner = owner; }
    internal NotepadsMainPage Page { get; }
    internal INotepadsCore Core => Page.TestCore;
    internal StorageFolder Folder { get; }

    internal static async Task<FeatureTestHost> CreateAsync(TransferToken windowMove = null)
    {
        var owner = windowMove?.TransferId ?? Guid.NewGuid();
        var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync("Feature-" + owner.ToString("N"));
        var registry = new SessionRegistry();
        var scope = new SessionScopeData { OwnerId = owner, InstanceId = owner, Kind = SessionScopeData.Secondary };
        var sessions = new SessionService(scope, SessionScopeData.GetSecondaryPrefix(owner) + "NotepadsSessionData.json", () => new[] { owner });
        var context = new WindowContext("Integration Tests", owner, windowMove == null, owner, Window.Current.Dispatcher,
            sessions, registry.Register, () => { }, windowMove);
        var frame = new Frame();
        if (!frame.Navigate(typeof(NotepadsMainPage), new WorkspaceNavigation(context, windowMove)))
            throw new InvalidOperationException("Production MainPage navigation failed.");
        var page = (NotepadsMainPage)frame.Content;
        var host = new FeatureTestHost(page, folder, owner);
        try
        {
            Window.Current.Content = frame;
            Window.Current.Activate();
            await WorkspaceTestUI.UntilAsync(() => page.TestStartupComplete, "Feature workspace failed to load.");
            return host;
        }
        catch { await host.DisposeAsync(); throw; }
    }

    public async ValueTask DisposeAsync()
    {
        var progress = Path.Combine(ApplicationData.Current.LocalFolder.Path, "progress.txt");
        File.AppendAllText(progress, "Host disposal started." + Environment.NewLine);
        await Page.TestDisposeWindowAsync().WaitAsync(TimeSpan.FromSeconds(20));
        File.AppendAllText(progress, "Host workspace disposed." + Environment.NewLine);
        Window.Current.Content = null;
        var prefix = SessionDocumentStore.GetFutureAccessTokenPrefix(_owner);
        foreach (var token in StorageApplicationPermissions.FutureAccessList.Entries.Select(entry => entry.Token)
            .Where(token => token.StartsWith(prefix, StringComparison.Ordinal)).ToArray())
            StorageApplicationPermissions.FutureAccessList.Remove(token);
        TestClipboard.Reset();
        TestFileSavePicker.Reset();
        TestFileOpenPicker.Reset();
        TestLauncher.Reset();
        File.AppendAllText(progress, "Host disposal completed." + Environment.NewLine);
    }
}
