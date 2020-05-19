namespace Notepads.Controls.TextEditor
{
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Notepads.Commands;
    using Notepads.Extensions;
    using Notepads.Services;
    using Notepads.Utilities;
    using Windows.ApplicationModel.DataTransfer;
    using Windows.Foundation;
    using Windows.System;
    using Windows.UI.Core;
    using Windows.UI.Text;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Controls.Primitives;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;

    [TemplatePart(Name = ContentElementName, Type = typeof(ScrollViewer))]
    [TemplatePart(Name = RootGridName, Type = typeof(Grid))]
    [TemplatePart(Name = LineNumberCanvasName, Type = typeof(Canvas))]
    [TemplatePart(Name = LineNumberGridName, Type = typeof(Grid))]
    [TemplatePart(Name = LineHighlighterAndIndicatorCanvasName, Type = typeof(Canvas))]
    [TemplatePart(Name = LineHighlighterName, Type = typeof(Border))]
    [TemplatePart(Name = LineIndicatorName, Type = typeof(Border))]
    public partial class TextEditorCore : RichEditBox
    {
        public event EventHandler<TextWrapping> TextWrappingChanged;
        public event EventHandler<double> FontSizeChanged;
        public event EventHandler<double> FontZoomFactorChanged;
        public event EventHandler<TextControlCopyingToClipboardEventArgs> CopySelectedTextToWindowsClipboardRequested;
        public event EventHandler<ScrollViewerViewChangingEventArgs> ScrollViewerViewChanging;

        private const char RichEditBoxDefaultLineEnding = '\r';

        private bool _isDocumentLinesCachePendingUpdate = true;
        private string[] _documentLinesCache; // internal copy of the active document text in array format
        private string _document = string.Empty; // internal copy of the active document text

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
        private const string RootGridName = "RootGrid";
        private Grid _rootGrid;
        private const string LineNumberCanvasName = "LineNumberCanvas";
        private Canvas _lineNumberCanvas;
        private const string LineNumberGridName = "LineNumberGrid";
        private Grid _lineNumberGrid;
        private const string ContentScrollViewerVerticalScrollBarName = "VerticalScrollBar";
        private ScrollBar _contentScrollViewerVerticalScrollBar;
        private const string LineHighlighterAndIndicatorCanvasName = "LineHighlighterAndIndicatorCanvas";
        private Canvas _lineHighlighterAndIndicatorCanvas;
        private const string LineHighlighterName = "LineHighlighter";
        private Border _lineHighlighter;
        private const string LineIndicatorName = "LineIndicator";
        private Border _lineIndicator;

        private TextWrapping _textWrapping = AppSettingsService.EditorDefaultTextWrapping;

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
        private double _fontSize = AppSettingsService.EditorFontSize;

        public new double FontSize
        {
            get => _fontSize;
            set
            {
                base.FontSize = value;
                _fontSize = value;
                SetDefaultTabStopAndLineSpacing(FontFamily, value);
                FontSizeChanged?.Invoke(this, value);

                var newZoomFactor = Math.Round((value * 100) / AppSettingsService.EditorFontSize);
                if (Math.Abs(newZoomFactor - _fontZoomFactor) >= 1)
                {
                    _fontZoomFactor = newZoomFactor;
                    FontZoomFactorChanged?.Invoke(this, newZoomFactor);
                }
            }
        }

        public TextEditorCore()
        {
            IsSpellCheckEnabled = AppSettingsService.IsHighlightMisspelledWordsEnabled;
            TextWrapping = AppSettingsService.EditorDefaultTextWrapping;
            FontFamily = new FontFamily(AppSettingsService.EditorFontFamily);
            FontSize = AppSettingsService.EditorFontSize;
            FontStyle = AppSettingsService.EditorFontStyle;
            FontWeight = AppSettingsService.EditorFontWeight;
            SelectionHighlightColor = new SolidColorBrush(ThemeSettingsService.AppAccentColor);
            SelectionHighlightColorWhenNotFocused = new SolidColorBrush(ThemeSettingsService.AppAccentColor);
            SelectionFlyout = null;
            HorizontalAlignment = HorizontalAlignment.Stretch;
            VerticalAlignment = VerticalAlignment.Stretch;
            DisplayLineNumbers = AppSettingsService.EditorDisplayLineNumbers;
            DisplayLineHighlighter = AppSettingsService.EditorDisplayLineHighlighter;
            HandwritingView.BorderThickness = new Thickness(0);

            CopyingToClipboard += OnCopyingToClipboard;
            Paste += OnPaste;
            TextChanging += OnTextChanging;
            TextChanged += OnTextChanged;
            SelectionChanging += OnSelectionChanging;

            SetDefaultTabStopAndLineSpacing(FontFamily, FontSize);
            PointerWheelChanged += OnPointerWheelChanged;
            LostFocus += OnLostFocus;
            Loaded += OnLoaded;

            SelectionChanged += OnSelectionChanged;
            TextWrappingChanged += OnTextWrappingChanged;
            SizeChanged += OnSizeChanged;
            FontSizeChanged += OnFontSizeChanged;

            // Init shortcuts
            _keyboardCommandHandler = GetKeyboardCommandHandler();
            _mouseCommandHandler = GetMouseCommandHandler();

            HookExternalEvents();

            Window.Current.CoreWindow.Activated += OnCoreWindowActivated;
        }

        protected override void OnApplyTemplate()
        {
            base.OnApplyTemplate();

            _rootGrid = GetTemplateChild(RootGridName) as Grid;

            _lineNumberGrid = GetTemplateChild(LineNumberGridName) as Grid;
            _lineNumberCanvas = GetTemplateChild(LineNumberCanvasName) as Canvas;

            _lineHighlighterAndIndicatorCanvas = GetTemplateChild(LineHighlighterAndIndicatorCanvasName) as Canvas;
            _lineHighlighter = GetTemplateChild(LineHighlighterName) as Border;
            _lineIndicator = GetTemplateChild(LineIndicatorName) as Border;

            _contentScrollViewer = GetTemplateChild(ContentElementName) as ScrollViewer;
            _shouldResetScrollViewerToLastKnownPositionAfterRegainingFocus = true;
            _contentScrollViewer.ViewChanging += OnContentScrollViewerViewChanging;
            _contentScrollViewer.ViewChanged += OnContentScrollViewerViewChanged;
            _contentScrollViewer.SizeChanged += OnContentScrollViewerSizeChanged;

            _contentScrollViewer.ApplyTemplate();
            var scrollViewerRoot = (FrameworkElement)VisualTreeHelper.GetChild(_contentScrollViewer, 0);
            _contentScrollViewerVerticalScrollBar = (ScrollBar)scrollViewerRoot.FindName(ContentScrollViewerVerticalScrollBarName);
            _contentScrollViewerVerticalScrollBar.ValueChanged += OnVerticalScrollBarValueChanged;

            _lineNumberGrid.SizeChanged += OnLineNumberGridSizeChanged;
            _rootGrid.SizeChanged += OnRootGridSizeChanged;

            Microsoft.Toolkit.Uwp.UI.Extensions.ScrollViewerExtensions.SetEnableMiddleClickScrolling(_contentScrollViewer, true);
        }

        // Unhook events and clear state
        public void Dispose()
        {
            CopyingToClipboard -= OnCopyingToClipboard;
            Paste -= OnPaste;
            TextChanging -= OnTextChanging;
            TextChanged -= OnTextChanged;
            SelectionChanging -= OnSelectionChanging;
            PointerWheelChanged -= OnPointerWheelChanged;
            LostFocus -= OnLostFocus;
            Loaded -= OnLoaded;

            if (_contentScrollViewer != null)
            {
                _contentScrollViewer.ViewChanging -= OnContentScrollViewerViewChanging;
                _contentScrollViewer.ViewChanged -= OnContentScrollViewerViewChanged;
                _contentScrollViewer.SizeChanged -= OnContentScrollViewerSizeChanged;
            }

            if (_contentScrollViewerVerticalScrollBar != null)
            {
                _contentScrollViewerVerticalScrollBar.ValueChanged -= OnVerticalScrollBarValueChanged;
            }

            if (_lineNumberGrid != null)
            {
                _lineNumberGrid.SizeChanged -= OnLineNumberGridSizeChanged;
            }

            if (_rootGrid != null)
            {
                _rootGrid.SizeChanged -= OnRootGridSizeChanged;
            }

            _lineNumberCanvas?.Children.Clear();
            _renderedLineNumberBlocks.Clear();
            _miniRequisiteIntegerTextRenderingWidthCache.Clear();

            SelectionChanged -= OnSelectionChanged;
            TextWrappingChanged -= OnTextWrappingChanged;
            SizeChanged -= OnSizeChanged;
            FontSizeChanged -= OnFontSizeChanged;

            UnhookExternalEvents();

            Window.Current.CoreWindow.Activated -= OnCoreWindowActivated;
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
                new KeyboardCommand<KeyRoutedEventArgs>(VirtualKey.Tab, (args) => AddIndentation(AppSettingsService.EditorDefaultTabIndents)),
                new KeyboardCommand<KeyRoutedEventArgs>(false, false, true, VirtualKey.Tab, (args) => RemoveIndentation(AppSettingsService.EditorDefaultTabIndents)),
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

        private void OnCoreWindowActivated(CoreWindow sender, WindowActivatedEventArgs args)
        {
            if (args.WindowActivationState == Windows.UI.Core.CoreWindowActivationState.CodeActivated ||
                args.WindowActivationState == Windows.UI.Core.CoreWindowActivationState.PointerActivated)
            {
                _shouldResetScrollViewerToLastKnownPositionAfterRegainingFocus = true;
            }
        }

        private void OnLoaded(object sender, RoutedEventArgs _)
        {
            _loaded = true;

            ResetRootGridClipping();

            if (_shouldResetScrollViewerToLastKnownPositionAfterRegainingFocus)
            {
                _shouldResetScrollViewerToLastKnownPositionAfterRegainingFocus = false;
                _contentScrollViewer.ChangeView(
                    _contentScrollViewerHorizontalOffsetLastKnownPosition,
                    _contentScrollViewerVerticalOffsetLastKnownPosition,
                    zoomFactor: null,
                    disableAnimation: true);
            }

            UpdateLineHighlighterAndIndicator();
            if (DisplayLineNumbers) ShowLineNumbers();
        }

        private void OnLostFocus(object sender, RoutedEventArgs _)
        {
            GetScrollViewerPosition(out _contentScrollViewerHorizontalOffsetLastKnownPosition, out _contentScrollViewerVerticalOffsetLastKnownPosition);
        }

        protected override void OnKeyDown(KeyRoutedEventArgs args)
        {
            var result = _keyboardCommandHandler.Handle(args);

            if (result.ShouldHandle)
            {
                args.Handled = true;
            }

            if (!result.ShouldSwallow)
            {
                base.OnKeyDown(args);
            }
        }

        private void OnPointerWheelChanged(object sender, PointerRoutedEventArgs args)
        {
            var result = _mouseCommandHandler.Handle(args);
            if (result.ShouldHandle)
            {
                args.Handled = true;
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
                Document.GetText(TextGetOptions.None, out var document);
                _document = TrimRichEditBoxText(document);
                _isDocumentLinesCachePendingUpdate = true;
            }
        }

        private void OnTextChanged(object sender, RoutedEventArgs _)
        {
            UpdateLineNumbersRendering();
        }

        private void OnSelectionChanging(RichEditBox sender, RichEditBoxSelectionChangingEventArgs args)
        {
            _textSelectionStartPosition = args.SelectionStart;
            _textSelectionEndPosition = args.SelectionStart + args.SelectionLength;
        }

        private void OnContentScrollViewerViewChanging(object sender, ScrollViewerViewChangingEventArgs args)
        {
            _contentScrollViewerHorizontalOffset = args.FinalView.HorizontalOffset;
            _contentScrollViewerVerticalOffset = args.FinalView.VerticalOffset;
            ScrollViewerViewChanging?.Invoke(sender, args);
        }

        private void OnContentScrollViewerViewChanged(object sender, ScrollViewerViewChangedEventArgs _)
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
            else
            {
                UpdateLineNumbersRendering();
            }
        }

        private void OnContentScrollViewerSizeChanged(object sender, SizeChangedEventArgs _)
        {
            UpdateLineNumbersRendering();
        }

        private void OnFontSizeChanged(object sender, double _)
        {
            UpdateLineHighlighterAndIndicator();
        }

        private void OnSizeChanged(object sender, SizeChangedEventArgs _)
        {
            UpdateLineHighlighterAndIndicator();
        }

        private void OnTextWrappingChanged(object sender, TextWrapping _)
        {
            UpdateLayout();
            UpdateLineHighlighterAndIndicator();
            UpdateLineNumbersRendering();
        }

        private void OnSelectionChanged(object sender, RoutedEventArgs _)
        {
            UpdateLineHighlighterAndIndicator();
        }

        private void OnVerticalScrollBarValueChanged(object sender, RangeBaseValueChangedEventArgs _)
        {
            // Make sure line number canvas is in sync with editor's ScrollViewer
            _contentScrollViewer.StartExpressionAnimation(_lineNumberCanvas, Axis.Y);

            // Make sure line highlighter and indicator canvas is in sync with editor's ScrollViewer
            _contentScrollViewer.StartExpressionAnimation(_lineHighlighterAndIndicatorCanvas, Axis.Y);
        }

        private void OnRootGridSizeChanged(object sender, SizeChangedEventArgs _)
        {
            ResetRootGridClipping();
        }

        private void ResetRootGridClipping()
        {
            if (!_loaded) return;

            _rootGrid.Clip = new RectangleGeometry
            {
                Rect = new Rect(
                    0,
                    0,
                    _rootGrid.ActualWidth,
                    Math.Clamp(_rootGrid.ActualHeight, .0f, Double.PositiveInfinity))
            };
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
            return _document;
        }

        public double GetSingleLineHeight()
        {
            Document.GetRange(0, 0).GetRect(PointOptions.ClientCoordinates, out var rect, out _);
            return rect.Height <= 0 ? 1.35 * FontSize : rect.Height;
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
            var lines = GetDocumentLinesCache();
            GetTextSelectionPosition(out var start, out var end);

            startLineIndex = 1;
            startColumnIndex = 1;
            endLineIndex = 1;
            endColumnIndex = 1;
            selectedCount = 0;
            lineCount = lines.Length - 1;

            var length = 0;
            bool startLocated = false;

            for (int i = 0; i < lineCount + 1; i++)
            {
                var line = lines[i];

                if (line.Length + length >= start && !startLocated)
                {
                    startLineIndex = i + 1;
                    startColumnIndex = start - length + 1;
                    startLocated = true;
                }

                if (line.Length + length >= end)
                {
                    if (i == startLineIndex - 1 || lineEnding != LineEnding.Crlf)
                    {
                        selectedCount = end - start;
                    }
                    else
                    {
                        selectedCount = end - start + (i - startLineIndex) + 1;
                    }

                    endLineIndex = i + 1;
                    endColumnIndex = end - length + 1;

                    // Reposition end position to previous line's end position if last selected char is RichEditBoxDefaultLineEnding ('\r')
                    if (endColumnIndex == 1 && end != start)
                    {
                        endLineIndex--;
                        endColumnIndex = lines[i - 1].Length + 1;
                    }

                    return;
                }

                length += line.Length + 1;
            }
        }

        private string[] GetDocumentLinesCache()
        {
            if (_isDocumentLinesCachePendingUpdate)
            {
                _documentLinesCache = (GetText() + RichEditBoxDefaultLineEnding).Split(RichEditBoxDefaultLineEnding);
                _isDocumentLinesCachePendingUpdate = false;
            }

            return _documentLinesCache;
        }

        /*public void GetLineColumnSelection(out int lineIndex, out int columnIndex, out int selectedCount)
        {
            GetTextSelectionPosition(out var start, out var end);

            var document = GetText();

            lineIndex = (document + RichEditBoxDefaultLineEnding).Substring(0, start).Length
                - document.Substring(0, start).Replace(RichEditBoxDefaultLineEnding.ToString(), string.Empty).Length
                + 1;
            columnIndex = start
                - (RichEditBoxDefaultLineEnding + document).LastIndexOf(RichEditBoxDefaultLineEnding, start)
                + 1;
            selectedCount = start != end && !string.IsNullOrEmpty(document)
                ? end - start + (document + RichEditBoxDefaultLineEnding).Substring(0, end).Length
                - (document + RichEditBoxDefaultLineEnding).Substring(0, end).Replace(RichEditBoxDefaultLineEnding.ToString(), string.Empty).Length
                : 0;
            if (end > document.Length) selectedCount -= 2;
        }*/

        public double GetFontZoomFactor()
        {
            return _fontZoomFactor;
        }

        public void SetFontZoomFactor(double fontZoomFactor)
        {
            var fontZoomFactorInt = Math.Round(fontZoomFactor);
            if (fontZoomFactorInt >= _minimumZoomFactor && fontZoomFactorInt <= _maximumZoomFactor)
                FontSize = (fontZoomFactorInt / 100) * AppSettingsService.EditorFontSize;
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
                //Document.Selection.CharacterFormat.TextScript = TextScript.Ansi;
                Document.Selection.StartPosition = Document.Selection.EndPosition;
                Document.EndUndoGroup();
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(TextEditorCore)}] Failed to paste plain text to Windows clipboard: {ex.Message}");
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
            if (string.IsNullOrEmpty(GetText()))
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
                _shouldResetScrollViewerToLastKnownPositionAfterRegainingFocus = false;
                Document.Selection.SetIndex(TextRangeUnit.Paragraph, line, false);
                return true;
            }
            catch
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
            var lines = GetDocumentLinesCache();
            var leadingSpacesAndTabs = lines[startLineIndex - 1].Substring(0, startColumnIndex - 1).LeadingSpacesAndTabs();
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
                _contentScrollViewer.ChangeView(null, _contentScrollViewer.VerticalOffset + (-1 * mouseWheelDelta), null, false);
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
            _contentScrollViewer.ChangeView(_contentScrollViewer.HorizontalOffset + (-1 * mouseWheelDelta), null, null, false);
        }

        private static string TrimRichEditBoxText(string text)
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

        private static void ShowEasterEgg()
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