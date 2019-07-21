
namespace Notepads.Controls.TextEditor
{
    using Notepads.Commands;
    using Notepads.Services;
    using Notepads.Utilities;
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Windows.ApplicationModel.DataTransfer;
    using Windows.System;
    using Windows.UI.Core;
    using Windows.UI.Text;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;

    public class TextEditorCore : RichEditBox
    {
        private string[] _documentLinesCache;

        private readonly IKeyboardCommandHandler<KeyRoutedEventArgs> _keyboardCommandHandler;

        public event EventHandler<TextWrapping> TextWrappingChanged;

        public event EventHandler<double> FontSizeChanged;

        public new TextWrapping TextWrapping
        {
            get => base.TextWrapping;
            set
            {
                base.TextWrapping = value;
                TextWrappingChanged?.Invoke(this, value);
            }
        }

        public new double FontSize
        {
            get => base.FontSize;
            set
            {
                base.FontSize = value;
                FontSizeChanged?.Invoke(this, value);
            }
        }

        public TextEditorCore()
        {
            IsSpellCheckEnabled = false;
            TextWrapping = EditorSettingsService.EditorDefaultTextWrapping;
            FontFamily = new FontFamily(EditorSettingsService.EditorFontFamily);
            FontSize = EditorSettingsService.EditorFontSize;
            SelectionHighlightColor = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            SelectionHighlightColorWhenNotFocused = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            SelectionFlyout = null;
            HorizontalAlignment = HorizontalAlignment.Stretch;
            VerticalAlignment = VerticalAlignment.Stretch;
            HandwritingView.BorderThickness = new Thickness(0);
            CopyingToClipboard += (sender, args) => CopyPlainTextToWindowsClipboard(args);
            Paste += async (sender, args) => await PastePlainTextFromWindowsClipboard(args);
            TextChanging += OnTextChanging;

            SetDefaultTabStop(FontFamily, FontSize);
            PointerWheelChanged += OnPointerWheelChanged;

            EditorSettingsService.OnFontFamilyChanged += (sender, fontFamily) =>
            {
                FontFamily = new FontFamily(fontFamily);
                SetDefaultTabStop(FontFamily, FontSize);
            };
            EditorSettingsService.OnFontSizeChanged += (sender, fontSize) =>
            {
                FontSize = fontSize;
                SetDefaultTabStop(FontFamily, FontSize);
            };

            EditorSettingsService.OnDefaultTextWrappingChanged += (sender, textWrapping) => { TextWrapping = textWrapping; };
            ThemeSettingsService.OnAccentColorChanged += (sender, color) =>
            {
                SelectionHighlightColor = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
                SelectionHighlightColorWhenNotFocused = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            };

            // Init shortcuts
            _keyboardCommandHandler = GetKeyboardCommandHandler();
        }

