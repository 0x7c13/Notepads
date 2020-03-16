namespace Notepads.Controls.TextEditor
{
    using System;
    using System.Collections.Generic;
    using System.Text;
    using System.Threading;
    using System.Threading.Tasks;
    using Microsoft.AppCenter.Analytics;
    using Microsoft.Toolkit.Uwp.UI.Controls;
    using Notepads.Commands;
    using Notepads.Controls.FindAndReplace;
    using Notepads.Controls.GoTo;
    using Notepads.Extensions;
    using Notepads.Models;
    using Notepads.Services;
    using Notepads.Utilities;
    using Windows.ApplicationModel.Core;
    using Windows.ApplicationModel.DataTransfer;
    using Windows.ApplicationModel.Resources;
    using Windows.Storage;
    using Windows.System;
    using Windows.UI.Core;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;

    public enum TextEditorMode
    {
        Editing = 0,
        DiffPreview
    }

    public enum FileModificationState
    {
        Untouched,
        Modified,
        RenamedMovedOrDeleted
    }

    public sealed partial class TextEditor : UserControl, ITextEditor
    {
        public Guid Id { get; set; }

        public INotepadsExtensionProvider ExtensionProvider;

        public new event RoutedEventHandler Loaded;

        public new event RoutedEventHandler Unloaded;

        public new event KeyEventHandler KeyDown;

        public event EventHandler ModeChanged;

        public event EventHandler ModificationStateChanged;

        public event EventHandler FileModificationStateChanged;

        public event EventHandler LineEndingChanged;

        public event EventHandler EncodingChanged;

        public event EventHandler TextChanging;

        public event EventHandler ChangeReverted;

        public event EventHandler SelectionChanged;

        public event EventHandler FontZoomFactorChanged;

        public event EventHandler FileSaved;

        public event EventHandler FileReloaded;

        private const char RichEditBoxDefaultLineEnding = '\r';

        public string FileNamePlaceholder { get; set; } = string.Empty;

        public FileType FileType { get; private set; }

        public TextFile LastSavedSnapshot { get; private set; }

        public LineEnding? RequestedLineEnding { get; private set; }

        public Encoding RequestedEncoding { get; private set; }

        public string EditingFileName { get; private set; }

        public string EditingFilePath { get; private set; }

        public StorageFile EditingFile
        {
            get => _editingFile;
            private set
            {
                if (value == null)
                {
                    EditingFileName = null;
                    EditingFilePath = null;
                    FileType = FileType.TextFile;
                }
                else
                {
                    EditingFileName = value.Name;
                    EditingFilePath = value.Path;
                    FileType = FileTypeUtility.GetFileTypeByFileName(value.Name);
                }
                _editingFile = value;
            }
        }

        public bool IsModified
        {
            get => _isModified;
            private set
            {
                if (_isModified != value)
                {
                    _isModified = value;
                    ModificationStateChanged?.Invoke(this, EventArgs.Empty);
                }
            }
        }

        public FileModificationState FileModificationState
        {
            get => _fileModificationState;
            private set
            {
                if (_fileModificationState != value)
                {
                    _fileModificationState = value;
                    FileModificationStateChanged?.Invoke(this, EventArgs.Empty);
                }
            }
        }

        private bool _loaded;

        private bool _isModified;

        private FileModificationState _fileModificationState;

        private bool _isContentPreviewPanelOpened;

        private StorageFile _editingFile;

        private readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

        private CancellationTokenSource _fileStatusCheckerCancellationTokenSource;

        private readonly int _fileStatusCheckerPollingRateInSec = 6;

        private readonly double _fileStatusCheckerDelayInSec = 0.2;

        private readonly SemaphoreSlim _fileStatusSemaphoreSlim = new SemaphoreSlim(1, 1);

        private TextEditorMode _mode = TextEditorMode.Editing;

        private readonly IKeyboardCommandHandler<KeyRoutedEventArgs> _keyboardCommandHandler;

        private IContentPreviewExtension _contentPreviewExtension;

        public TextEditorMode Mode
        {
            get => _mode;
            private set
            {
                if (_mode != value)
                {
                    _mode = value;
                    ModeChanged?.Invoke(this, EventArgs.Empty);
                }
            }
        }

        public TextEditor()
        {
            InitializeComponent();

            TextEditorCore.TextChanging += TextEditorCore_OnTextChanging;
            TextEditorCore.SelectionChanged += TextEditorCore_OnSelectionChanged;
            TextEditorCore.KeyDown += TextEditorCore_OnKeyDown;
            TextEditorCore.CopySelectedTextToWindowsClipboardRequested += TextEditorCore_CopySelectedTextToWindowsClipboardRequested;
            TextEditorCore.ContextFlyout = new TextEditorContextFlyout(this, TextEditorCore);

            // Init shortcuts
            _keyboardCommandHandler = GetKeyboardCommandHandler();

            ThemeSettingsService.OnThemeChanged += ThemeSettingsService_OnThemeChanged;

            base.Loaded += TextEditor_Loaded;
            base.Unloaded += TextEditor_Unloaded;
            base.KeyDown += TextEditor_KeyDown;

            EditorSettingsService.OnDefaultLineHighlighterViewStateChanged += LineHighlighter_OnViewStateChanged;
            TextEditorCore.SelectionChanged += LineHighlighter_OnSelectionChanged;
            TextEditorCore.FontSizeChanged += LineHighlighter_OnFontSizeChanged;
            TextEditorCore.TextWrappingChanged += LineHighlighter_OnTextWrappingChanged;
            TextEditorCore.ScrollViewerOffsetChanged += LineHighlighter_OnScrolled;
            base.SizeChanged += LineHighlighter_WindowSizeChanged;

            TextEditorCore.FontZoomFactorChanged += TextEditorCore_OnFontZoomFactorChanged;
        }

        private void TextEditor_KeyDown(object sender, KeyRoutedEventArgs e)
        {
            KeyDown?.Invoke(this, e);
        }

        // Unhook events and clear state
        public void Dispose()
        {
            StopCheckingFileStatus();

            TextEditorCore.TextChanging -= TextEditorCore_OnTextChanging;
            TextEditorCore.SelectionChanged -= TextEditorCore_OnSelectionChanged;
            TextEditorCore.KeyDown -= TextEditorCore_OnKeyDown;
            TextEditorCore.CopySelectedTextToWindowsClipboardRequested -= TextEditorCore_CopySelectedTextToWindowsClipboardRequested;

            if (TextEditorCore.ContextFlyout is TextEditorContextFlyout contextFlyout)
            {
                contextFlyout.Dispose();
            }

            ThemeSettingsService.OnThemeChanged -= ThemeSettingsService_OnThemeChanged;

            Unloaded?.Invoke(this, new RoutedEventArgs());

            base.Loaded -= TextEditor_Loaded;
            base.Unloaded -= TextEditor_Unloaded;
            base.KeyDown -= TextEditor_KeyDown;

            EditorSettingsService.OnDefaultLineHighlighterViewStateChanged -= LineHighlighter_OnViewStateChanged;
            TextEditorCore.SelectionChanged -= LineHighlighter_OnSelectionChanged;
            TextEditorCore.FontSizeChanged -= LineHighlighter_OnFontSizeChanged;
            TextEditorCore.TextWrappingChanged -= LineHighlighter_OnTextWrappingChanged;
            TextEditorCore.ScrollViewerOffsetChanged -= LineHighlighter_OnScrolled;
            base.SizeChanged -= LineHighlighter_WindowSizeChanged;

            TextEditorCore.FontZoomFactorChanged -= TextEditorCore_OnFontZoomFactorChanged;

            _contentPreviewExtension?.Dispose();

            if (SplitPanel != null)
            {
                SplitPanel.KeyDown -= SplitPanel_OnKeyDown;
                UnloadObject(SplitPanel);
            }

            if (SideBySideDiffViewer != null)
            {
                SideBySideDiffViewer.OnCloseEvent -= SideBySideDiffViewer_OnCloseEvent;
                SideBySideDiffViewer.Dispose();
                UnloadObject(SideBySideDiffViewer);
            }

            if (FindAndReplacePlaceholder != null && FindAndReplacePlaceholder.Content is FindAndReplaceControl findAndReplaceControl)
            {
                findAndReplaceControl.Dispose();
                UnloadObject(FindAndReplacePlaceholder);
            }

            if (GoToPlaceholder != null && GoToPlaceholder.Content is GoToControl goToControl)
            {
                goToControl.Dispose();
                UnloadObject(GoToPlaceholder);
            }

            if (GridSplitter != null)
            {
                UnloadObject(GridSplitter);
            }

            TextEditorCore.Dispose();
        }

        private void ThemeSettingsService_OnThemeChanged(object sender, ElementTheme theme)
        {
            if (Mode == TextEditorMode.DiffPreview)
            {
                SideBySideDiffViewer.RenderDiff(LastSavedSnapshot.Content, TextEditorCore.GetText());
                Task.Factory.StartNew(
                    () => Dispatcher.RunAsync(CoreDispatcherPriority.Low,
                        () => SideBySideDiffViewer.Focus()));
            }
        }

        public string GetText()
        {
            return TextEditorCore.GetText();
        }

        // Make sure this method is thread safe
        public TextEditorStateMetaData GetTextEditorStateMetaData()
        {
            TextEditorCore.GetScrollViewerPosition(out var horizontalOffset, out var verticalOffset);
            TextEditorCore.GetTextSelectionPosition(out var textSelectionStartPosition, out var textSelectionEndPosition);

            var metaData = new TextEditorStateMetaData
            {
                FileNamePlaceholder = FileNamePlaceholder,
                LastSavedEncoding = EncodingUtility.GetEncodingName(LastSavedSnapshot.Encoding),
                LastSavedLineEnding = LineEndingUtility.GetLineEndingName(LastSavedSnapshot.LineEnding),
                DateModifiedFileTime = LastSavedSnapshot.DateModifiedFileTime,
                HasEditingFile = EditingFile != null,
                IsModified = IsModified,
                SelectionStartPosition = textSelectionStartPosition,
                SelectionEndPosition = textSelectionEndPosition,
                WrapWord = TextEditorCore.TextWrapping == TextWrapping.Wrap ||
                           TextEditorCore.TextWrapping == TextWrapping.WrapWholeWords,
                ScrollViewerHorizontalOffset = horizontalOffset,
                ScrollViewerVerticalOffset = verticalOffset,
                FontZoomFactor = TextEditorCore.GetFontZoomFactor() / 100,
                IsContentPreviewPanelOpened = _isContentPreviewPanelOpened,
                IsInDiffPreviewMode = (Mode == TextEditorMode.DiffPreview)
            };

            if (RequestedEncoding != null)
            {
                metaData.RequestedEncoding = EncodingUtility.GetEncodingName(RequestedEncoding);
            }

            if (RequestedLineEnding != null)
            {
                metaData.RequestedLineEnding = LineEndingUtility.GetLineEndingName(RequestedLineEnding.Value);
            }

            return metaData;
        }

        private void TextEditor_Loaded(object sender, RoutedEventArgs e)
        {
            Loaded?.Invoke(this, e);
            StartCheckingFileStatusPeriodically();
        }

        private void TextEditor_Unloaded(object sender, RoutedEventArgs e)
        {
            Unloaded?.Invoke(this, e);
            StopCheckingFileStatus();
        }

        public async void StartCheckingFileStatusPeriodically()
        {
            if (EditingFile == null) return;
            StopCheckingFileStatus();
            var cancellationTokenSource = new CancellationTokenSource();
            var cancellationToken = cancellationTokenSource.Token;
            _fileStatusCheckerCancellationTokenSource = cancellationTokenSource;

            try
            {
                await Task.Run(async () =>
                {
                    while (!cancellationToken.IsCancellationRequested)
                    {
                        await Task.Delay(TimeSpan.FromSeconds(_fileStatusCheckerDelayInSec), cancellationToken);
                        LoggingService.LogInfo($"Checking file status for \"{EditingFile.Path}\".", consoleOnly: true);
                        await CheckAndUpdateFileStatus();
                        await Task.Delay(TimeSpan.FromSeconds(_fileStatusCheckerPollingRateInSec), cancellationToken);
                    }
                }, cancellationToken);
            }
            catch (TaskCanceledException)
            {
                // ignore
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"Failed to check status for file [{EditingFile?.Path}]: {ex.Message}");
            }
        }

        public void StopCheckingFileStatus()
        {
            if (_fileStatusCheckerCancellationTokenSource?.IsCancellationRequested == false)
            {
                _fileStatusCheckerCancellationTokenSource.Cancel();
            }
        }

        private async Task CheckAndUpdateFileStatus()
        {
            if (EditingFile == null) return;

            await _fileStatusSemaphoreSlim.WaitAsync();

            FileModificationState? newState = null;

            if (!await FileSystemUtility.FileExists(EditingFile))
            {
                newState = FileModificationState.RenamedMovedOrDeleted;
            }
            else
            {
                newState = await FileSystemUtility.GetDateModified(EditingFile) != LastSavedSnapshot.DateModifiedFileTime ?
                    FileModificationState.Modified :
                    FileModificationState.Untouched;
            }

            await CoreApplication.MainView.CoreWindow.Dispatcher.RunAsync(CoreDispatcherPriority.Normal, () =>
            {
                FileModificationState = newState.Value;
            });

            _fileStatusSemaphoreSlim.Release();
        }

        private KeyboardCommandHandler GetKeyboardCommandHandler()
        {
            return new KeyboardCommandHandler(new List<IKeyboardCommand<KeyRoutedEventArgs>>
            {
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.F, (args) => ShowFindAndReplaceControl(showReplaceBar: false)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, true, VirtualKey.F, (args) => ShowFindAndReplaceControl(showReplaceBar: true)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.H, (args) => ShowFindAndReplaceControl(showReplaceBar: true)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.G, (args) => ShowGoToControl()),
                new KeyboardShortcut<KeyRoutedEventArgs>(false, true, false, VirtualKey.P, (args) => ShowHideContentPreview()),
                new KeyboardShortcut<KeyRoutedEventArgs>(false, true, false, VirtualKey.D, (args) => ShowHideSideBySideDiffViewer()),
                new KeyboardShortcut<KeyRoutedEventArgs>(VirtualKey.Escape, (args) =>
                {
                    if (_isContentPreviewPanelOpened)
                    {
                        _contentPreviewExtension.IsExtensionEnabled = false;
                        CloseSplitView();
                    }
                    else if (FindAndReplacePlaceholder != null && FindAndReplacePlaceholder.Visibility == Visibility.Visible)
                    {
                        HideFindAndReplaceControl();
                    }
                    else if (GoToPlaceholder != null && GoToPlaceholder.Visibility == Visibility.Visible)
                    {
                        HideGoToControl();
                    }
                }),
            });
        }

        public void Init(TextFile textFile, StorageFile file, bool resetLastSavedSnapshot = true, bool clearUndoQueue = true, bool isModified = false, bool resetText = true)
        {
            _loaded = false;
            EditingFile = file;
            RequestedEncoding = null;
            RequestedLineEnding = null;
            if (resetText)
            {
                TextEditorCore.SetText(textFile.Content);
            }
            if (resetLastSavedSnapshot)
            {
                textFile.Content = TextEditorCore.GetText();
                LastSavedSnapshot = textFile;
            }
            if (clearUndoQueue)
            {
                TextEditorCore.ClearUndoQueue();
            }
            IsModified = isModified;
            _loaded = true;
        }

        public async Task ReloadFromEditingFile()
        {
            await ReloadFromEditingFile(null);
        }

        public async Task ReloadFromEditingFile(Encoding encoding)
        {
            if (EditingFile != null)
            {
                var textFile = await FileSystemUtility.ReadFile(EditingFile, ignoreFileSizeLimit: false, encoding: encoding);
                Init(textFile, EditingFile, clearUndoQueue: false);
                LineEndingChanged?.Invoke(this, EventArgs.Empty);
                EncodingChanged?.Invoke(this, EventArgs.Empty);
                StartCheckingFileStatusPeriodically();
                CloseSideBySideDiffViewer();
                HideGoToControl();
                FileReloaded?.Invoke(this, EventArgs.Empty);
                Analytics.TrackEvent(encoding == null ? "OnFileReloaded" : "OnFileReopenedWithEncoding");
            }
        }

        public void ResetEditorState(TextEditorStateMetaData metadata, string newText = null)
        {
            if (!string.IsNullOrEmpty(metadata.RequestedEncoding))
            {
                TryChangeEncoding(EncodingUtility.GetEncodingByName(metadata.RequestedEncoding));
            }

            if (!string.IsNullOrEmpty(metadata.RequestedLineEnding))
            {
                TryChangeLineEnding(LineEndingUtility.GetLineEndingByName(metadata.RequestedLineEnding));
            }

            if (newText != null)
            {
                TextEditorCore.SetText(newText);
            }

            TextEditorCore.TextWrapping = metadata.WrapWord ? TextWrapping.Wrap : TextWrapping.NoWrap;
            TextEditorCore.FontSize = metadata.FontZoomFactor * EditorSettingsService.EditorFontSize;
            TextEditorCore.SetTextSelectionPosition(metadata.SelectionStartPosition, metadata.SelectionEndPosition);
            TextEditorCore.SetScrollViewerPosition(metadata.ScrollViewerHorizontalOffset, metadata.ScrollViewerVerticalOffset);

            if (EditorSettingsService.IsLineHighlighterEnabled) DrawLineHighlighter();
        }

        public void RevertAllChanges()
        {
            Init(LastSavedSnapshot, EditingFile, clearUndoQueue: false);
            ChangeReverted?.Invoke(this, EventArgs.Empty);
        }

        public bool TryChangeEncoding(Encoding encoding)
        {
            if (encoding == null) return false;

            if (!EncodingUtility.Equals(LastSavedSnapshot.Encoding, encoding))
            {
                RequestedEncoding = encoding;
                IsModified = true;
                EncodingChanged?.Invoke(this, EventArgs.Empty);
                return true;
            }

            if (RequestedEncoding != null && EncodingUtility.Equals(LastSavedSnapshot.Encoding, encoding))
            {
                RequestedEncoding = null;
                IsModified = !NoChangesSinceLastSaved();
                EncodingChanged?.Invoke(this, EventArgs.Empty);
                return true;
            }
            return false;
        }

        public bool TryChangeLineEnding(LineEnding lineEnding)
        {
            if (LastSavedSnapshot.LineEnding != lineEnding)
            {
                RequestedLineEnding = lineEnding;
                IsModified = true;
                LineEndingChanged?.Invoke(this, EventArgs.Empty);
                return true;
            }

            if (RequestedLineEnding != null && LastSavedSnapshot.LineEnding == lineEnding)
            {
                RequestedLineEnding = null;
                IsModified = !NoChangesSinceLastSaved();
                LineEndingChanged?.Invoke(this, EventArgs.Empty);
                return true;
            }
            return false;
        }

        public LineEnding GetLineEnding()
        {
            return RequestedLineEnding ?? LastSavedSnapshot.LineEnding;
        }

        public Encoding GetEncoding()
        {
            return RequestedEncoding ?? LastSavedSnapshot.Encoding;
        }

        private void OpenSplitView(IContentPreviewExtension extension)
        {
            SplitPanel.Content = extension;
            SplitPanelColumnDefinition.Width = new GridLength(ActualWidth / 2.0f);
            SplitPanelColumnDefinition.MinWidth = 100.0f;
            SplitPanel.Visibility = Visibility.Visible;
            GridSplitter.Visibility = Visibility.Visible;
            Analytics.TrackEvent("MarkdownContentPreview_Opened");
            _isContentPreviewPanelOpened = true;
        }

        private void CloseSplitView()
        {
            SplitPanelColumnDefinition.Width = new GridLength(0);
            SplitPanelColumnDefinition.MinWidth = 0.0f;
            SplitPanel.Visibility = Visibility.Collapsed;
            GridSplitter.Visibility = Visibility.Collapsed;
            TextEditorCore.Focus(FocusState.Programmatic);
            _isContentPreviewPanelOpened = false;
        }

        public void ShowHideContentPreview()
        {
            if (_contentPreviewExtension == null)
            {
                _contentPreviewExtension = ExtensionProvider?.GetContentPreviewExtension(FileType);
                if (_contentPreviewExtension == null) return;
                _contentPreviewExtension.Bind(TextEditorCore);
            }

            if (SplitPanel == null) LoadSplitView();

            if (SplitPanel.Visibility == Visibility.Collapsed)
            {
                _contentPreviewExtension.IsExtensionEnabled = true;
                OpenSplitView(_contentPreviewExtension);
            }
            else
            {
                _contentPreviewExtension.IsExtensionEnabled = false;
                CloseSplitView();
            }
        }

        public void OpenSideBySideDiffViewer()
        {
            if (string.Equals(LastSavedSnapshot.Content, TextEditorCore.GetText())) return;
            if (Mode == TextEditorMode.DiffPreview) return;
            if (SideBySideDiffViewer == null) LoadSideBySideDiffViewer();
            Mode = TextEditorMode.DiffPreview;
            TextEditorCore.IsEnabled = false;
            EditorRowDefinition.Height = new GridLength(0);
            SideBySideDiffViewRowDefinition.Height = new GridLength(1, GridUnitType.Star);
            SideBySideDiffViewer.Visibility = Visibility.Visible;
            SideBySideDiffViewer.RenderDiff(LastSavedSnapshot.Content, TextEditorCore.GetText());
            SideBySideDiffViewer.Focus();
            Analytics.TrackEvent("SideBySideDiffViewer_Opened");
        }

        public void CloseSideBySideDiffViewer()
        {
            if (Mode != TextEditorMode.DiffPreview) return;
            Mode = TextEditorMode.Editing;
            TextEditorCore.IsEnabled = true;
            EditorRowDefinition.Height = new GridLength(1, GridUnitType.Star);
            SideBySideDiffViewRowDefinition.Height = new GridLength(0);
            SideBySideDiffViewer.Visibility = Visibility.Collapsed;
            SideBySideDiffViewer.StopRenderingAndClearCache();
            TextEditorCore.Focus(FocusState.Programmatic);
        }

        private void ShowHideSideBySideDiffViewer()
        {
            if (Mode != TextEditorMode.DiffPreview)
            {
                OpenSideBySideDiffViewer();
            }
            else
            {
                CloseSideBySideDiffViewer();
            }
        }

        public void GetCurrentLineColumn(out int lineIndex, out int columnIndex, out int selectedCount)
        {
            TextEditorCore.GetCurrentLineColumn(out int line, out int column, out int selected);
            lineIndex = line;
            columnIndex = column;
            selectedCount = selected;
        }

        public double GetFontZoomFactor()
        {
            return TextEditorCore.GetFontZoomFactor();
        }

        public void SetFontZoomFactor(double fontZoomFactor)
        {
            TextEditorCore.SetFontZoomFactor(fontZoomFactor);
        }

        public bool IsEditorEnabled()
        {
            return TextEditorCore.IsEnabled;
        }

        public async Task SaveContentToFileAndUpdateEditorState(StorageFile file)
        {
            if (Mode == TextEditorMode.DiffPreview) CloseSideBySideDiffViewer();
            TextFile textFile = await SaveContentToFile(file); // Will throw if not succeeded
            FileModificationState = FileModificationState.Untouched;
            Init(textFile, file, clearUndoQueue: false, resetText: false);
            FileSaved?.Invoke(this, EventArgs.Empty);
            StartCheckingFileStatusPeriodically();
        }

        private async Task<TextFile> SaveContentToFile(StorageFile file)
        {
            var text = TextEditorCore.GetText();
            var encoding = RequestedEncoding ?? LastSavedSnapshot.Encoding;
            var lineEnding = RequestedLineEnding ?? LastSavedSnapshot.LineEnding;
            await FileSystemUtility.WriteToFile(LineEndingUtility.ApplyLineEnding(text, lineEnding), encoding, file); // Will throw if not succeeded
            var newFileModifiedTime = await FileSystemUtility.GetDateModified(file);
            return new TextFile(text, encoding, lineEnding, newFileModifiedTime);
        }

        public string GetContentForSharing()
        {
            return TextEditorCore.Document.Selection.StartPosition == TextEditorCore.Document.Selection.EndPosition ?
                TextEditorCore.GetText() :
                TextEditorCore.Document.Selection.Text;
        }

        public void TypeText(string text)
        {
            if (TextEditorCore.IsEnabled)
            {
                TextEditorCore.Document.Selection.TypeText(text);
            }
        }

        public void Focus()
        {
            if (Mode == TextEditorMode.DiffPreview)
            {
                SideBySideDiffViewer.Focus();
            }
            else if (Mode == TextEditorMode.Editing)
            {
                TextEditorCore.Focus(FocusState.Programmatic);
            }
        }

        public void CopySelectedTextToWindowsClipboard(TextControlCopyingToClipboardEventArgs args)
        {
            if (args != null)
            {
                args.Handled = true;
            }

            try
            {
                DataPackage dataPackage = new DataPackage { RequestedOperation = DataPackageOperation.Copy };
                var text = LineEndingUtility.ApplyLineEnding(TextEditorCore.Document.Selection.Text, GetLineEnding());
                dataPackage.SetText(text);
                Clipboard.SetContentWithOptions(dataPackage, new ClipboardContentOptions() { IsAllowedInHistory = true, IsRoamable = true });
                Clipboard.Flush(); // This method allows the content to remain available after the application shuts down.
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"Failed to copy plain text to Windows clipboard: {ex.Message}");
            }
        }

        public bool NoChangesSinceLastSaved(bool compareTextOnly = false)
        {
            if (!_loaded) return true;

            if (!compareTextOnly)
            {
                if (RequestedLineEnding != null)
                {
                    return false;
                }

                if (RequestedEncoding != null)
                {
                    return false;
                }
            }
            if (!string.Equals(LastSavedSnapshot.Content, TextEditorCore.GetText()))
            {
                return false;
            }
            return true;
        }

        private void LoadSplitView()
        {
            FindName("SplitPanel");
            FindName("GridSplitter");
            SplitPanel.Visibility = Visibility.Collapsed;
            GridSplitter.Visibility = Visibility.Collapsed;
            SplitPanel.KeyDown += SplitPanel_OnKeyDown;
        }

        private void LoadSideBySideDiffViewer()
        {
            FindName("SideBySideDiffViewer");
            SideBySideDiffViewer.Visibility = Visibility.Collapsed;
            SideBySideDiffViewer.OnCloseEvent += SideBySideDiffViewer_OnCloseEvent;
        }

        private void SideBySideDiffViewer_OnCloseEvent(object sender, EventArgs e)
        {
            CloseSideBySideDiffViewer();
        }

        private void SplitPanel_OnKeyDown(object sender, KeyRoutedEventArgs e)
        {
            _keyboardCommandHandler.Handle(e);
        }

        private void TextEditorCore_OnSelectionChanged(object sender, RoutedEventArgs e)
        {
            SelectionChanged?.Invoke(this, EventArgs.Empty);
        }

        private void TextEditorCore_OnFontZoomFactorChanged(object sender, double e)
        {
            FontZoomFactorChanged?.Invoke(this, EventArgs.Empty);
        }

        private void TextEditorCore_OnKeyDown(object sender, KeyRoutedEventArgs e)
        {
            _keyboardCommandHandler.Handle(e);
        }

        private void TextEditorCore_OnTextChanging(RichEditBox textEditor, RichEditBoxTextChangingEventArgs args)
        {
            if (!args.IsContentChanging || !_loaded) return;
            if (IsModified)
            {
                IsModified = !NoChangesSinceLastSaved();
            }
            else
            {
                IsModified = !NoChangesSinceLastSaved(compareTextOnly: true);
            }
            TextChanging?.Invoke(this, EventArgs.Empty);

            GoToPlaceholder?.Dismiss();
        }

        private void TextEditorCore_CopySelectedTextToWindowsClipboardRequested(object sender, TextControlCopyingToClipboardEventArgs e)
        {
            CopySelectedTextToWindowsClipboard(e);
        }

        public void ShowFindAndReplaceControl(bool showReplaceBar)
        {
            if (!TextEditorCore.IsEnabled || Mode != TextEditorMode.Editing)
            {
                return;
            }

            GoToPlaceholder?.Dismiss();

            if (FindAndReplacePlaceholder == null)
            {
                FindName("FindAndReplacePlaceholder"); // Lazy loading
            }

            var findAndReplace = (FindAndReplaceControl)FindAndReplacePlaceholder.Content;

            if (findAndReplace == null) return;

            FindAndReplacePlaceholder.Height = findAndReplace.GetHeight(showReplaceBar);
            findAndReplace.ShowReplaceBar(showReplaceBar);

            if (FindAndReplacePlaceholder.Visibility == Visibility.Collapsed)
            {
                FindAndReplacePlaceholder.Show();
            }

            findAndReplace.Focus();
        }

        public void HideFindAndReplaceControl()
        {
            FindAndReplacePlaceholder?.Dismiss();
        }

        private void FindAndReplaceControl_OnFindAndReplaceButtonClicked(object sender, FindAndReplaceEventArgs e)
        {
            TextEditorCore.Focus(FocusState.Programmatic);
            bool found = false;
            string regexError = null;

            switch (e.FindAndReplaceMode)
            {
                case FindAndReplaceMode.FindOnly:
                    found = TextEditorCore.FindNextAndSelect(e.SearchText, e.MatchCase, e.MatchWholeWord, e.UseRegex, out regexError, false);
                    // In case user hit "enter" key in search box instead of clicking on search button or hit F3
                    // We should re-focus on FindAndReplaceControl to make the next search "flows"
                    if (!(sender is Button))
                    {
                        FindAndReplaceControl.Focus();   
                    }
                    break;
                case FindAndReplaceMode.Replace:
                    found = TextEditorCore.FindNextAndReplace(e.SearchText, e.ReplaceText, e.MatchCase, e.MatchWholeWord, e.UseRegex, out regexError);
                    break;
                case FindAndReplaceMode.ReplaceAll:
                    found = TextEditorCore.FindAndReplaceAll(e.SearchText, e.ReplaceText, e.MatchCase, e.MatchWholeWord, e.UseRegex, out regexError);
                    break;
            }

            if (!found)
            {
                if (e.UseRegex && !string.IsNullOrEmpty(regexError))
                {
                    NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("FindAndReplace_NotificationMsg_InvalidRegex"), 1500);
                }
                else
                {
                    NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("FindAndReplace_NotificationMsg_NotFound"), 1500);   
                }
            }
        }

        private void FindAndReplacePlaceholder_Closed(object sender, Microsoft.Toolkit.Uwp.UI.Controls.InAppNotificationClosedEventArgs e)
        {
            FindAndReplacePlaceholder.Visibility = Visibility.Collapsed;
        }

        private void FindAndReplaceControl_OnDismissKeyDown(object sender, RoutedEventArgs e)
        {
            FindAndReplacePlaceholder?.Dismiss();
            TextEditorCore.Focus(FocusState.Programmatic);
        }

        public void ShowGoToControl()
        {
            if (!TextEditorCore.IsEnabled || Mode != TextEditorMode.Editing) return;

            FindAndReplacePlaceholder?.Dismiss();

            if (GoToPlaceholder == null)
                FindName("GoToPlaceholder"); // Lazy loading

            var goToControl = (GoToControl)GoToPlaceholder.Content;

            if (goToControl == null) return;

            GoToPlaceholder.Height = goToControl.GetHeight();

            if (GoToPlaceholder.Visibility == Visibility.Collapsed)
                GoToPlaceholder.Show();

            GetCurrentLineColumn(out var line, out _, out _);
            var maxLine = TextEditorCore.GetText().Length 
                - TextEditorCore.GetText().Replace(RichEditBoxDefaultLineEnding.ToString(), string.Empty).Length 
                + 1;
            goToControl.SetLineData(line, maxLine);
            goToControl.Focus();
        }

        public void HideGoToControl()
        {
            GoToPlaceholder?.Dismiss();
        }

        private void GoToControl_OnGoToButtonClicked(object sender, GoToEventArgs e)
        {
            var found = false;

            if (int.TryParse(e.SearchLine, out var line))
            { 
                found = TextEditorCore.GoTo(line);
            }

            if (!found)
            {
                GoToControl.Focus();
                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("FindAndReplace_NotificationMsg_NotFound"), 1500);
            }
            else
            {
                HideGoToControl();
                TextEditorCore.Focus(FocusState.Programmatic);
            }
        }

        private void GoToPlaceholder_Closed(object sender, Microsoft.Toolkit.Uwp.UI.Controls.InAppNotificationClosedEventArgs e)
        {
            GoToPlaceholder.Visibility = Visibility.Collapsed;
        }

        private void GoToControl_OnDismissKeyDown(object sender, RoutedEventArgs e)
        {
            GoToPlaceholder.Dismiss();
            TextEditorCore.Focus(FocusState.Programmatic);
        }

        private void DrawLineHighlighter()
        {
            TextEditorCore.Document.Selection.GetRect(Windows.UI.Text.PointOptions.ClientCoordinates, out Windows.Foundation.Rect highlightRect, out var _);
            LineHighlighter.Height = 1.35 * TextEditorCore.FontSize;

            var lineHighlighterBorderThickness = 0.1 * LineHighlighter.Height;

            TextEditorCore.GetScrollViewerPosition(out var _, out var verticalOffset);
            var lineHighlighterMargin = new Thickness(0, TextEditorCore.Padding.Top + highlightRect.Y - verticalOffset, 0, 0);

            if (lineHighlighterMargin.Top >= 0)
            {
                LineHighlighter.Visibility = Visibility.Visible;
                LineHighlighter.Margin = lineHighlighterMargin;
                LineHighlighter.BorderThickness = new Thickness(lineHighlighterBorderThickness);
            }
            else
            {
                if (LineHighlighter.Height + lineHighlighterMargin.Top > TextEditorCore.Padding.Top)
                {
                    LineHighlighter.BorderThickness = new Thickness(lineHighlighterBorderThickness, 0, lineHighlighterBorderThickness, lineHighlighterBorderThickness);
                    LineHighlighter.Height += lineHighlighterMargin.Top;
                    LineHighlighter.Visibility = Visibility.Visible;
                }
                else
                {
                    LineHighlighter.Visibility = Visibility.Collapsed;
                }
            }
        }

        private void LineHighlighter_OnViewStateChanged(object sender, bool enabled)
        {
            if (enabled)
            {
                LineHighlighter.Visibility = Visibility.Visible;
                DrawLineHighlighter();
            }
            else
            {
                LineHighlighter.Visibility = Visibility.Collapsed;
            }
        }

        private void LineHighlighter_OnSelectionChanged(object sender, RoutedEventArgs e)
        {
            if (EditorSettingsService.IsLineHighlighterEnabled) DrawLineHighlighter();
        }

        private void LineHighlighter_OnTextWrappingChanged(object sender, TextWrapping e)
        {
            if (EditorSettingsService.IsLineHighlighterEnabled) DrawLineHighlighter();
        }

        private void LineHighlighter_OnFontSizeChanged(object sender, double e)
        {
            if (EditorSettingsService.IsLineHighlighterEnabled) DrawLineHighlighter();
        }

        private void LineHighlighter_WindowSizeChanged(object sender, SizeChangedEventArgs e)
        {
            if (EditorSettingsService.EditorDefaultTextWrapping == TextWrapping.Wrap && EditorSettingsService.IsLineHighlighterEnabled) DrawLineHighlighter();
        }

        private void LineHighlighter_OnScrolled(object sender, ScrollViewerViewChangedEventArgs e)
        {
            if (EditorSettingsService.IsLineHighlighterEnabled) DrawLineHighlighter();
        }
    }
}