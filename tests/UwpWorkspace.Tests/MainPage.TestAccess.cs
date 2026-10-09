// Copyright (c) 2026, Jiaqi (0x7c13) Liu. See LICENSE.
// This partial is compiled only into the isolated source-linked harness.
using System.Linq;
using System.Threading.Tasks;
using Notepads.Controls;
using Notepads.Presentation.Workspace;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;

namespace Notepads.Presentation.Views.MainPage;

public sealed partial class NotepadsMainPage
{
    internal INotepadsCore TestCore => NotepadsCore;
    internal SessionController TestController => (SessionController)SessionController;
    internal bool TestStartupComplete => _loaded;
    internal Task TestDisposeWindowAsync() => DisposeWindowAsync();
    internal Task TestCloseTabAsync(WorkspaceTab tab) => RequestTabCloseAsync(tab);
    internal Task<bool> TestCloseWindowAsync() => RequestWindowCloseAsync();
    internal Task<bool> TestSaveSelectedAsAsync() => SaveAsync(NotepadsCore.GetSelectedReadyEditor(), saveAs: true);
    internal void TestRefreshMenu() => MainMenuButtonFlyout_Opening(MainMenuButtonFlyout, null);
    internal bool TestSaveAsEnabled => MenuSaveAsButton.IsEnabled;
    internal bool TestFindEnabled => MenuFindButton.IsEnabled;
    internal string TestStatusPath => PathIndicator?.Text;
    internal string TestStatusPosition => LineColumnIndicator?.Text;
    internal bool TestStatusModified => ModificationIndicator?.Visibility == Visibility.Visible;
    internal SetsViewItem TestContainer(WorkspaceTab tab) => Sets.Items.Cast<SetsViewItem>().Single(item => ReferenceEquals(item.Tag, tab));
    internal ContentControl TestContentHost(WorkspaceTab tab) => (ContentControl)TestContainer(tab).Content;
}
