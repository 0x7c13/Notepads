// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.Markdown;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    private static async Task PreviewDestinationPathsAsync(StringBuilder log)
    {
        const string origin = @"D:\notes\source.md";
        foreach (var destination in new[]
        {
            "/README.md", @"\README.md", "C:note.md", "custom:foo", "file:///README.md",
            @"\\?\C:\notes\target.md", @"\\.\C:\notes\target.md", "//?/C:/notes/target.md",
            "%5C%5C%3F%5CC%3A%5Cnotes%5Ctarget.md", @"\\??\C:\notes\target.md",
            @"\\?\GLOBALROOT\Device\HarddiskVolume1\target.md", "//??/C:/notes/target.md"
        })
            Check(!MarkdownExtensionView.TryResolvePreviewDestination(destination, false, origin, out _, out _, out _),
                $"The preview rejects a destination without an explicit file base: {destination}.");
        Check(!MarkdownExtensionView.TryResolvePreviewDestination("target.md", false, null, out _, out _, out _) &&
            !MarkdownExtensionView.TryResolvePreviewDestination("target.md", false, "source.md", out _, out _, out _) &&
            !MarkdownExtensionView.TryResolvePreviewDestination("C:note", true, origin, out _, out _, out _),
            "Relative and wiki links never acquire a process current directory or drive.");
        Check(MarkdownExtensionView.TryResolvePreviewDestination("target%20%23guide.md?view=local#section", false, origin,
            out var remote, out var fullPath, out var fragment) && remote == null &&
            fullPath == @"D:\notes\target #guide.md" && fragment == "#section",
            "Relative file links decode their filename once and preserve their fragment.");
        Check(MarkdownExtensionView.TryResolvePreviewDestination("target 25%20 会议 1.2#安装 指南", true, origin,
            out remote, out fullPath, out fragment) && remote == null &&
            fullPath == @"D:\notes\target 25%20 会议 1.2" && fragment == "安装 指南",
            "Wiki filenames retain literal percent escapes and Unicode heading titles.");
        foreach (var destination in new[] { @"C:\notes\target.md", "file:///C:/notes/target.md" })
            Check(MarkdownExtensionView.TryResolvePreviewDestination(destination, false, null, out remote, out fullPath,
                out fragment) && remote == null && fullPath == @"C:\notes\target.md" && fragment == null,
                "A fully qualified Windows path or file URI needs no source document base.");
        foreach (var destination in new[] { @"C:\notes\my%20guide.md?v=1#intro", "C:/notes/my%20guide.md?v=1#intro" })
            Check(MarkdownExtensionView.TryResolvePreviewDestination(destination, false, null,
                out remote, out fullPath, out fragment) && remote == null &&
                fullPath == @"C:\notes\my guide.md" && fragment == "#intro",
                "Native Windows drive links decode filenames once and separate query/fragment suffixes.");
        Check(MarkdownExtensionView.TryResolvePreviewDestination("images/icon.png", false,
            @"\\server\share\notes\source.md", out remote, out fullPath, out fragment) && remote == null &&
            fullPath == @"\\server\share\notes\images\icon.png" && fragment == null,
            "A legitimate UNC document remains a base for its relative resources.");
        Check(MarkdownExtensionView.TryResolvePreviewDestination(@"\\server\share\target.md", false, null,
            out remote, out fullPath, out fragment) && remote == null && fullPath == @"\\server\share\target.md" &&
            MarkdownExtensionView.TryResolvePreviewDestination("file:///C:/notes/target%2520.md#section", false, null,
                out remote, out fullPath, out fragment) && remote == null && fullPath == @"C:\notes\target%20.md" && fragment == "#section",
            "UNC links stay fully qualified and file URI paths decode exactly once.");
        foreach (var destination in new[] { "https://example.test/path?view=1#section", "http://example.test/", "mailto:me@example.test" })
            Check(MarkdownExtensionView.TryResolvePreviewDestination(destination, false, null, out remote, out fullPath,
                out fragment) && remote?.AbsoluteUri == new Uri(destination).AbsoluteUri && fullPath == null && fragment == null,
                "Supported external destinations retain their URI and bypass local path resolution.");

        await using var host = await FeatureTestHost.CreateAsync();
        var source = await OpenAsync(host, "invalid-link.md", Encoding.UTF8.GetBytes(
            "[Root relative](/README.md) [Drive relative](C:note.md) [Unknown scheme](custom:foo)"));
        source.ShowHideContentPreview();
        var extension = await CurrentAsync(source);
        var preview = extension.PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var status = (TextBlock)extension.FindName("PreviewStatus");
        var tabCount = host.Core.GetTabCount();
        foreach (var (label, destination) in new[]
        {
            ("Root relative", "/README.md"), ("Drive relative", "C:note.md"), ("Unknown scheme", "custom:foo")
        })
        {
            IInvokeProvider action = null;
            await UntilAsync(() =>
            {
                var link = peer.GetChildren()?.SingleOrDefault(child => child.GetName() == label);
                action = link?.GetPattern(PatternInterface.Invoke) as IInvokeProvider;
                return action != null && link.GetHelpText() == destination;
            }, "Accepted Markdown exposes the original destination for " + label + ".");
            status.Visibility = Visibility.Collapsed;
            action.Invoke();
            await UntilAsync(() => status.Visibility == Visibility.Visible && !string.IsNullOrEmpty(status.Text),
                "An invalid clicked destination shows the existing link-failed status.");
            Check(ReferenceEquals(host.Core.GetSelectedReadyEditor(), source) && host.Core.GetTabCount() == tabCount &&
                TestLauncher.Requests.Count == 0, "Invalid destinations leave the workspace and external launcher untouched.");
        }
        log.AppendLine("PASS: Markdown.LinkDestinations — rejects process-relative paths, device paths and unsupported schemes; preserves file, external and wiki semantics; invalid UI Automation links fail cleanly.");
    }
}
