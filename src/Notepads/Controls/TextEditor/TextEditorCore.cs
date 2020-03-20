namespace Notepads.Controls.TextEditor
{
    using System;
    using System.Collections.Generic;
    using System.Globalization;
    using System.Text;
    using System.Text.RegularExpressions;
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

        public event EventHandler<ScrollViewerViewChangedEventArgs> ScrollViewerOffsetChanged;

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

        private readonly double _minimumZoomFactor = 10;

        private readonly double _maximumZoomFactor = 500;

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

        private double _fontZoomFactor = 100;
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

                var newZoomFactor = Math.Round((value * 100) / EditorSettingsService.EditorFontSize);
                if (Math.Abs(newZoomFactor - _fontZoomFactor) >= 1)
                {
                    _fontZoomFactor = newZoomFactor;
                    FontZoomFactorChanged?.Invoke(this, newZoomFactor);
                }
            }
        }

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
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.Add, (args) => IncreaseFontSize(0.1)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, (VirtualKey)187, (args) => IncreaseFontSize(0.1)), // (VirtualKey)187: =
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.Subtract, (args) => DecreaseFontSize(0.1)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, (VirtualKey)189, (args) => DecreaseFontSize(0.1)), // (VirtualKey)189: -
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.Number0, (args) => ResetFontSizeToDefault()),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.NumberPad0, (args) => ResetFontSizeToDefault()),
                new KeyboardShortcut<KeyRoutedEventArgs>(VirtualKey.F5, (args) => InsertDataTimeString()),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.E, (args) => SearchInWeb()),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.D, (args) => DuplicateText()),
                new KeyboardShortcut<KeyRoutedEventArgs>(VirtualKey.Tab, (args) => AddIndentation()),
                new KeyboardShortcut<KeyRoutedEventArgs>(false, false, true, VirtualKey.Tab, (args) => RemoveIndentation()),
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
        public void GetLineColumnSelection(
            out int startLineIndex, 
            out int endLineIndex, 
            out int startColumnIndex, 
            out int endColumnIndex, 
            out int selectedCount,
            out int lineCount)
        {
            if (_isLineCachePendingUpdate)
            {
                _contentLinesCache = (_content + RichEditBoxDefaultLineEnding).Split(RichEditBoxDefaultLineEnding);
                _isLineCachePendingUpdate = false;
            }

            GetTextSelectionPosition(out var start, out var end);

            startLineIndex = 1;
            startColumnIndex = 1;
            endLineIndex = 1;
            endColumnIndex = 1;
            selectedCount = 0;
            lineCount = _contentLinesCache.Length - 1;

            var length = 0;
            bool startLocated = false;

            for (int i = 0; i < lineCount + 1; i++)
            {
                var line = _contentLinesCache[i];

                if (line.Length + length >= start && !startLocated)
                {
                    startLineIndex = i + 1;
                    startColumnIndex = start - length + 1;
                    startLocated = true;
                }

                if (line.Length + length >= end)
                {
                    if (i == startLineIndex - 1)
                        selectedCount = end - start;
                    else
                        selectedCount = end - start + (i - startLineIndex);
                    endLineIndex = i + 1;
                    endColumnIndex = end - length + 1;

                    // Reposition end position to previous line's end position if last selected char is RichEditBoxDefaultLineEnding ('\r')
                    if (endColumnIndex == 1 && end != start)
                    {
                        endLineIndex--;
                        endColumnIndex = _contentLinesCache[i - 1].Length + 1;
                    }

                    return;
                }

                length += line.Length + 1;
            }
        }

        /*public void GetLineColumnSelection(out int lineIndex, out int columnIndex, out int selectedCount)
        {
            GetTextSelectionPosition(out var start, out var end);

            lineIndex = (_content + RichEditBoxDefaultLineEnding).Substring(0, start).Length
                - _content.Substring(0, start).Replace(RichEditBoxDefaultLineEnding.ToString(), string.Empty).Length
                + 1;
            columnIndex = start
                - (RichEditBoxDefaultLineEnding + _content).LastIndexOf(RichEditBoxDefaultLineEnding, start)
                + 1;
            selectedCount = start != end && !string.IsNullOrEmpty(_content)
                ? end - start + (_content + RichEditBoxDefaultLineEnding).Substring(0, end).Length
                - (_content + RichEditBoxDefaultLineEnding).Substring(0, end).Replace(RichEditBoxDefaultLineEnding.ToString(), string.Empty).Length
                : 0;
            if (end > _content.Length) selectedCount -= 2;
        }*/

        public double GetFontZoomFactor()
        {
            return _fontZoomFactor;
        }

        public void SetFontZoomFactor(double fontZoomFactor)
        {
            var fontZoomFactorInt = Math.Round(fontZoomFactor);
            if (fontZoomFactorInt >= _minimumZoomFactor && fontZoomFactorInt <= _maximumZoomFactor)
                FontSize = (fontZoomFactorInt / 100) * EditorSettingsService.EditorFontSize;
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

        public bool FindNextAndReplace(string searchText, string replaceText, bool matchCase, bool matchWholeWord, bool useRegex, out string regexError)
        {
            if (FindNextAndSelect(searchText, matchCase, matchWholeWord, useRegex, out var error))
            {
                regexError = error;
                Document.Selection.SetText(TextSetOptions.None, replaceText);
                return true;
            }

            regexError = error;
            return false;
        }

        public bool FindAndReplaceAll(string searchText, string replaceText, bool matchCase, bool matchWholeWord, bool useRegex, out string regexError)
        {
            regexError = null;
            var found = false;
            var text = GetText();

            if (string.IsNullOrEmpty(searchText))
            {
                return false;
            }

            if (useRegex)
            {
                try
                {
                    Regex regex = new Regex(searchText, RegexOptions.Compiled | (matchCase ? RegexOptions.None : RegexOptions.IgnoreCase));
                    if(regex.IsMatch(text))
                    {
                        text = regex.Replace(text, replaceText);
                        found = true;
                    }
                }
                catch (Exception ex)
                {
                    regexError = ex.Message;
                    found = false;
                }
            }
            else
            {
                var pos = 0;
                var searchTextLength = searchText.Length;
                var replaceTextLength = replaceText.Length;

                StringComparison comparison = matchCase ? StringComparison.Ordinal : StringComparison.OrdinalIgnoreCase;

                pos = matchWholeWord ? IndexOfWholeWord(text, pos, searchText, comparison) : text.IndexOf(searchText, pos, comparison);

                while (pos != -1)
                {
                    found = true;
                    text = text.Remove(pos, searchTextLength).Insert(pos, replaceText);
                    pos += replaceTextLength;
                    pos = matchWholeWord ? IndexOfWholeWord(text, pos, searchText, comparison) : text.IndexOf(searchText, pos, comparison);
                }
            }

            if (found)
            {
                SetText(text);
                Document.Selection.StartPosition = int.MaxValue;
                Document.Selection.EndPosition = Document.Selection.StartPosition;
            }

            return found;
        }

        public bool FindNextAndSelect(string searchText, bool matchCase, bool matchWholeWord, bool useRegex, out string regexError, bool stopAtEof = true)
        {
            regexError = null;

            if (string.IsNullOrEmpty(searchText))
            {
                return false;
            }

            var text = GetText();

            if (Document.Selection.EndPosition > text.Length) Document.Selection.EndPosition = text.Length;

            if (useRegex)
            {
                try
                {
                    Regex regex = new Regex(searchText, RegexOptions.Compiled | (matchCase ? RegexOptions.None : RegexOptions.IgnoreCase));

                    var match = regex.Match(text, Document.Selection.EndPosition);

                    if (match.Success)
                    {
                        Document.Selection.StartPosition = match.Index;
                        Document.Selection.EndPosition = match.Index + match.Length;
                    }
                    else
                    {
                        if (!stopAtEof)
                        {
                            match = regex.Match(text, 0);

                            if (match.Success)
                            {
                                Document.Selection.StartPosition = match.Index;
                                Document.Selection.EndPosition = match.Index + match.Length;
                            }
                        }
                    }

                    if (!match.Success)
                    {
                        Document.Selection.StartPosition = Document.Selection.EndPosition;
                        return false;
                    }

                    return true;
                }
                catch (Exception ex)
                {
                    regexError = ex.Message;
                    return false;
                }
            }

            StringComparison comparison = matchCase ? StringComparison.Ordinal : StringComparison.OrdinalIgnoreCase;

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
                    e.Key == VirtualKey.Number9 || e.Key == VirtualKey.Tab ||
                    (shift.HasFlag(CoreVirtualKeyStates.Down) && e.Key == (VirtualKey)187))
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
            format.SetLineSpacing(LineSpacingRule.Exactly, (float)fontSize);
            Document.SetDefaultParagraphFormat(format);
        }

        private void IncreaseFontSize(double delta)
        {
            if (_fontZoomFactor<_maximumZoomFactor)
            {
                if (_fontZoomFactor % 10 > 0)
                {
                    SetFontZoomFactor(Math.Ceiling(_fontZoomFactor / 10) * 10);
                }
                else
                {
                    FontSize += delta * EditorSettingsService.EditorFontSize;
                }
            }
        }

        private void DecreaseFontSize(double delta)
        {
            if (_fontZoomFactor > _minimumZoomFactor)
            {
                if (_fontZoomFactor % 10 > 0)
                {
                    SetFontZoomFactor(Math.Floor(_fontZoomFactor / 10) * 10);
                }
                else
                {
                    FontSize -= delta * EditorSettingsService.EditorFontSize;
                }
            }
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

            ScrollViewerOffsetChanged?.Invoke(this, e);
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
                    IncreaseFontSize(0.1);
                }
                else if (mouseWheelDelta < 0)
                {
                    DecreaseFontSize(0.1);
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
                GetLineColumnSelection(out int startLineIndex, out int endLineIndex, out int startColumnIndex, out int endColumnIndex, out int selectedCount, out int lineCount);
                GetTextSelectionPosition(out var start, out var end);

                if (end == start)
                {
                    // Duplicate Line
                    var line = _contentLinesCache[startLineIndex - 1];
                    var column = Document.Selection.EndPosition + line.Length + 1;

                    if (startColumnIndex == 1)
                        Document.Selection.EndPosition += 1;

                    Document.Selection.EndOf(TextRangeUnit.Paragraph, false);

                    if (startLineIndex < lineCount)
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

                        if (startLineIndex < lineCount && end < _content.Length)
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
            try
            {
                Document.Selection.SetIndex(TextRangeUnit.Paragraph, line, false);
                return true;
            }
            catch (Exception)
            {
                return false;
            }
        }

        public async void SearchInWeb()
        {
            try
            {
                var selectedText = Document.Selection.Text.Trim();

                // The maximum length of a URL in the address bar is 2048 characters
                // Let's take 2000 here to make sure we are not exceeding the limit
                // Otherwise we will see "Invalid URI: The uri string is too long" exception
                var searchString = selectedText.Length <= 2000 ? selectedText : selectedText.Substring(0, 2000);

                if (Uri.TryCreate(searchString, UriKind.Absolute, out var webUrl) && (webUrl.Scheme == Uri.UriSchemeHttp || webUrl.Scheme == Uri.UriSchemeHttps))
                {
                    await Launcher.LaunchUriAsync(webUrl);
                    return;
                }
                var searchUri = new Uri(string.Format(SearchEngineUtility.GetSearchUrlBySearchEngine(EditorSettingsService.EditorDefaultSearchEngine), string.Join("+", searchString.Split(null))));
                await Launcher.LaunchUriAsync(searchUri);
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"Failed to open search link: {ex.Message}");
            }
        }

        private void AddIndentation()
        {
            GetTextSelectionPosition(out var start, out var end);
            GetLineColumnSelection(out var startLine, out var endLine, out var startColumn, out var endColumn, out _, out _);

            var startLineInitialIndex = start - startColumn + 1;
            var endLineFinalIndex = end - endColumn + _contentLinesCache[endLine - 1].Length + 1;
            if (endLineFinalIndex > _content.Length) endLineFinalIndex = _content.Length;

            var tabStr = EditorSettingsService.EditorDefaultTabIndents == -1
                ? "\t"
                : new string(' ', EditorSettingsService.EditorDefaultTabIndents);

            // Handle single line selection scenario where part of the line is selected
            if (startLine == endLine)
            {
                Document.Selection.TypeText(tabStr);
                Document.Selection.StartPosition = Document.Selection.EndPosition;
                return;
            }

            var indentAmount = EditorSettingsService.EditorDefaultTabIndents == -1 ? 1 : EditorSettingsService.EditorDefaultTabIndents;
            start += indentAmount;

            var indentedStringBuilder = new StringBuilder();
            for (var i = startLine - 1; i < endLine; i++)
            {
                indentedStringBuilder.Append(string.Concat(tabStr, _contentLinesCache[i], i < endLine - 1 ? RichEditBoxDefaultLineEnding.ToString() : string.Empty));
                end += indentAmount;
            }

            if (string.Equals(_content.Substring(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex),
                indentedStringBuilder.ToString())) return;

            if (Document.Selection.Text.EndsWith(RichEditBoxDefaultLineEnding) && endLineFinalIndex < _content.Length)
            {
                indentedStringBuilder.Append(RichEditBoxDefaultLineEnding);
                if (string.Equals(_content.Substring(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex),
                    indentedStringBuilder.ToString())) return;
            }

            Document.SetText(TextSetOptions.None,
                _content.Remove(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex)
                .Insert(startLineInitialIndex, indentedStringBuilder.ToString()));

            Document.Selection.SetRange(start, end);
        }

        private void RemoveIndentation()
        {
            GetTextSelectionPosition(out var start, out var end);
            GetLineColumnSelection(out var startLine, out var endLine, out var startColumn, out var endColumn, out _, out _);

            var startLineInitialIndex = start - startColumn + 1;
            var endLineFinalIndex = end - endColumn + _contentLinesCache[endLine - 1].Length + 1;
            if (endLineFinalIndex > _content.Length) endLineFinalIndex = _content.Length;

            if (startLineInitialIndex == endLineFinalIndex) return;

            var indentedStringBuilder = new StringBuilder();
            for (var i = startLine - 1; i < endLine; i++)
            {
                var lineTrailingString = i < endLine - 1 ? RichEditBoxDefaultLineEnding.ToString() : string.Empty;
                if (_contentLinesCache[i].StartsWith('\t'))
                {
                    indentedStringBuilder.Append(_contentLinesCache[i].Remove(0, 1) + lineTrailingString);
                    end--;
                }
                else
                {
                    var spaceCount = 0;
                    var indentAmount = EditorSettingsService.EditorDefaultTabIndents == -1 ? 4 : EditorSettingsService.EditorDefaultTabIndents;

                    for (var charIndex = 0; charIndex < _contentLinesCache[i].Length && _contentLinesCache[i][charIndex] == ' '; charIndex++)
                    {
                        spaceCount++;
                    }

                    if (spaceCount == 0)
                    {
                        indentedStringBuilder.Append(_contentLinesCache[i] + lineTrailingString);
                        continue;
                    }

                    var insufficientSpace = spaceCount % indentAmount;

                    if (insufficientSpace > 0)
                    {
                        indentedStringBuilder.Append(_contentLinesCache[i].Remove(0, insufficientSpace) + lineTrailingString);
                        end -= insufficientSpace;
                    }
                    else
                    {
                        indentedStringBuilder.Append(_contentLinesCache[i].Remove(0, indentAmount) + lineTrailingString);
                        end -= indentAmount;
                    }
                }

                if (i == startLine - 1)
                {
                    if (startLine == endLine)
                        start -= _contentLinesCache[i].Length - indentedStringBuilder.Length;
                    else
                        start -= _contentLinesCache[i].Length - indentedStringBuilder.Length + 1;

                    if (start < startLineInitialIndex)
                    {
                        if (end == start) end = startLineInitialIndex;
                        start = startLineInitialIndex;
                    }
                }
            }

            if (string.Equals(_content.Substring(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex),
                indentedStringBuilder.ToString())) return;

            if (Document.Selection.Text.EndsWith(RichEditBoxDefaultLineEnding) && endLineFinalIndex < _content.Length)
            {
                indentedStringBuilder.Append(RichEditBoxDefaultLineEnding);
                if (string.Equals(_content.Substring(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex),
                    indentedStringBuilder.ToString())) return;
            }

            Document.SetText(TextSetOptions.None,
                _content.Remove(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex)
                .Insert(startLineInitialIndex, indentedStringBuilder.ToString()));
            Document.Selection.SetRange(start, end);
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