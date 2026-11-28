// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Features.Documents.FileTypes;
using Windows.System;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Media;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditorContextFlyout
{
    private MenuFlyoutItem _previewToggle;

    public MenuFlyoutItem PreviewToggle
    {
        get
        {
            if (_previewToggle != null) return _previewToggle;

            _previewToggle = new MenuFlyoutItem
            {
                Text = _resourceLoader.GetString("TextEditor_ContextFlyout_PreviewToggleDisplay_Text"),
                Icon = new FontIcon()
                {
                    FontFamily = new FontFamily("Segoe MDL2 Assets"),
                    Glyph = "\uE89F"
                }
            };
            _previewToggle.KeyboardAccelerators.Add(new KeyboardAccelerator()
            {
                Modifiers = VirtualKeyModifiers.Menu,
                Key = VirtualKey.P,
                IsEnabled = false,
            });
            _previewToggle.Click += (sender, args) =>
            {
                if (FileTypeUtility.IsPreviewSupported(_textEditor.FileType, _textEditor.DocumentLanguage?.Id))
                {
                    _textEditor.ShowHideContentPreview();
                }
            };
            return _previewToggle;
        }
    }
}
