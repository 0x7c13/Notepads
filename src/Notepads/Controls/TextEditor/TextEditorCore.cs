namespace Notepads.Controls.TextEditor
{
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Notepads.Commands;
    using Notepads.Services;
    using Notepads.Utilities;
    using Windows.ApplicationModel.DataTransfer;
    using Windows.System;
    using Windows.UI;
    using Windows.UI.Core;
    using Windows.UI.Text;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;

    [TemplatePart(Name = ContentElementName, Type = typeof(ScrollViewer))]
    public partial class TextEditorCore : RichEditBox
    {
        public event EventHandler<TextWrapping> TextWrappingChanged;
        public event EventHandler<double> FontSizeChanged;
        public event EventHandler<double> FontZoomFactorChanged;
        public event EventHandler<TextControlCopyingToClipboardEventArgs> CopySelectedTextToWindowsClipboardRequested;
        public event EventHandler<ScrollViewerViewChangingEventArgs> ScrollViewerViewChanging;

        private const char RichEditBoxDefaultLineEnding = '\r';

        private bool _isLineCachePendingUpdate = true;
        private string[] _contentLinesCache;
        private string _content = string.Empty;

        private readonly ICommandHandler<KeyRoutedEventArgs> _keyboardCommandHandler;
        private readonly ICommandHandler<PointerRoutedEventArgs> _mouseCommandHandler;

        private int _textSelectionStartPosition = 0;
        private int _textSelectionEndPosition = 0;

        private double _contentScrollViewerHorizontalOffset = 0;
        private double _contentScrollViewerVerticalOffset = 0;
        private double _contentScrollViewerHorizontalOffsetLastKnownPosition = 0;
        private double _contentScrollViewerVerticalOffsetLastKnownPosition = 0;

        private bool _shouldResetScrollViewerToLastKnownPositionAfterRegainingFocus = false;

        private bool _loaded = false;

        private readonly double _minimumZoomFactor = 10;
        private readonly double _maximumZoomFactor = 500;

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

            CopyingToClipboard += OnCopyingToClipboard;
            Paste += OnPaste;
            TextChanging += OnTextChanging;
            SelectionChanging += OnSelectionChanging;

            SetDefaultTabStopAndLineSpacing(FontFamily, FontSize);
            PointerWheelChanged += OnPointerWheelChanged;
            LostFocus += OnLostFocus;
            Loaded += OnLoaded;

            EditorSettingsService.OnFontFamilyChanged += EditorSettingsService_OnFontFamilyChanged;
            EditorSettingsService.OnFontSizeChanged += EditorSettingsService_OnFontSizeChanged;
            EditorSettingsService.OnDefaultTextWrappingChanged += EditorSettingsService_OnDefaultTextWrappingChanged;
            EditorSettingsService.OnHighlightMisspelledWordsChanged += EditorSettingsService_OnHighlightMisspelledWordsChanged;

            ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;

            // Init shortcuts
            _keyboardCommandHandler = GetKeyboardCommandHandler();
            _mouseCommandHandler = GetMouseCommandHandler();

            Window.Current.CoreWindow.Activated += OnCoreWindowActivated;
        }

        protected override void OnApplyTemplate()
        {
            base.OnApplyTemplate();
            _contentScrollViewer = GetTemplateChild(ContentElementName) as ScrollViewer;
            _shouldResetScrollViewerToLastKnownPositionAfterRegainingFocus = true;
            _contentScrollViewer.ViewChanging += OnContentScrollViewerViewChanging;
            _contentScrollViewer.ViewChanged += OnContentScrollViewerViewChanged;
            _contentScrollViewer.ChangeView(
                _contentScrollViewerHorizontalOffsetLastKnownPosition,
                _contentScrollViewerVerticalOffsetLastKnownPosition,
                zoomFactor: null,
                disableAnimation: true);
        }

        // Unhook events and clear state
        public void Dispose()
        {
            CopyingToClipboard -= OnCopyingToClipboard;
            Paste -= OnPaste;
            TextChanging -= OnTextChanging;
            SelectionChanging -= OnSelectionChanging;
            PointerWheelChanged -= OnPointerWheelChanged;
            LostFocus -= OnLostFocus;
            Loaded -= OnLoaded;

            if (_contentScrollViewer != null)
            {
                _contentScrollViewer.ViewChanging -= OnContentScrollViewerViewChanging;
                _contentScrollViewer.ViewChanged -= OnContentScrollViewerViewChanged;
            }

            EditorSettingsService.OnFontFamilyChanged -= EditorSettingsService_OnFontFamilyChanged;
            EditorSettingsService.OnFontSizeChanged -= EditorSettingsService_OnFontSizeChanged;
            EditorSettingsService.OnDefaultTextWrappingChanged -= EditorSettingsService_OnDefaultTextWrappingChanged;
            EditorSettingsService.OnHighlightMisspelledWordsChanged -= EditorSettingsService_OnHighlightMisspelledWordsChanged;

            ThemeSettingsService.OnAccentColorChanged -= ThemeSettingsService_OnAccentColorChanged;

            Window.Current.CoreWindow.Activated -= OnCoreWindowActivated;

            _contentLinesCache = null;
        }

        private KeyboardCommandHandler GetKeyboardCommandHandler()
        {
            var swallowedKeys = new List<VirtualKey>()
            {
                VirtualKey.B, VirtualKey.I, VirtualKey.U, VirtualKey.Tab,
                VirtualKey.Number1, VirtualKey.Number2, VirtualKey.Number3,
                VirtualKey.Number4, VirtualKey.Number5, VirtualKey.Number6,
                VirtualKey.Number7, VirtualKey.Number8, VirtualKey.Number9,
                VirtualKey.F3,
            };

            return new KeyboardCommandHandler(new List<IKeyboardCommand<KeyRoutedEventArgs>>
            {
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.Z, (args) => Undo()),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, true, VirtualKey.Z, (args) => Redo()),
                new KeyboardCommand<KeyRoutedEventArgs>(false, true, false, VirtualKey.Z, (args) => TextWrapping = TextWrapping == TextWrapping.Wrap ? TextWrapping.NoWrap : TextWrapping.Wrap),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.Add, (args) => IncreaseFontSize(0.1)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, (VirtualKey)187, (args) => IncreaseFontSize(0.1)), // (VirtualKey)187: =
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.Subtract, (args) => DecreaseFontSize(0.1)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, (VirtualKey)189, (args) => DecreaseFontSize(0.1)), // (VirtualKey)189: -
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.Number0, (args) => ResetFontSizeToDefault()),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.NumberPad0, (args) => ResetFontSizeToDefault()),
                new KeyboardCommand<KeyRoutedEventArgs>(VirtualKey.F5, (args) => InsertDateTimeString()),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.E, (args) => SearchInWeb()),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.D, (args) => DuplicateText()),
                new KeyboardCommand<KeyRoutedEventArgs>(VirtualKey.Tab, (args) => AddIndentation(EditorSettingsService.EditorDefaultTabIndents)),
                new KeyboardCommand<KeyRoutedEventArgs>(false, false, true, VirtualKey.Tab, (args) => RemoveIndentation(EditorSettingsService.EditorDefaultTabIndents)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, true, true, VirtualKey.D, (args) => ShowEasterEgg(), requiredHits: 10),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.L, (args) => SwitchTextFlowDirection(FlowDirection.LeftToRight)),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.R, (args) => SwitchTextFlowDirection(FlowDirection.RightToLeft)),
                // By default, RichEditBox insert '\v' when user hit "Shift + Enter"
                // This should be converted to '\r' to match same behaviour as single "Enter"
                new KeyboardCommand<KeyRoutedEventArgs>(false, false, true, VirtualKey.Enter, (args) => EnterWithAutoIndentation()),
                new KeyboardCommand<KeyRoutedEventArgs>(VirtualKey.Enter, (args) => EnterWithAutoIndentation()),
                // Disable RichEditBox default shortcuts (Bold, Underline, Italic)
                // https://docs.microsoft.com/en-us/windows/desktop/controls/about-rich-edit-controls
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, swallowedKeys, null, shouldHandle: false, shouldSwallow: true),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, true, swallowedKeys, null, shouldHandle: false, shouldSwallow: true),
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, true, (VirtualKey)187, null, shouldHandle: false, shouldSwallow: true), // (VirtualKey)187: =
                new KeyboardCommand<KeyRoutedEventArgs>(true, false, true, VirtualKey.L, null, shouldHandle: false, shouldSwallow: true),
                new KeyboardCommand<KeyRoutedEventArgs>(false, false, true, VirtualKey.F3, null, shouldHandle: false, shouldSwallow: true),
            });
        }

        private ICommandHandler<PointerRoutedEventArgs> GetMouseCommandHandler()
        {
            return new MouseCommandHandler(new List<IMouseCommand<PointerRoutedEventArgs>>()
            {
                new MouseCommand<PointerRoutedEventArgs>(true, false, true, false, false, false, ChangeHorizontalScrollingBasedOnMouseInput),
                new MouseCommand<PointerRoutedEventArgs>(false, false, true, false, false, false, ChangeHorizontalScrollingBasedOnMouseInput),
                new MouseCommand<PointerRoutedEventArgs>(false, true, false, ChangeHorizontalScrollingBasedOnMouseInput),
                new MouseCommand<PointerRoutedEventArgs>(true, false, false, false, false, false, ChangeZoomingBasedOnMouseInput),
                new MouseCommand<PointerRoutedEventArgs>(true, false, false, OnPointerLeftButtonDown),
            }, this);
        }

        private async void EditorSettingsService_OnFontFamilyChanged(object sender, string fontFamily)
        {
            await ThreadUtility.CallOnUIThreadAsync(Dispatcher, () =>
            {
                FontFamily = new FontFamily(fontFamily);
                SetDefaultTabStopAndLineSpacing(FontFamily, FontSize);
            });
        }

        private async void EditorSettingsService_OnFontSizeChanged(object sender, int fontSize)
        {
            await ThreadUtility.CallOnUIThreadAsync(Dispatcher, () =>
            {
                FontSize = fontSize;
            });
        }

        private async void EditorSettingsService_OnDefaultTextWrappingChanged(object sender, TextWrapping textWrapping)
        {
            await ThreadUtility.CallOnUIThreadAsync(Dispatcher, () =>
            {
                TextWrapping = textWrapping;
            });
        }

        private async void EditorSettingsService_OnHighlightMisspelledWordsChanged(object sender, bool isSpellCheckEnabled)
        {
            await ThreadUtility.CallOnUIThreadAsync(Dispatcher, () =>
            {
                IsSpellCheckEnabled = isSpellCheckEnabled;
            });
        }

        private async void ThemeSettingsService_OnAccentColorChanged(object sender, Color color)
        {
            await ThreadUtility.CallOnUIThreadAsync(Dispatcher, () =>
            {
                SelectionHighlightColor = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
                SelectionHighlightColorWhenNotFocused = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            });
        }

        private void OnCoreWindowActivated(CoreWindow sender, WindowActivatedEventArgs args)
        {
            if (args.WindowActivationState == Windows.UI.Core.CoreWindowActivationState.CodeActivated ||
                args.WindowActivationState == Windows.UI.Core.CoreWindowActivationState.PointerActivated)
            {
                _shouldResetScrollViewerToLastKnownPositionAfterRegainingFocus = true;
            }
        }

        private void OnLoaded(object sender, RoutedEventArgs e)
        {
            _loaded = true;
            if (_shouldResetScrollViewerToLastKnownPositionAfterRegainingFocus)
            {
                _shouldResetScrollViewerToLastKnownPositionAfterRegainingFocus = false;
                _contentScrollViewer.ChangeView(
                    _contentScrollViewerHorizontalOffsetLastKnownPosition,
                    _contentScrollViewerVerticalOffsetLastKnownPosition,
                    zoomFactor: null,
                    disableAnimation: true);
            }
        }

        private void OnLostFocus(object sender, RoutedEventArgs e)
        {
            GetScrollViewerPosition(out _contentScrollViewerHorizontalOffsetLastKnownPosition, out _contentScrollViewerVerticalOffsetLastKnownPosition);
        }

        protected override void OnKeyDown(KeyRoutedEventArgs e)
        {
            var result = _keyboardCommandHandler.Handle(e);

            if (result.ShouldHandle)
            {
                e.Handled = true;
            }

            if (!result.ShouldSwallow)
            {
                base.OnKeyDown(e);
            }
        }

        private void OnPointerWheelChanged(object sender, PointerRoutedEventArgs e)
        {
            var result = _mouseCommandHandler.Handle(e);
            if (result.ShouldHandle)
            {
                e.Handled = true;
            }
        }

        private async void OnPaste(object sender, TextControlPasteEventArgs args)
        {
            await PastePlainTextFromWindowsClipboard(args);
        }

        private void OnCopyingToClipboard(RichEditBox sender, TextControlCopyingToClipboardEventArgs args)
        {
            CopySelectedTextToWindowsClipboardRequested?.Invoke(sender, args);
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

        private void OnSelectionChanging(RichEditBox sender, RichEditBoxSelectionChangingEventArgs args)
        {
            _textSelectionStartPosition = args.SelectionStart;
            _textSelectionEndPosition = args.SelectionStart + args.SelectionLength;
        }

        private void OnContentScrollViewerViewChanging(object sender, ScrollViewerViewChangingEventArgs e)
        {
            _contentScrollViewerHorizontalOffset = e.FinalView.HorizontalOffset;
            _contentScrollViewerVerticalOffset = e.FinalView.VerticalOffset;
            ScrollViewerViewChanging?.Invoke(sender, e);
        }

        private void OnContentScrollViewerViewChanged(object sender, ScrollViewerViewChangedEventArgs e)
        {
            if (_shouldResetScrollViewerToLastKnownPositionAfterRegainingFocus)
            {
                _shouldResetScrollViewerToLastKnownPositionAfterRegainingFocus = false;
                _contentScrollViewer.ChangeView(
                    _contentScrollViewerHorizontalOffsetLastKnownPosition,
                    _contentScrollViewerVerticalOffsetLastKnownPosition,
                    zoomFactor: null,
                    disableAnimation: true);
            }
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

        /// <summary>
        /// Thread safe way of getting the text in the active story (document)
        /// </summary>
        public string GetText()
        {
            return _content;
        }

        /// <summary>
        /// Thread safe way of getting the current ScrollViewer position
        /// </summary>
        public void GetScrollViewerPosition(out double horizontalOffset, out double verticalOffset)
        {
            if (_loaded)
            {
                horizontalOffset = _contentScrollViewerHorizontalOffset;
                verticalOffset = _contentScrollViewerVerticalOffset;
            }
            else // If current TextEditorCore never loaded, we should use last known position
            {
                horizontalOffset = _contentScrollViewerHorizontalOffsetLastKnownPosition;
                verticalOffset = _contentScrollViewerVerticalOffsetLastKnownPosition;
            }
        }

        /// <summary>
        /// Thread safe way of getting the current document selection position
        /// </summary>
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

        public void SetScrollViewerInitPosition(double horizontalOffset, double verticalOffset)
        {
            _contentScrollViewerHorizontalOffsetLastKnownPosition = horizontalOffset;
            _contentScrollViewerVerticalOffsetLastKnownPosition = verticalOffset;
        }

        /// <summary>
        /// Returns 1-based indexing values
        /// </summary>
        public void GetLineColumnSelection(
            out int startLineIndex,
            out int endLineIndex,
            out int startColumnIndex,
            out int endColumnIndex,
            out int selectedCount,
            out int lineCount,
            LineEnding lineEnding = LineEnding.Crlf)
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
                    if (i == startLineIndex - 1 || lineEnding != LineEnding.Crlf)
                        selectedCount = end - start;
                    else
                        selectedCount = end - start + (i - startLineIndex) + 1;
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
                Document.BeginUndoGroup();
                Document.Selection.SetText(TextSetOptions.None, text);
                Document.Selection.CharacterFormat.TextScript = TextScript.Ansi;
                Document.Selection.StartPosition = Document.Selection.EndPosition;
                Document.EndUndoGroup();
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

        public void SwitchTextFlowDirection(FlowDirection direction)
        {
            if (string.IsNullOrEmpty(_content))
            {
                // If content is empty, switching text flow direction might not work
                // Let's not do anything here
                return;
            }

            FlowDirection = direction;
            TextReadingOrder = TextReadingOrder.UseFlowDirection;

            UpdateLayout();
            SetDefaultTabStopAndLineSpacing(FontFamily, FontSize);
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

        public void ResetFocusAndScrollToPreviousPosition()
        {
            _shouldResetScrollViewerToLastKnownPositionAfterRegainingFocus = true;
            base.Focus(FocusState.Programmatic);
        }

        private void SetDefaultTabStopAndLineSpacing(FontFamily font, double fontSize)
        {
            Document.DefaultTabStop = (float)FontUtility.GetTextSize(font, fontSize, "text").Width;
            var format = Document.GetDefaultParagraphFormat();
            format.SetLineSpacing(LineSpacingRule.Exactly, (float)fontSize);
            Document.SetDefaultParagraphFormat(format);
        }

        private void EnterWithAutoIndentation()
        {
            // Automatically indent on new lines based on current line's leading spaces/tabs
            GetLineColumnSelection(out var startLineIndex, out _, out var startColumnIndex, out _, out _, out _);
            var leadingSpacesAndTabs = StringUtility.GetLeadingSpacesAndTabs(_contentLinesCache[startLineIndex - 1].Substring(0, startColumnIndex - 1));
            Document.Selection.SetText(TextSetOptions.None, RichEditBoxDefaultLineEnding + leadingSpacesAndTabs);
            Document.Selection.StartPosition = Document.Selection.EndPosition;
        }

        private void OnPointerLeftButtonDown(PointerRoutedEventArgs args)
        {
            if (Document.Selection.Type == SelectionType.Normal ||
                Document.Selection.Type == SelectionType.InlineShape ||
                Document.Selection.Type == SelectionType.Shape)
            {
                var mouseWheelDelta = args.GetCurrentPoint(this).Properties.MouseWheelDelta;
                _contentScrollViewer.ChangeView(null, _contentScrollViewer.VerticalOffset + (-1 * mouseWheelDelta), null, true);
            }
        }

        // Ctrl + Wheel -> zooming
        private void ChangeZoomingBasedOnMouseInput(PointerRoutedEventArgs args)
        {
            var mouseWheelDelta = args.GetCurrentPoint(this).Properties.MouseWheelDelta;
            if (mouseWheelDelta > 0)
            {
                IncreaseFontSize(0.1);
            }
            else if (mouseWheelDelta < 0)
            {
                DecreaseFontSize(0.1);
            }
        }

        // Ctrl + Shift + Wheel -> horizontal scrolling
        private void ChangeHorizontalScrollingBasedOnMouseInput(PointerRoutedEventArgs args)
        {
            var mouseWheelDelta = args.GetCurrentPoint(this).Properties.MouseWheelDelta;
            _contentScrollViewer.ChangeView(_contentScrollViewer.HorizontalOffset + (-1 * mouseWheelDelta), null, null, true);
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

        private bool IsSelectionRectInView(Windows.Foundation.Rect rect, double horizontalOffset, double verticalOffset)
        {
            var isSelectionStartPositionInView = false;
            var isSelectionEndPositionInView = false;

            if (verticalOffset <= rect.Y && rect.Y <= verticalOffset + _contentScrollViewer.ViewportHeight &&
                horizontalOffset <= rect.X && rect.X <= horizontalOffset + _contentScrollViewer.ViewportWidth)
            {
                isSelectionStartPositionInView = true;
            }

            if (verticalOffset <= rect.Y + rect.Height && rect.Y + rect.Height <= verticalOffset + _contentScrollViewer.ViewportHeight &&
                horizontalOffset <= rect.X + rect.Width && rect.X + rect.Width <= horizontalOffset + _contentScrollViewer.ViewportWidth)
            {
                isSelectionEndPositionInView = true;
            }

            return isSelectionStartPositionInView && isSelectionEndPositionInView;
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