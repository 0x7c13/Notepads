// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Presentation.Controls.Markdown;
using Notepads.Presentation.Controls.FindAndReplace;
using Windows.UI.Xaml;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditor
{
    internal MarkdownExtensionView TestMarkdownPreview => _markdownPreview;
    internal bool TestMarkdownPanelOpen => _isContentPreviewPanelOpened;
    internal bool TestHasMarkdownReopenEntry => _markdownReopenCache?.Owns(this) == true;
    internal void TestEvictMarkdownReopenEntry() => _markdownReopenCache?.Evict(this);
    internal void TestSetMarkdownPreviewOpen(bool opened) => SetMarkdownPreviewOpen(opened);
    internal System.Threading.Tasks.Task<bool> TestSourceFindAsync(SearchContext context, bool previous = false,
        FindAndReplaceMode mode = FindAndReplaceMode.FindOnly, string replacement = "") =>
        InitiateFindAndReplaceAsync(new FindAndReplaceEventArgs(context, replacement, mode,
            previous ? SearchDirection.Previous : SearchDirection.Next));
    internal void TestConfirmSourceFindAll() => ConfirmFindAllResult();
    internal void TestSetMarkdownSplitWeights(double source, double preview)
    {
        if (!_isContentPreviewPanelOpened || source <= 0 || preview <= 0)
            throw new System.InvalidOperationException("A visible Markdown split and positive column weights are required.");
        EditorColumnDefinition.Width = new GridLength(source, GridUnitType.Star);
        SplitPanelColumnDefinition.Width = new GridLength(preview, GridUnitType.Star);
    }
}
