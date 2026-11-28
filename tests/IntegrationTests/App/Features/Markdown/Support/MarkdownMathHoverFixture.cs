// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Threading.Tasks;
using Windows.UI.Xaml.Automation.Peers;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal sealed partial class MarkdownKeyboardFixture
{
    internal async Task PrepareMathHoverAsync(string scenario)
    {
        if (scenario is not ("error" or "ready")) throw new ArgumentException("Declared math hover fixture only.");
        string formula = scenario == "error" ? @"\frac{1}{" : @"\frac{1}{2}";
        _editor.TestNativeCore.SetText("# Formula diagnostics\r\rInline $" + formula + "$\r\r" +
            "A failed formula keeps its original source. Hover its fallback for details.\r\r" +
            "![missing image](missing.png)\r\r" + string.Join("\r\r", Enumerable.Repeat("A paragraph for scrolling.", 30)));
        if (!_editor.TestMarkdownPanelOpen) _editor.ShowHideContentPreview();
        var preview = (await MarkdownFeatureTests.CurrentAsync(_editor)).PreviewView;
        await UntilAsync(() =>
        {
            var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
            return peer.GetChildren()?.Any(child => child.GetName().StartsWith("Formula:", StringComparison.Ordinal) &&
                child.GetHelpText().StartsWith(scenario == "error" ? "Formula syntax error." : "Formula rendered.", StringComparison.Ordinal)) == true;
        }, "The real published formula reaches its expected diagnostic state.");
    }
}
