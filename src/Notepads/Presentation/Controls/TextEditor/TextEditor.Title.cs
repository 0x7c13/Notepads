// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Features.Documents.Text;
using Windows.UI.Xaml;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditor
{
    public event EventHandler DisplayTitleChanged;

    private bool _useContentBasedTitle;
    private long _contentTitleVersion = -1;
    private string _contentTitle;
    private string _publishedDisplayTitle;

    public bool UsesContentBasedTitle => _useContentBasedTitle;

    public string DisplayTitle => EditingFileName ??
        (_useContentBasedTitle ? GetContentTitle() : null) ?? FileNamePlaceholder;

    private bool IsDefaultUntitledName(string name) =>
        string.Equals(name, _resourceLoader.GetString("TextEditor_DefaultNewFileName"), StringComparison.Ordinal);

    private string GetContentTitle()
    {
        if (!_disposed && TextEditorCore != null && _contentTitleVersion != TextEditorCore.LeadingTextVersion)
        {
            _contentTitle = DocumentTitle.FromPrefix(TextEditorCore.GetTextPrefix(DocumentTitle.SampleBytes),
                complete: TextEditorCore.DocumentLength <= DocumentTitle.SampleBytes);
            _contentTitleVersion = TextEditorCore.LeadingTextVersion;
        }
        return _contentTitle;
    }

    private void TextEditorCore_OnTitleTextChanged(object sender, RoutedEventArgs args) => RefreshDisplayTitle();

    private void RefreshDisplayTitle()
    {
        if (_disposed) return;
        var title = DisplayTitle;
        if (string.Equals(_publishedDisplayTitle, title, StringComparison.Ordinal)) return;
        _publishedDisplayTitle = title;
        DisplayTitleChanged?.Invoke(this, EventArgs.Empty);
    }
}
