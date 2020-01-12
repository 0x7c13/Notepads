namespace Notepads.Controls.TextEditor
{
    using System;
    using System.Collections.Generic;
    using System.Globalization;
    using System.Threading.Tasks;
    using Microsoft.AppCenter.Analytics;
    using Notepads.Commands;
    using Notepads.Services;
    using Notepads.Utilities;
    using Windows.ApplicationModel.DataTransfer;
    using Windows.System;
    using Windows.UI.Core;
    using Windows.UI.Text;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;

    [TemplatePart(Name = ContentElementName, Type = typeof(ScrollViewer))]
    public class TextEditorCore : RichEditBox
    {
        public event EventHandler<TextWrapping> TextWrappingChanged;

        public event EventHandler<double> FontSizeChanged;

        public event EventHandler<double> FontZoomFactorChanged;

        public event EventHandler<TextControlCopyingToClipboardEventArgs> CopySelectedTextToWindowsClipboardRequested;

        private const char RichEditBoxDefaultLineEnding = '\r';

        private string[] _contentLinesCache;

        private bool _isLineCachePendingUpdate = true;

        private string _content = string.Empty;

        private readonly IKeyboardCommandHandler<KeyRoutedEventArgs> _keyboardCommandHandler;

        private int _textSelectionStartPosition = 0;

        private int _textSelectionEndPosition = 0;

        private double _contentScrollViewerHorizontalOffset = 0;

        private double _contentScrollViewerVerticalOffset = 0;

        private const string ContentElementName = "ContentElement";

        private ScrollViewer _contentScrollViewer;

        private TextWrapping _textWrapping = EditorSettingsService.EditorDefaultTextWrapping;

        public new TextWrapping TextWrapping
        {
            get => _textWrapping;
            set
            {
                base.TextWrapping = value;
                _textWrapping = value;
                TextWrappingChanged?.Invoke(this, value);
            }
        }

        private double _fontZoomFactor = 1.0f;
        private double _fontSize = EditorSettingsService.EditorFontSize;

        public new double FontSize
        {
            get => _fontSize;
            set
            {
                base.FontSize = value;
                _fontSize = value;
                SetDefaultTabStopAndLineSpacing(FontFamily, value);
                FontSizeChanged?.Invoke(this, value);

                var newZoomFactor = value / EditorSettingsService.EditorFontSize;
                if (Math.Abs(newZoomFactor - _fontZoomFactor) > 0.00001)
                {
                    _fontZoomFactor = newZoomFactor;
                    FontZoomFactorChanged?.Invoke(this, newZoomFactor);
                }
            }
        }

        public double FontZoomFactor => _fontZoomFactor;

        public TextEditorCore()
        {
            IsSpellCheckEnabled = EditorSettingsService.IsHighlightMisspelledWordsEnabled;
            TextWrapping = EditorSettingsService.EditorDefaultTextWrapping;
            FontFamily = new FontFamily(EditorSettingsService.EditorFontFamily);
            FontSize = EditorSettingsService.EditorFontSize;
            SelectionHighlightColor = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            SelectionHighlightColorWhenNotFocused = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            SelectionFlyout = null;
            HorizontalAlignment = HorizontalAlignment.Stretch;
            VerticalAlignment = VerticalAlignment.Stretch;
            HandwritingView.BorderThickness = new Thickness(0);

            CopyingToClipboard += TextEditorCore_CopySelectedTextToWindowsClipboard;
            Paste += TextEditorCore_Paste;
            TextChanging += OnTextChanging;
            SelectionChanged += OnSelectionChanged;

            SetDefaultTabStopAndLineSpacing(FontFamily, FontSize);
            PointerWheelChanged += OnPointerWheelChanged;

            EditorSettingsService.OnFontFamilyChanged += EditorSettingsService_OnFontFamilyChanged;
            EditorSettingsService.OnFontSizeChanged += EditorSettingsService_OnFontSizeChanged;
            EditorSettingsService.OnDefaultTextWrappingChanged += EditorSettingsService_OnDefaultTextWrappingChanged;
            EditorSettingsService.OnHighlightMisspelledWordsChanged += EditorSettingsService_OnHighlightMisspelledWordsChanged;

            ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;

            // Init shortcuts
            _keyboardCommandHandler = GetKeyboardCommandHandler();
        }

        // Unhook events and clear state
        public void Dispose()
        {
            CopyingToClipboard -= TextEditorCore_CopySelectedTextToWindowsClipboard;
            Paste -= TextEditorCore_Paste;
            TextChanging -= OnTextChanging;
            SelectionChanged -= OnSelectionChanged;

            PointerWheelChanged -= OnPointerWheelChanged;

            if (_contentScrollViewer != null)
            {
                _contentScrollViewer.ViewChanged -= OnContentScrollViewerViewChanged;
            }

            EditorSettingsService.OnFontFamilyChanged -= EditorSettingsService_OnFontFamilyChanged;
            EditorSettingsService.OnFontSizeChanged -= EditorSettingsService_OnFontSizeChanged;
            EditorSettingsService.OnDefaultTextWrappingChanged -= EditorSettingsService_OnDefaultTextWrappingChanged;
            EditorSettingsService.OnHighlightMisspelledWordsChanged -= EditorSettingsService_OnHighlightMisspelledWordsChanged;

            ThemeSettingsService.OnAccentColorChanged -= ThemeSettingsService_OnAccentColorChanged;

            _contentLinesCache = null;
        }

        private void EditorSettingsService_OnFontFamilyChanged(object sender, string fontFamily)
        {
            FontFamily = new FontFamily(fontFamily);
            SetDefaultTabStopAndLineSpacing(FontFamily, FontSize);
        }

        private void EditorSettingsService_OnFontSizeChanged(object sender, int fontSize)
        {
            FontSize = fontSize;
        }

        private void EditorSettingsService_OnDefaultTextWrappingChanged(object sender, TextWrapping textWrapping)
        {
            TextWrapping = textWrapping;
        }

        private void EditorSettingsService_OnHighlightMisspelledWordsChanged(object sender, bool isSpellCheckEnabled)
        {
            IsSpellCheckEnabled = isSpellCheckEnabled;
        }

        private void ThemeSettingsService_OnAccentColorChanged(object sender, Windows.UI.Color color)
        {
            SelectionHighlightColor = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            SelectionHighlightColorWhenNotFocused = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
        }

        private KeyboardCommandHandler GetKeyboardCommandHandler()
        {
            return new KeyboardCommandHandler(new List<IKeyboardCommand<KeyRoutedEventArgs>>
            {
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.Z, (args) => Undo()),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, true, VirtualKey.Z, (args) => Redo()),
                new KeyboardShortcut<KeyRoutedEventArgs>(false, true, false, VirtualKey.Z, (args) => TextWrapping = TextWrapping == TextWrapping.Wrap ? TextWrapping.NoWrap : TextWrapping.Wrap),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.Add, (args) => IncreaseFontSize(2)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, (VirtualKey)187, (args) => IncreaseFontSize(2)), // (VirtualKey)187: =
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.Subtract, (args) => DecreaseFontSize(2)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, (VirtualKey)189, (args) => DecreaseFontSize(2)), // (VirtualKey)189: -
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.Number0, (args) => ResetFontSizeToDefault()),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.NumberPad0, (args) => ResetFontSizeToDefault()),
                new KeyboardShortcut<KeyRoutedEventArgs>(VirtualKey.F5, (args) => InsertDataTimeString()),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.D, (args) => DuplicateText()),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, true, true, VirtualKey.D, (args) => ShowEasterEgg(), requiredHits: 10)
            });
        }

        protected override void OnApplyTemplate()
        {
            base.OnApplyTemplate();
            _contentScrollViewer = GetTemplateChild(ContentElementName) as ScrollViewer;
            _contentScrollViewer.BringIntoViewOnFocusChange = false;
            _contentScrollViewer.ChangeView(
                _contentScrollViewerHorizontalOffset,
                _contentScrollViewerVerticalOffset,
                zoomFactor: null,
                disableAnimation: true);
            _contentScrollViewer.ViewChanged += OnContentScrollViewerViewChanged;
        }

        public void Undo()
        {
            if (Document.CanUndo() && IsEnabled)
            {
                Document.Undo();
            }
        }

        public void Redo()
        {
            if (Document.CanRedo() && IsEnabled)
            {
                Document.Redo();
            }
        }

        public void SetText(string text)
        {
            Document.SetText(TextSetOptions.None, text);
        }

        // Thread safe
        public string GetText()
        {
            return _content;
        }

        // Thread safe
        public void GetScrollViewerPosition(out double horizontalOffset, out double verticalOffset)
        {
            horizontalOffset = _contentScrollViewerHorizontalOffset;
            verticalOffset = _contentScrollViewerVerticalOffset;
        }

        // Thread safe
        public void GetTextSelectionPosition(out int startPosition, out int endPosition)
        {
            startPosition = _textSelectionStartPosition;
            endPosition = _textSelectionEndPosition;
        }

        public void SetTextSelectionPosition(int selectionStartPosition, int selectionEndPosition)
        {
            _textSelectionStartPosition = selectionStartPosition;
            _textSelectionEndPosition = selectionEndPosition;
            Document.Selection.StartPosition = selectionStartPosition;
            Document.Selection.EndPosition = selectionEndPosition;
        }

        public void SetScrollViewerPosition(double horizontalOffset, double verticalOffset)
        {
            _contentScrollViewerHorizontalOffset = horizontalOffset;
            _contentScrollViewerVerticalOffset = verticalOffset;
            _contentScrollViewer?.ChangeView(horizontalOffset, verticalOffset, zoomFactor: null, disableAnimation: true);
        }

        //TODO This method I wrote is pathetic, need to find a way to implement it in a better way 
        public void GetCurrentLineColumn(out int lineIndex, out int columnIndex, out int selectedCount)
        {
            if (_isLineCachePendingUpdate)
            {
                _contentLinesCache = (_content + RichEditBoxDefaultLineEnding).Split(RichEditBoxDefaultLineEnding);
                _isLineCachePendingUpdate = false;
            }

            GetTextSelectionPosition(out var start, out var end);

            lineIndex = 1;
            columnIndex = 1;
            selectedCount = 0;

            var length = 0;
            bool startLocated = false;
            for (int i = 0; i < _contentLinesCache.Length; i++)
            {
                var line = _contentLinesCache[i];

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

        public async Task PastePlainTextFromWindowsClipboard(TextControlPasteEventArgs args)
        {
            if (args != null)
            {
                args.Handled = true;
            }

            if (!Document.CanPaste()) return;

            try
            {
                var dataPackageView = Windows.ApplicationModel.DataTransfer.Clipboard.GetContent();
                if (!dataPackageView.Contains(StandardDataFormats.Text)) return;
                var text = await dataPackageView.GetTextAsync();
                Document.Selection.SetText(TextSetOptions.None, text);
                Document.Selection.StartPosition = Document.Selection.EndPosition;
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"Failed to paste plain text to Windows clipboard: {ex.Message}");
            }
        }

        public void ClearUndoQueue()
        {
            // Clear UndoQueue by setting its limit to 0 and set it back
            var undoLimit = Document.UndoLimit;

            // Check to prevent the undo limit stuck on zero
            // because it returns 0 even if the undo limit isn't set yet
            if (undoLimit != 0)
            {
                Document.UndoLimit = 0;
                Document.UndoLimit = undoLimit;
            }
        }

        public bool FindNextAndReplace(string searchText, string replaceText, bool matchCase, bool matchWholeWord)
        {
            if (FindNextAndSelect(searchText, matchCase, matchWholeWord))
            {
                Document.Selection.SetText(TextSetOptions.None, replaceText);
                return true;
            }

            return false;
        }

        public bool FindAndReplaceAll(string searchText, string replaceText, bool matchCase, bool matchWholeWord)
        {
            var found = false;

            var pos = 0;
            var searchTextLength = searchText.Length;
            var replaceTextLength = replaceText.Length;

            var text = GetText();

            StringComparison comparison = matchCase ? StringComparison.Ordinal : StringComparison.OrdinalIgnoreCase;

            pos = matchWholeWord ? IndexOfWholeWord(text, pos, searchText, comparison) : text.IndexOf(searchText, pos, comparison);

            while (pos != -1)
            {
                found = true;
                text = text.Remove(pos, searchTextLength).Insert(pos, replaceText);
                pos += replaceTextLength;
                pos = matchWholeWord ? IndexOfWholeWord(text, pos, searchText, comparison) : text.IndexOf(searchText, pos, comparison);
            }

            if (found)
            {
                SetText(text);
                Document.Selection.StartPosition = int.MaxValue;
                Document.Selection.EndPosition = Document.Selection.StartPosition;
            }

            return found;
        }

        public bool FindNextAndSelect(string searchText, bool matchCase, bool matchWholeWord, bool stopAtEof = true)
        {
            if (string.IsNullOrEmpty(searchText))
            {
                return false;
            }

            var text = GetText();

            StringComparison comparison = matchCase ? StringComparison.Ordinal : StringComparison.OrdinalIgnoreCase;

            if (Document.Selection.EndPosition > text.Length) Document.Selection.EndPosition = text.Length;

            var index = matchWholeWord ? IndexOfWholeWord(text, Document.Selection.EndPosition, searchText, comparison) : text.IndexOf(searchText, Document.Selection.EndPosition, comparison);

            if (index != -1)
            {
                Document.Selection.StartPosition = index;
                Document.Selection.EndPosition = index + searchText.Length;
            }
            else
            {
                if (!stopAtEof)
                {
                    index = matchWholeWord ? IndexOfWholeWord(text, 0, searchText, comparison) : text.IndexOf(searchText, 0, comparison);

                    if (index != -1)
                    {
                        Document.Selection.StartPosition = index;
                        Document.Selection.EndPosition = index + searchText.Length;
                    }
                }
            }

            if (index == -1)
            {
                Document.Selection.StartPosition = Document.Selection.EndPosition;
                return false;
            }

            return true;
        }

        protected override void OnKeyDown(KeyRoutedEventArgs e)
        {
            var ctrl = Window.Current.CoreWindow.GetKeyState(VirtualKey.Control);
            var alt = Window.Current.CoreWindow.GetKeyState(VirtualKey.Menu);
            var shift = Window.Current.CoreWindow.GetKeyState(VirtualKey.Shift);

            // Disable RichEditBox default shortcuts (Bold, Underline, Italic)
            // https://docs.microsoft.com/en-us/windows/desktop/controls/about-rich-edit-controls
            if (ctrl.HasFlag(CoreVirtualKeyStates.Down) && !alt.HasFlag(CoreVirtualKeyStates.Down))
            {
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

            // By default, RichEditBox insert '\v' when user hit "Shift + Enter"
            // This should be converted to '\r' to match same behaviour as single "Enter"
            if (shift.HasFlag(CoreVirtualKeyStates.Down) && e.Key == VirtualKey.Enter)
            {
                Document.Selection.SetText(TextSetOptions.None, RichEditBoxDefaultLineEnding.ToString());
                Document.Selection.StartPosition = Document.Selection.EndPosition;
                return;
            }

            _keyboardCommandHandler.Handle(e);

            if (!e.Handled)
            {
                base.OnKeyDown(e);
            }
        }

        private async void TextEditorCore_Paste(object sender, TextControlPasteEventArgs args)
        {
            await PastePlainTextFromWindowsClipboard(args);
        }

        private void TextEditorCore_CopySelectedTextToWindowsClipboard(RichEditBox sender, TextControlCopyingToClipboardEventArgs args)
        {
            CopySelectedTextToWindowsClipboardRequested?.Invoke(sender, args);
        }

        private void SetDefaultTabStopAndLineSpacing(FontFamily font, double fontSize)
        {
            Document.DefaultTabStop = (float)FontUtility.GetTextSize(font, fontSize, "text").Width;
            var format = Document.GetDefaultParagraphFormat();
            format.SetLineSpacing(LineSpacingRule.AtLeast, (float)fontSize);
            Document.SetDefaultParagraphFormat(format);
        }

        private void IncreaseFontSize(double delta)
        {
            FontSize += delta;
        }

        private void DecreaseFontSize(double delta)
        {
            if (FontSize < delta + 2) return;
            FontSize -= delta;
        }

        private void ResetFontSizeToDefault()
        {
            FontSize = EditorSettingsService.EditorFontSize;
        }

        private string TrimRichEditBoxText(string text)
        {
            // Trim end \r
            if (!string.IsNullOrEmpty(text) && text[text.Length - 1] == RichEditBoxDefaultLineEnding)
            {
                text = text.Substring(0, text.Length - 1);
            }

            return text;
        }

        private void OnTextChanging(RichEditBox sender, RichEditBoxTextChangingEventArgs args)
        {
            if (args.IsContentChanging)
            {
                Document.GetText(TextGetOptions.None, out _content);
                _content = TrimRichEditBoxText(_content);
                _isLineCachePendingUpdate = true;
            }
        }

        private void OnSelectionChanged(object sender, RoutedEventArgs e)
        {
            _textSelectionStartPosition = Document.Selection.StartPosition;
            _textSelectionEndPosition = Document.Selection.EndPosition;
        }

        private void OnContentScrollViewerViewChanged(object sender, ScrollViewerViewChangedEventArgs e)
        {
            _contentScrollViewerHorizontalOffset = _contentScrollViewer.HorizontalOffset;
            _contentScrollViewerVerticalOffset = _contentScrollViewer.VerticalOffset;
        }

        private void OnPointerWheelChanged(object sender, PointerRoutedEventArgs e)
        {
            var ctrl = Window.Current.CoreWindow.GetKeyState(VirtualKey.Control);
            var alt = Window.Current.CoreWindow.GetKeyState(VirtualKey.Menu);
            var shift = Window.Current.CoreWindow.GetKeyState(VirtualKey.Shift);

            // Ctrl + Wheel -> zooming
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

            // Enabling scrolling during text selection
            if (!ctrl.HasFlag(CoreVirtualKeyStates.Down) &&
                !alt.HasFlag(CoreVirtualKeyStates.Down) &&
                !shift.HasFlag(CoreVirtualKeyStates.Down))
            {
                if (Document.Selection.Type == SelectionType.Normal ||
                    Document.Selection.Type == SelectionType.InlineShape ||
                    Document.Selection.Type == SelectionType.Shape)
                {
                    var mouseWheelDelta = e.GetCurrentPoint(this).Properties.MouseWheelDelta;
                    _contentScrollViewer.ChangeView(_contentScrollViewer.HorizontalOffset,
                            _contentScrollViewer.VerticalOffset + (-1 * mouseWheelDelta), null, true);
                }
            }

            // Ctrl + Shift + Wheel -> horizontal scrolling
            if (ctrl.HasFlag(CoreVirtualKeyStates.Down) &&
                !alt.HasFlag(CoreVirtualKeyStates.Down) &&
                shift.HasFlag(CoreVirtualKeyStates.Down))
            {
                var mouseWheelDelta = e.GetCurrentPoint(this).Properties.MouseWheelDelta;
                _contentScrollViewer.ChangeView(_contentScrollViewer.HorizontalOffset + (-1 * mouseWheelDelta),
                    _contentScrollViewer.VerticalOffset, null, true);
            }
        }

        private static int IndexOfWholeWord(string target, int startIndex, string value, StringComparison comparison)
        {
            int pos = startIndex;

            while (pos < target.Length && (pos = target.IndexOf(value, pos, comparison)) != -1)
            {
                bool startBoundary = true;
                if (pos > 0)
                    startBoundary = !char.IsLetterOrDigit(target[pos - 1]);

                bool endBoundary = true;
                if (pos + value.Length < target.Length)
                    endBoundary = !char.IsLetterOrDigit(target[pos + value.Length]);

                if (startBoundary && endBoundary)
                    return pos;

                pos++;
            }
            return -1;
        }

        private void InsertDataTimeString()
        {
            var dateStr = DateTime.Now.ToString(CultureInfo.CurrentCulture);
            Document.Selection.SetText(TextSetOptions.None, dateStr);
            Document.Selection.StartPosition = Document.Selection.EndPosition;
        }

        private void DuplicateText()
        {
            try
            {
                GetCurrentLineColumn(out int lineIndex, out int columnIndex, out int selectedCount);
                GetTextSelectionPosition(out var start, out var end);

                if (end == start)
                {
                    // Duplicate Line                
                    var line = _contentLinesCache[lineIndex - 1];
                    var column = Document.Selection.EndPosition + line.Length + 1;
                
                    if (columnIndex == 1)
                        Document.Selection.EndPosition += 1;

                    Document.Selection.EndOf(TextRangeUnit.Paragraph, false);

                    if (lineIndex < (_contentLinesCache.Length - 1))
                        Document.Selection.EndPosition -= 1;

                    Document.Selection.SetText(TextSetOptions.None, RichEditBoxDefaultLineEnding + line);
                    Document.Selection.StartPosition = Document.Selection.EndPosition = column;
                }
                else
                {
                    // Duplicate selection
                    var textRange = Document.GetRange(start, end);
                    textRange.GetText(TextGetOptions.None, out string text);

                    if (text.EndsWith(RichEditBoxDefaultLineEnding))
                    {
                        Document.Selection.EndOf(TextRangeUnit.Line, false);

                        if (lineIndex < (_contentLinesCache.Length - 1))
                            Document.Selection.StartPosition = Document.Selection.EndPosition - 1;
                    }
                    else
                    {              
                        Document.Selection.StartPosition = Document.Selection.EndPosition;
                    }

                    Document.Selection.SetText(TextSetOptions.None, text);
                } 
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[TextEditorCore] Failed to duplicate text: {ex}");
                Analytics.TrackEvent("TextEditorCore_FailedToDuplicateText", new Dictionary<string, string> {{ "Exception", ex.ToString() }});
            }
        }

        public bool GoTo(int line)
        {
            if (_isLineCachePendingUpdate)
            {
                _contentLinesCache = (_content + RichEditBoxDefaultLineEnding).Split(RichEditBoxDefaultLineEnding);
                _isLineCachePendingUpdate = false;
            }

            if (line > 0 && line < _contentLinesCache.Length)
            {
                Document.Selection.SetIndex(TextRangeUnit.Paragraph, line, false);
                return true;
            }
            else
            {
                return false;
            }
        }

        private void ShowEasterEgg()
        {
            //_contentScrollViewer.Background = new ImageBrush
            //{
            //    ImageSource = new BitmapImage(new Uri(BaseUri, "/Assets/EasterEgg.jpg")),
            //    AlignmentX = AlignmentX.Center,
            //    AlignmentY = AlignmentY.Center,
            //    Stretch = Stretch.Uniform
            //};
        }
    }
}