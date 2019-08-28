﻿
namespace Notepads.Controls.TextEditor
{
    using Microsoft.AppCenter.Analytics;
    using Notepads.Commands;
    using Notepads.Controls.FindAndReplace;
    using Notepads.Extensions;
    using Notepads.Services;
    using Notepads.Utilities;
    using System;
    using System.Collections.Generic;
    using System.Text;
    using System.Threading;
    using System.Threading.Tasks;
    using Windows.ApplicationModel.Core;
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

        private readonly IKeyboardCommandHandler<KeyRoutedEventArgs> _keyboardCommandHandler;

        private IContentPreviewExtension _contentPreviewExtension;

        public event EventHandler ModeChanged;

        public event EventHandler ModificationStateChanged;

        public event EventHandler FileModificationStateChanged;

        public event EventHandler LineEndingChanged;

        public event EventHandler EncodingChanged;

        public event EventHandler TextChanging;

        public event EventHandler ChangeReverted;

        public event RoutedEventHandler SelectionChanged;

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

        private StorageFile _editingFile;

        private readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

        private CancellationTokenSource _fileStatusCheckerCancellationTokenSource;

        private readonly int _fileStatusCheckerPollingRateInSec = 6;

        private readonly double _fileStatusCheckerDelayInSec = 0.2;

        private readonly SemaphoreSlim _fileStatusSemaphoreSlim = new SemaphoreSlim(1, 1);

        private TextEditorMode _mode = TextEditorMode.Editing;

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
            TextEditorCore.SelectionChanged += (sender, args) => { SelectionChanged?.Invoke(this, args); };
            TextEditorCore.KeyDown += TextEditorCore_OnKeyDown;
            TextEditorCore.ContextFlyout = new TextEditorContextFlyout(this, TextEditorCore);

            // Init shortcuts
            _keyboardCommandHandler = GetKeyboardCommandHandler();

            ThemeSettingsService.OnThemeChanged += (sender, theme) =>
            {
                if (Mode == TextEditorMode.DiffPreview)
                {
                    SideBySideDiffViewer.RenderDiff(LastSavedSnapshot.Content, TextEditorCore.GetText());
                    Task.Factory.StartNew(
                        () => Dispatcher.RunAsync(CoreDispatcherPriority.Low,
                            () => SideBySideDiffViewer.Focus()));
                }
            };

