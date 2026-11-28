// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Globalization;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;

namespace NotepadsIntegrationTests;

internal sealed partial class MarkdownKeyboardFixture
{
    private string _htmlTablesSource;
    private bool _htmlTablesUndo;
    private bool _htmlTablesModified;

    internal async Task PrepareHtmlTablesAsync()
    {
        _htmlTablesSource = MarkdownHtmlTablesFixture.Source;
        _editor.TestNativeCore.SetText(_htmlTablesSource);
        _editor.TestNativeCore.TestFeatureNative.SetEmptySelection(0);
        if (!_editor.TestMarkdownPanelOpen) _editor.ShowHideContentPreview();
        var extension = await MarkdownFeatureTests.CurrentAsync(_editor);
        extension.FollowSource = false;
        extension.PreviewView.TrySetVerticalScrollProgress(0);
        _htmlTablesUndo = _editor.TestNativeCore.CanUndo;
        _htmlTablesModified = _editor.IsModified;
    }

    internal string HtmlTablesState()
    {
        if (_htmlTablesSource == null) throw new InvalidOperationException("No prepared HTML table fixture.");
        var preview = _editor.TestMarkdownPreview.PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var text = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        var result = new StringBuilder();
        result.AppendLine("htmlTablesSourceUnchanged=" + (_editor.GetText() == _htmlTablesSource));
        result.AppendLine("htmlTablesUndoUnchanged=" + (_editor.TestNativeCore.CanUndo == _htmlTablesUndo));
        result.AppendLine("htmlTablesModifiedUnchanged=" + (_editor.IsModified == _htmlTablesModified));
        result.AppendLine("htmlTablesMainExact=" + (preview.DisplayedText == MarkdownHtmlTablesFixture.Main));
        var names = new[] { MarkdownHtmlTablesFixture.Caption, "列一", "列二", MarkdownHtmlTablesFixture.SpanText, "R2C1", "x < y" };
        for (var index = 0; index < names.Length; index++)
        {
            var range = text.DocumentRange.FindText(names[index], false, false);
            if (range == null) throw new InvalidOperationException("Missing actual HTML table text: " + names[index]);
            range.GetBoundingRectangles(out var bounds);
            result.AppendLine("htmlTable" + index + "Name64=" + Convert.ToBase64String(Encoding.UTF8.GetBytes(names[index])));
            result.AppendLine("htmlTable" + index + "Bounds=" + string.Join(",", bounds.Select(value => value.ToString("R", CultureInfo.InvariantCulture))));
        }
        return result.ToString();
    }
}
