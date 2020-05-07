namespace Notepads.Controls.TextEditor
{
    using System;
    using Notepads.Utilities;
    using Windows.ApplicationModel.Resources;
    using Windows.System;
    using Windows.UI.Text;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;

    public class TextEditorContextFlyout : MenuFlyout
    {
        private MenuFlyoutItem _cut;
        private MenuFlyoutItem _copy;
        private MenuFlyoutItem _paste;
        private MenuFlyoutItem _undo;
        private MenuFlyoutItem _redo;
        private MenuFlyoutItem _selectAll;
        private MenuFlyoutItem _rightToLeftReadingOrder;
        private MenuFlyoutItem _webSearch;
        private MenuFlyoutItem _wordWrap;
        private MenuFlyoutItem _previewToggle;
        private MenuFlyoutItem _share;

        private readonly ITextEditor _textEditor;
        private readonly TextEditorCore _textEditorCore;

        private readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

        public TextEditorContextFlyout(ITextEditor editor, TextEditorCore editorCore)
        {
            _textEditor = editor;
            _textEditorCore = editorCore;
            Items.Add(Cut);
            Items.Add(Copy);
            Items.Add(Paste);
            Items.Add(Undo);
            Items.Add(Redo);
            Items.Add(SelectAll);
            Items.Add(new MenuFlyoutSeparator());
            Items.Add(RightToLeftReadingOrder);
            Items.Add(WordWrap);
            Items.Add(WebSearch);
            Items.Add(PreviewToggle);
            Items.Add(Share);

            Opening += TextEditorContextFlyout_Opening;
        }

        public void Dispose()
        {
            Opening -= TextEditorContextFlyout_Opening;
        }

        private void TextEditorContextFlyout_Opening(object sender, object e)
        {
            if (_textEditorCore.Document.Selection.Type == SelectionType.InsertionPoint ||
                _textEditorCore.Document.Selection.Type == SelectionType.None)
            {
                PrepareForInsertionMode();
            }
            else
            {
                PrepareForSelectionMode();
            }

            Undo.IsEnabled = _textEditorCore.Document.CanUndo();
            Redo.IsEnabled = _textEditorCore.Document.CanRedo();

            PreviewToggle.Visibility = FileTypeUtility.IsPreviewSupported(_textEditor.FileType) ? Visibility.Visible : Visibility.Collapsed;
            WordWrap.Icon.Visibility = (_textEditorCore.TextWrapping == TextWrapping.Wrap) ? Visibility.Visible : Visibility.Collapsed;
            RightToLeftReadingOrder.Icon.Visibility = (_textEditorCore.FlowDirection == FlowDirection.RightToLeft) ? Visibility.Visible : Visibility.Collapsed;

            if (App.IsGameBarWidget)
            {
                Share.Visibility = Visibility.Collapsed;
            }
        }

        public void PrepareForInsertionMode()
        {
            Cut.Visibility = Visibility.Collapsed;
            Copy.Visibility = Visibility.Collapsed;
            RightToLeftReadingOrder.Visibility = !string.IsNullOrEmpty(_textEditor.GetText()) ? Visibility.Visible : Visibility.Collapsed;
            WebSearch.Visibility = Visibility.Collapsed;
            Share.Text = _resourceLoader.GetString("TextEditor_ContextFlyout_ShareButtonDisplayText");
        }

        public void PrepareForSelectionMode()
        {
            Cut.Visibility = Visibility.Visible;
            Copy.Visibility = Visibility.Visible;
            RightToLeftReadingOrder.Visibility = !string.IsNullOrEmpty(_textEditor.GetText()) ? Visibility.Visible : Visibility.Collapsed;
            WebSearch.Visibility = Visibility.Visible;
            Share.Text = _resourceLoader.GetString("TextEditor_ContextFlyout_ShareSelectedButtonDisplayText");
        }

        public MenuFlyoutItem Cut
        {
            get
            {
                if (_cut == null)
                {
                    _cut = new MenuFlyoutItem { Icon = new SymbolIcon(Symbol.Cut), Text = _resourceLoader.GetString("TextEditor_ContextFlyout_CutButtonDisplayText") };
                    _cut.KeyboardAccelerators.Add(new KeyboardAccelerator()
                    {
                        Modifiers = VirtualKeyModifiers.Control,
                        Key = VirtualKey.X,
                        IsEnabled = false,
                    });
                    _cut.Click += (sender, args) => { _textEditorCore.Document.Selection.Cut(); };
                }
                return _cut;
            }
        }

        public MenuFlyoutItem Copy
        {
            get
            {
                if (_copy == null)
                {
                    _copy = new MenuFlyoutItem { Icon = new SymbolIcon(Symbol.Copy), Text = _resourceLoader.GetString("TextEditor_ContextFlyout_CopyButtonDisplayText") };
                    _copy.KeyboardAccelerators.Add(new KeyboardAccelerator()
                    {
                        Modifiers = VirtualKeyModifiers.Control,
                        Key = VirtualKey.C,
                        IsEnabled = false,
                    });
                    _copy.Click += (sender, args) => _textEditor.CopySelectedTextToWindowsClipboard(null);
                }
                return _copy;
            }
        }

        public MenuFlyoutItem Paste
        {
            get
            {
                if (_paste == null)
                {
                    _paste = new MenuFlyoutItem { Icon = new SymbolIcon(Symbol.Paste), Text = _resourceLoader.GetString("TextEditor_ContextFlyout_PasteButtonDisplayText") };
                    _paste.KeyboardAccelerators.Add(new KeyboardAccelerator()
                    {
                        Modifiers = VirtualKeyModifiers.Control,
                        Key = VirtualKey.V,
                        IsEnabled = false,
                    });
                    _paste.Click += async (sender, args) => { await _textEditorCore.PastePlainTextFromWindowsClipboard(null); };
                }
                return _paste;
            }
        }

        public MenuFlyoutItem Undo
        {
            get
            {
                if (_undo == null)
                {
                    _undo = new MenuFlyoutItem { Icon = new SymbolIcon(Symbol.Undo), Text = _resourceLoader.GetString("TextEditor_ContextFlyout_UndoButtonDisplayText") };
                    _undo.KeyboardAccelerators.Add(new KeyboardAccelerator()
                    {
                        Modifiers = VirtualKeyModifiers.Control,
                        Key = VirtualKey.Z,
                        IsEnabled = false,
                    });
                    _undo.Click += (sender, args) => { _textEditorCore.Undo(); };
                }
                return _undo;
            }
        }

        public MenuFlyoutItem Redo
        {
            get
            {
                if (_redo == null)
                {
                    _redo = new MenuFlyoutItem { Icon = new SymbolIcon(Symbol.Redo), Text = _resourceLoader.GetString("TextEditor_ContextFlyout_RedoButtonDisplayText") };
                    _redo.KeyboardAccelerators.Add(new KeyboardAccelerator()
                    {
                        Modifiers = (VirtualKeyModifiers.Control & VirtualKeyModifiers.Shift),
                        Key = VirtualKey.Z,
                        IsEnabled = false,
                    });
                    _redo.KeyboardAcceleratorTextOverride = "Ctrl+Shift+Z";
                    _redo.Click += (sender, args) => { _textEditorCore.Redo(); };
                }
                return _redo;
            }
        }

        public MenuFlyoutItem SelectAll
        {
            get
            {
                if (_selectAll == null)
                {
                    _selectAll = new MenuFlyoutItem { Icon = new SymbolIcon(Symbol.SelectAll), Text = _resourceLoader.GetString("TextEditor_ContextFlyout_SelectAllButtonDisplayText") };
                    _selectAll.KeyboardAccelerators.Add(new KeyboardAccelerator()
                    {
                        Modifiers = VirtualKeyModifiers.Control,
                        Key = VirtualKey.A,
                        IsEnabled = false,
                    });
                    _selectAll.Click += (sender, args) =>
                    {
                        _textEditorCore.Document.Selection.SetRange(0, Int32.MaxValue);
                    };
                }
                return _selectAll;
            }
        }

        public MenuFlyoutItem RightToLeftReadingOrder
        {
            get
            {
                if (_rightToLeftReadingOrder != null) return _rightToLeftReadingOrder;

                _rightToLeftReadingOrder = new MenuFlyoutItem
                {
                    Text = _resourceLoader.GetString("TextEditor_ContextFlyout_RightToLeftReadingOrderButtonDisplayText"),
                    Icon = new FontIcon()
                    {
                        FontFamily = new FontFamily("Segoe MDL2 Assets"),
                        Glyph = "\uE73E"
                    }
                };
                _rightToLeftReadingOrder.Icon.Visibility = (_textEditorCore.FlowDirection == FlowDirection.RightToLeft) ? Visibility.Visible : Visibility.Collapsed;
                _rightToLeftReadingOrder.Click += (sender, args) =>
                {
                    var flowDirection = (_textEditorCore.FlowDirection == FlowDirection.LeftToRight)
                        ? FlowDirection.RightToLeft
                        : FlowDirection.LeftToRight;
                    _textEditorCore.SwitchTextFlowDirection(flowDirection);
                    _rightToLeftReadingOrder.Icon.Visibility = (_textEditorCore.FlowDirection == FlowDirection.RightToLeft) ? Visibility.Visible : Visibility.Collapsed;
                };
                return _rightToLeftReadingOrder;
            }
        }

        public MenuFlyoutItem WebSearch
        {
            get
            {
                if (_webSearch != null) return _webSearch;

                _webSearch = new MenuFlyoutItem
                {
                    Text = _resourceLoader.GetString("TextEditor_ContextFlyout_WebSearchButtonDisplayText"),
                    Icon = new FontIcon()
                    {
                        FontFamily = new FontFamily("Segoe MDL2 Assets"),
                        Glyph = "\uE721"
                    }
                };
                _webSearch.KeyboardAccelerators.Add(new KeyboardAccelerator()
                {
                    Modifiers = VirtualKeyModifiers.Control,
                    Key = VirtualKey.E,
                    IsEnabled = false,
                });
                _webSearch.Click += (sender, args) =>
                {
                    _textEditorCore.SearchInWeb();
                };
                return _webSearch;
            }
        }

        public MenuFlyoutItem Share
        {
            get
            {
                if (_share == null)
                {
                    _share = new MenuFlyoutItem { Icon = new SymbolIcon(Symbol.Share), Text = _resourceLoader.GetString("TextEditor_ContextFlyout_ShareButtonDisplayText") };
                    _share.Click += (sender, args) =>
                    {
                        Windows.ApplicationModel.DataTransfer.DataTransferManager.ShowShareUI();
                    };
                }
                return _share;
            }
        }

        public MenuFlyoutItem WordWrap
        {
            get
            {
                if (_wordWrap != null) return _wordWrap;

                _wordWrap = new MenuFlyoutItem
                {
                    Text = _resourceLoader.GetString("TextEditor_ContextFlyout_WordWrapButtonDisplayText"),
                    Icon = new FontIcon()
                    {
                        FontFamily = new FontFamily("Segoe MDL2 Assets"),
                        Glyph = "\uE73E"
                    }
                };
                _wordWrap.KeyboardAccelerators.Add(new KeyboardAccelerator()
                {
                    Modifiers = VirtualKeyModifiers.Menu,
                    Key = VirtualKey.Z,
                    IsEnabled = false,
                });
                _wordWrap.Icon.Visibility = _textEditorCore.TextWrapping == TextWrapping.Wrap ? Visibility.Visible : Visibility.Collapsed;
                _wordWrap.Click += (sender, args) =>
                {
                    _wordWrap.Icon.Visibility = _textEditorCore.TextWrapping == TextWrapping.Wrap ? Visibility.Visible : Visibility.Collapsed;
                    _textEditorCore.TextWrapping = _textEditorCore.TextWrapping == TextWrapping.Wrap ? TextWrapping.NoWrap : TextWrapping.Wrap;
                };
                return _wordWrap;
            }
        }

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
                    _textEditor.ShowHideContentPreview();
                };
                return _previewToggle;
            }
        }
    }
}