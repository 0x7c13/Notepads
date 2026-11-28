// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Globalization;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;

namespace NotepadsIntegrationTests;

// Real pointer/keyboard input is supplied by the owned external desktop driver.
// This fixture only prepares source and observes the product's actual state.
internal sealed partial class MarkdownKeyboardFixture
{
    private MarkdownView _htmlControlsView;
    private int _htmlControlsRequests;
    private bool _htmlControlsUndo;
    private bool _htmlControlsModified;

    internal async Task PrepareHtmlControlsAsync()
    {
        ReleaseHtmlControls();
        _editor.TestNativeCore.SetText(MarkdownHtmlControlsFixture.Source);
        _editor.TestNativeCore.TestFeatureNative.SetEmptySelection(0);
        if (!_editor.TestMarkdownPanelOpen) _editor.ShowHideContentPreview();
        var extension = await MarkdownFeatureTests.CurrentAsync(_editor);
        extension.FollowSource = false;
        _htmlControlsView = extension.PreviewView;
        _htmlControlsView.TrySetVerticalScrollProgress(0);
        _htmlControlsView.TaskInteractionEnabled = true;
        _htmlControlsView.TaskToggleRequested += ObserveHtmlControlsRequest;
        _htmlControlsRequests = 0;
        _htmlControlsUndo = _editor.TestNativeCore.CanUndo;
        _htmlControlsModified = _editor.IsModified;
    }

    private void ObserveHtmlControlsRequest(MarkdownView sender, MarkdownTaskToggleRequestedEventArgs args) =>
        _htmlControlsRequests++;

    internal string HtmlControlsState()
    {
        if (_htmlControlsView == null) throw new InvalidOperationException("No prepared HTML controls fixture.");
        var peer = FrameworkElementAutomationPeer.FromElement(_htmlControlsView) ??
            FrameworkElementAutomationPeer.CreatePeerForElement(_htmlControlsView);
        var children = peer.GetChildren();
        var result = new StringBuilder();
        result.AppendLine("htmlControlsTaskRequests=" + _htmlControlsRequests);
        result.AppendLine("htmlControlsSourceUnchanged=" + (_editor.GetText() == MarkdownHtmlControlsFixture.Source));
        result.AppendLine("htmlControlsUndoUnchanged=" + (_editor.TestNativeCore.CanUndo == _htmlControlsUndo));
        result.AppendLine("htmlControlsModifiedUnchanged=" + (_editor.IsModified == _htmlControlsModified));
        for (var index = 0; index < MarkdownHtmlControlsFixture.Labels.Length; index++)
        {
            var label = MarkdownHtmlControlsFixture.Labels[index];
            var checkbox = children.Single(child => child.GetName() == label);
            var bounds = checkbox.GetBoundingRectangle();
            var toggle = (IToggleProvider)checkbox.GetPattern(PatternInterface.Toggle);
            var prefix = "htmlCheckbox" + index;
            result.AppendLine(prefix + "Name64=" + Convert.ToBase64String(Encoding.UTF8.GetBytes(label)));
            result.AppendLine(prefix + "Type=" + checkbox.GetAutomationControlType());
            result.AppendLine(prefix + "Enabled=" + checkbox.IsEnabled());
            result.AppendLine(prefix + "Focusable=" + checkbox.IsKeyboardFocusable());
            result.AppendLine(prefix + "State=" + toggle.ToggleState);
            result.AppendLine(prefix + "Bounds=" + string.Join(",", new[] { bounds.X, bounds.Y, bounds.Width, bounds.Height }
                .Select(value => value.ToString("R", CultureInfo.InvariantCulture))));
        }
        return result.ToString();
    }

    private void ReleaseHtmlControls()
    {
        if (_htmlControlsView == null) return;
        _htmlControlsView.TaskToggleRequested -= ObserveHtmlControlsRequest;
        _htmlControlsView.TaskInteractionEnabled = false;
        _htmlControlsView = null;
    }
}