            Loaded += TextEditor_Loaded;
            Unloaded += TextEditor_Unloaded;
        }

        public string GetText()
        {
            return TextEditorCore.GetText();
        }

        public TextEditorStateMetaData GetTextEditorStateMetaData()
        {
            TextEditorCore.GetScrollViewerPosition(out var horizontalOffset, out var verticalOffset);

            var metaData = new TextEditorStateMetaData
            {
                LastSavedEncoding = EncodingUtility.GetEncodingName(LastSavedSnapshot.Encoding),
                LastSavedLineEnding = LineEndingUtility.GetLineEndingName(LastSavedSnapshot.LineEnding),
                DateModifiedFileTime = LastSavedSnapshot.DateModifiedFileTime,
                HasEditingFile = EditingFile != null,
                IsModified = IsModified,
                SelectionStartPosition = TextEditorCore.Document.Selection.StartPosition,
                SelectionEndPosition = TextEditorCore.Document.Selection.EndPosition,
                WrapWord = TextEditorCore.TextWrapping == TextWrapping.Wrap || TextEditorCore.TextWrapping == TextWrapping.WrapWholeWords,
                ScrollViewerHorizontalOffset = horizontalOffset,
                ScrollViewerVerticalOffset = verticalOffset,
                FontSize = TextEditorCore.FontSize,
                IsContentPreviewPanelOpened = (SplitPanel != null && SplitPanel.Visibility == Visibility.Visible),
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
            StartCheckingFileStatusPeriodically();
        }
        private void TextEditor_Unloaded(object sender, RoutedEventArgs e)
        {
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
            if (_fileStatusCheckerCancellationTokenSource != null)
            {
                if (!_fileStatusCheckerCancellationTokenSource.IsCancellationRequested)
                {
                    _fileStatusCheckerCancellationTokenSource.Cancel();
                }
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
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.P, (args) => ShowHideContentPreview()),
                new KeyboardShortcut<KeyRoutedEventArgs>(false, true, false, VirtualKey.D, (args) => ShowHideSideBySideDiffViewer()),
                new KeyboardShortcut<KeyRoutedEventArgs>(VirtualKey.Escape, (args) =>
                {
                    if (SplitPanel != null && SplitPanel.Visibility == Visibility.Visible)
                    {
                        _contentPreviewExtension.IsExtensionEnabled = false;
                        CloseSplitView();
                    }
                    else if (FindAndReplacePlaceholder != null && FindAndReplacePlaceholder.Visibility == Visibility.Visible)
                    {
                        HideFindAndReplaceControl();
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

            TextEditorCore.Document.Selection.StartPosition = metadata.SelectionStartPosition;
            TextEditorCore.Document.Selection.EndPosition = metadata.SelectionEndPosition;
            TextEditorCore.TextWrapping = metadata.WrapWord ? TextWrapping.Wrap : TextWrapping.NoWrap;
            TextEditorCore.FontSize = metadata.FontSize;
            TextEditorCore.SetScrollViewerPosition(metadata.ScrollViewerHorizontalOffset, metadata.ScrollViewerVerticalOffset);
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
        }

        private void CloseSplitView()
        {
            SplitPanelColumnDefinition.Width = new GridLength(0);
            SplitPanelColumnDefinition.MinWidth = 0.0f;
            SplitPanel.Visibility = Visibility.Collapsed;
            GridSplitter.Visibility = Visibility.Collapsed;
            TextEditorCore.Focus(FocusState.Programmatic);
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
            StartCheckingFileStatusPeriodically();
        }

        public async Task<TextFile> SaveContentToFile(StorageFile file)
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
            SideBySideDiffViewer.OnCloseEvent += (sender, args) => CloseSideBySideDiffViewer();
        }

        private void TextEditorCore_OnKeyDown(object sender, KeyRoutedEventArgs e)
        {
            _keyboardCommandHandler.Handle(e);
        }

        private void SplitPanel_OnKeyDown(object sender, KeyRoutedEventArgs e)
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
        }

        public void ShowFindAndReplaceControl(bool showReplaceBar)
        {
            if (!TextEditorCore.IsEnabled || Mode != TextEditorMode.Editing)
            {
                return;
            }

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

            findAndReplace.Focus(showReplaceBar ? FindAndReplaceMode.Replace : FindAndReplaceMode.FindOnly);
        }

        public void HideFindAndReplaceControl()
        {
            FindAndReplacePlaceholder?.Dismiss();
        }

        private void FindAndReplaceControl_OnFindAndReplaceButtonClicked(object sender, FindAndReplaceEventArgs e)
        {
            TextEditorCore.Focus(FocusState.Programmatic);
            bool found = false;

            switch (e.FindAndReplaceMode)
            {
                case FindAndReplaceMode.FindOnly:
                    found = TextEditorCore.FindNextAndSelect(e.SearchText, e.MatchCase, e.MatchWholeWord, false);
                    FindAndReplaceControl.Focus(FindAndReplaceMode.FindOnly);
                    break;
                case FindAndReplaceMode.Replace:
                    found = TextEditorCore.FindNextAndReplace(e.SearchText, e.ReplaceText, e.MatchCase, e.MatchWholeWord);
                    break;
                case FindAndReplaceMode.ReplaceAll:
                    found = TextEditorCore.FindAndReplaceAll(e.SearchText, e.ReplaceText, e.MatchCase, e.MatchWholeWord);
                    break;
            }

            if (!found)
            {
                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("FindAndReplace_NotificationMsg_NotFound"), 1500);
            }
        }

        private void FindAndReplacePlaceholder_Closed(object sender, Microsoft.Toolkit.Uwp.UI.Controls.InAppNotificationClosedEventArgs e)
        {
            FindAndReplacePlaceholder.Visibility = Visibility.Collapsed;
        }

        private void FindAndReplaceControl_OnDismissKeyDown(object sender, RoutedEventArgs e)
        {
            FindAndReplacePlaceholder.Dismiss();
            TextEditorCore.Focus(FocusState.Programmatic);
        }
    }
}