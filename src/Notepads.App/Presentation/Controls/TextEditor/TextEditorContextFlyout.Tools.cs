// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Features.Documents.Text;
using Windows.UI.Xaml.Controls;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditorContextFlyout
{
    private MenuFlyoutSubItem _lineTools;

    private MenuFlyoutItem ToolItem(string resource, Action action)
    {
        var item = new MenuFlyoutItem { Text = _resourceLoader.GetString(resource) };
        item.Click += (_, _) => action();
        return item;
    }

    private void AddEditorTools()
    {
        _lineTools = new MenuFlyoutSubItem { Text = _resourceLoader.GetString("EditorTools_SelectedLines") };
        if (_textEditor is TextEditor editor)
        {
            _lineTools.Items.Add(ToolItem("EditorTools_SortAscending", async () => await editor.TransformSelectedLinesAsync(SelectedLineOperation.SortAscending)));
            _lineTools.Items.Add(ToolItem("EditorTools_SortDescending", async () => await editor.TransformSelectedLinesAsync(SelectedLineOperation.SortDescending)));
            _lineTools.Items.Add(ToolItem("EditorTools_RemoveDuplicates", async () => await editor.TransformSelectedLinesAsync(SelectedLineOperation.RemoveDuplicates)));
        }
        Items.Add(_lineTools);
    }
}