        private KeyboardCommandHandler GetKeyboardCommandHandler()
        {
            return new KeyboardCommandHandler(new List<IKeyboardCommand<KeyRoutedEventArgs>>
            {
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.Z, (args) => Document.Undo()),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, true, VirtualKey.Z, (args) => Document.Redo()),
                new KeyboardShortcut<KeyRoutedEventArgs>(false, true, false, VirtualKey.Z, (args) => TextWrapping = TextWrapping == TextWrapping.Wrap ? TextWrapping.NoWrap : TextWrapping.Wrap),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.Add, (args) => IncreaseFontSize(2)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, (VirtualKey)187, (args) => IncreaseFontSize(2)), // (VirtualKey)187: =
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.Subtract, (args) => DecreaseFontSize(2)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, (VirtualKey)189, (args) => DecreaseFontSize(2)), // (VirtualKey)189: -
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.Number0, (args) => ResetFontSizeToDefault()),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.NumberPad0, (args) => ResetFontSizeToDefault()),
            });
        }

        public string GetText()
        {
            Document.GetText(TextGetOptions.None, out var text);
            // RichEditBox's Document.GetText() method by default append an extra '\r' at end of the text string
            // We need to trim it before proceeding
            return TrimRichEditBoxText(text);
        }

        //TODO This method I wrote is pathetic, need to find a way to implement it in a better way 
        public void GetCurrentLineColumn(out int lineIndex, out int columnIndex, out int selectedCount)
        {
            if (_documentLinesCache == null)
            {
                Document.GetText(TextGetOptions.None, out var text);
                _documentLinesCache = text.Split("\r");
            }

            var start = Document.Selection.StartPosition;
            var end = Document.Selection.EndPosition;

            lineIndex = 1;
            columnIndex = 1;
            selectedCount = 0;

            var length = 0;
            bool startLocated = false;
            for (int i = 0; i < _documentLinesCache.Length; i++)
            {
                var line = _documentLinesCache[i];

                if (line.Length + length >= start && !startLocated)
                {
                    lineIndex = i + 1;
                    columnIndex = start - length + 1;
                    startLocated = true;
                }

                if (line.Length + length >= end)
                {
                    if (i == lineIndex - 1)
                        selectedCount = end - start;
                    else
                        selectedCount = end - start + (i - lineIndex);
                    return;
                }

                length += line.Length + 1;
            }
        }

        public void CopyPlainTextToWindowsClipboard(TextControlCopyingToClipboardEventArgs args)
        {
            if (args != null)
            {
                args.Handled = true;
            }

            DataPackage dataPackage = new DataPackage { RequestedOperation = DataPackageOperation.Copy };
            dataPackage.SetText(Document.Selection.Text);
            Clipboard.SetContent(dataPackage);
        }

        public async Task PastePlainTextFromWindowsClipboard(TextControlPasteEventArgs args)
        {
            if (args != null)
            {
                args.Handled = true;
            }

            if (!Document.CanPaste()) return;

            var dataPackageView = Windows.ApplicationModel.DataTransfer.Clipboard.GetContent();
            if (!dataPackageView.Contains(StandardDataFormats.Text)) return;
            try
            {
                var text = await dataPackageView.GetTextAsync();
                Document.Selection.SetText(TextSetOptions.None, text);
                Document.Selection.StartPosition = Document.Selection.EndPosition;
            }
            catch (Exception)
            {
                // ignore
            }
        }

        private void SetDefaultTabStop(FontFamily font, double fontSize)
        {
            Document.DefaultTabStop = (float)FontUtility.GetTextSize(font, fontSize, "text").Width;
        }

        private void IncreaseFontSize(double delta)
        {
            SetDefaultTabStop(FontFamily, FontSize + delta);
            FontSize += delta;
        }

        private void DecreaseFontSize(double delta)
        {
            if (FontSize < delta + 2) return;
            SetDefaultTabStop(FontFamily, FontSize - delta);
            FontSize -= delta;
        }

        private void ResetFontSizeToDefault()
        {
            SetDefaultTabStop(FontFamily, EditorSettingsService.EditorFontSize);
            FontSize = EditorSettingsService.EditorFontSize;
        }

        private string TrimRichEditBoxText(string text)
        {
            // Trim end \r
            if (!string.IsNullOrEmpty(text) && text[text.Length - 1] == '\r')
            {
                text = text.Substring(0, text.Length - 1);
            }

            return text;
        }

        private void OnTextChanging(RichEditBox sender, RichEditBoxTextChangingEventArgs args)
        {
            if (args.IsContentChanging)
            {
                _documentLinesCache = null;
            }
        }

        private void OnPointerWheelChanged(object sender, PointerRoutedEventArgs e)
        {
            var ctrl = Window.Current.CoreWindow.GetKeyState(VirtualKey.Control);
            var alt = Window.Current.CoreWindow.GetKeyState(VirtualKey.Menu);
            var shift = Window.Current.CoreWindow.GetKeyState(VirtualKey.Shift);

            if (ctrl.HasFlag(CoreVirtualKeyStates.Down) &&
                !alt.HasFlag(CoreVirtualKeyStates.Down) &&
                !shift.HasFlag(CoreVirtualKeyStates.Down))
            {
                var mouseWheelDelta = e.GetCurrentPoint(this).Properties.MouseWheelDelta;
                if (mouseWheelDelta > 0)
                {
                    IncreaseFontSize(1);
                }
                else if (mouseWheelDelta < 0)
                {
                    DecreaseFontSize(1);
                }
            }
        }

        protected override void OnKeyDown(KeyRoutedEventArgs e)
        {
            var ctrl = Window.Current.CoreWindow.GetKeyState(VirtualKey.Control);
            var alt = Window.Current.CoreWindow.GetKeyState(VirtualKey.Menu);

            if (ctrl.HasFlag(CoreVirtualKeyStates.Down) && !alt.HasFlag(CoreVirtualKeyStates.Down))
            {
                // Disable RichEditBox default shortcuts (Bold, Underline, Italic)
                // https://docs.microsoft.com/en-us/windows/desktop/controls/about-rich-edit-controls
                if (e.Key == VirtualKey.B || e.Key == VirtualKey.I || e.Key == VirtualKey.U ||
                    e.Key == VirtualKey.Number1 || e.Key == VirtualKey.Number2 ||
                    e.Key == VirtualKey.Number3 || e.Key == VirtualKey.Number4 ||
                    e.Key == VirtualKey.Number5 || e.Key == VirtualKey.Number6 ||
                    e.Key == VirtualKey.Number7 || e.Key == VirtualKey.Number8 ||
                    e.Key == VirtualKey.Number9 || e.Key == VirtualKey.Tab)
                {
                    return;
                }
            }

            _keyboardCommandHandler.Handle(e);

            if (!e.Handled)
            {
                base.OnKeyDown(e);
            }
        }
    }
}
