// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Diagnostics.CodeAnalysis;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Controls;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Documents.Text;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Storage;
using Notepads.Presentation.Controls.FindAndReplace;
using Notepads.Presentation.Controls.GoTo;
using Notepads.Presentation.Helpers;
using Notepads.Presentation.Input;
using Notepads.Presentation.PreviewExtensions;
using Notepads.Presentation.Theming;
using Notepads.Presentation.Workspace;
using Windows.ApplicationModel.DataTransfer;
using Windows.ApplicationModel.Resources;
using Windows.Storage;
using Windows.System;
using Windows.UI.Core;
using Windows.UI.Text;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Input;

namespace Notepads.Presentation.Controls.TextEditor;

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

public sealed partial class TextEditor : ITextEditor, IDisposable
{
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
    public event EventHandler FileRenamed;

    public Guid Id { get; set; }

    public INotepadsExtensionProvider ExtensionProvider;

    private string _fileNamePlaceholder = string.Empty;
    public string FileNamePlaceholder
    {
        get => _fileNamePlaceholder;
        set
        {
            _fileNamePlaceholder = value;
            UpdateDocumentInfo();
        }
    }

    public FileType FileType { get; private set; }

    public string EditingFileName { get; private set; }

    public string EditingFilePath { get; private set; }

    private StorageFile _editingFile;

    public StorageFile EditingFile
    {
        get => _editingFile;
        private set
        {
            _editingFile = value;
            UpdateDocumentInfo();
        }
    }

    private void UpdateDocumentInfo()
    {
        if (EditingFile == null)
        {
            EditingFileName = null;
            EditingFilePath = null;
            FileType = FileTypeUtility.GetFileTypeByFileName(FileNamePlaceholder);
        }
        else
        {
            EditingFileName = EditingFile.Name;
            EditingFilePath = EditingFile.Path;
            FileType = FileTypeUtility.GetFileTypeByFileName(EditingFile.Name);
        }

        RefreshDocumentLanguage();
        // Hide content preview if current file type is not supported for previewing
        if (!FileTypeUtility.IsPreviewSupported(FileType))
        {
            if (SplitPanel != null && SplitPanel.Visibility == Visibility.Visible)
            {
                ShowHideContentPreview();
            }
        }
    }

    private bool _isModified;

    public bool IsDocumentEmpty => TextEditorCore.IsDocumentEmpty;

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

    private FileModificationState _fileModificationState;

    private bool _isContentPreviewPanelOpened;

    private readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

    [SuppressMessage("Usage", "CA2213:Disposable fields should be disposed",
        Justification = "The poll task owns and disposes this source in its finally block; Dispose cancels and drains that task first.")]
    private CancellationTokenSource _fileStatusCheckerCancellationTokenSource;
    private Task _fileStatusCheckerCompletion = Task.CompletedTask;

    private readonly int _fileStatusCheckerPollingRateInSec = 6;

    private readonly double _fileStatusCheckerDelayInSec = 0.3;

    private readonly SemaphoreSlim _fileStatusSemaphoreSlim = new(1, 1);
    private bool _disposed;

    private TextEditorMode _mode = TextEditorMode.Editing;

    private readonly ICommandHandler<KeyRoutedEventArgs> _keyboardCommandHandler;

    private IContentPreviewExtension _contentPreviewExtension;
    private SearchContext _lastSearchContext = new(string.Empty);

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

    public bool DisplayLineNumbers
    {
        get => TextEditorCore.DisplayLineNumbers;
        set => TextEditorCore.DisplayLineNumbers = value;
    }

    public bool DisplayLineHighlighter
    {
        get => TextEditorCore.DisplayLineHighlighter;
        set => TextEditorCore.DisplayLineHighlighter = value;
    }

    public TextEditor()
    {
        InitializeComponent();

        TextEditorCore.TextChanging += TextEditorCore_OnTextChanging;
        TextEditorCore.LanguageDetectionRequested += OnLanguageDetectionRequested;
        TextEditorCore.SyntaxStatusChanged += OnSyntaxStatusChanged;
        TextEditorCore.ModificationStateChanged += TextEditorCore_OnModificationStateChanged;
        TextEditorCore.SelectionChanged += TextEditorCore_OnSelectionChanged;
        TextEditorCore.SizeChanged += TextEditorCore_OnSizeChanged;
        TextEditorCore.KeyDown += TextEditorCore_OnKeyDown;
        TextEditorCore.CopyTextToWindowsClipboardRequested += TextEditorCore_CopyTextToWindowsClipboardRequested;
        TextEditorCore.CutSelectedTextToWindowsClipboardRequested += TextEditorCore_CutSelectedTextToWindowsClipboardRequested;
        TextEditorCore.ContextFlyout = new TextEditorContextFlyout(this, TextEditorCore);

        // Init shortcuts
        _keyboardCommandHandler = GetKeyboardCommandHandler();

        base.Loaded += TextEditor_Loaded;
        base.Unloaded += TextEditor_Unloaded;
        base.PreviewKeyDown += TextEditor_PreviewKeyDown;
        base.KeyDown += TextEditor_KeyDown;

        TextEditorCore.FontZoomFactorChanged += TextEditorCore_OnFontZoomFactorChanged;
        RefreshDocumentLanguage();
    }

    private void TextEditor_KeyDown(object sender, KeyRoutedEventArgs e)
    {
        KeyDown?.Invoke(this, e);
    }

    private void TextEditor_PreviewKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Handled) return;
        if (e.Key == VirtualKey.Escape &&
            (FindAndReplacePlaceholder?.Visibility == Visibility.Visible ||
             GoToPlaceholder?.Visibility == Visibility.Visible || _isContentPreviewPanelOpened || _diffGeneration != null))
        {
            OnEscapeKeyDown();
            e.Handled = true;
            return;
        }

        // Scintilla handles Ctrl+T before the normal KeyDown event bubbles to the page.
        // Forward this application shortcut while it can still override the editor binding.
        if (e.Key == VirtualKey.T &&
            Window.Current.CoreWindow.GetKeyState(VirtualKey.Control).HasFlag(CoreVirtualKeyStates.Down) &&
            !Window.Current.CoreWindow.GetKeyState(VirtualKey.Menu).HasFlag(CoreVirtualKeyStates.Down) &&
            !Window.Current.CoreWindow.GetKeyState(VirtualKey.Shift).HasFlag(CoreVirtualKeyStates.Down))
        {
            KeyDown?.Invoke(this, e);
        }
    }

    // Unhook events and clear state
    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        var failures = new List<Exception>();
        BeginShutdown(CancelSearch, failures);
        BeginShutdown(CloseSideBySideDiffViewer, failures);
        BeginShutdown(StopCheckingFileStatus, failures);
        BeginShutdown(_documentOperations.Dispose, failures);
        try { DisposeUserInterface(); }
        catch (Exception failure) { failures.Add(failure); }
        finally
        {
            _ = CompleteDisposalAsync(failures.Count == 0 ? null : new AggregateException(failures));
        }
    }

    private void DisposeUserInterface()
    {
        TextEditorCore.IsEnabled = false;

        TextEditorCore.TextChanging -= TextEditorCore_OnTextChanging;
        TextEditorCore.LanguageDetectionRequested -= OnLanguageDetectionRequested;
        TextEditorCore.SyntaxStatusChanged -= OnSyntaxStatusChanged;
        TextEditorCore.ModificationStateChanged -= TextEditorCore_OnModificationStateChanged;
        TextEditorCore.SelectionChanged -= TextEditorCore_OnSelectionChanged;
        TextEditorCore.SizeChanged -= TextEditorCore_OnSizeChanged;
        TextEditorCore.KeyDown -= TextEditorCore_OnKeyDown;
        TextEditorCore.CopyTextToWindowsClipboardRequested -= TextEditorCore_CopyTextToWindowsClipboardRequested;
        TextEditorCore.CutSelectedTextToWindowsClipboardRequested -= TextEditorCore_CutSelectedTextToWindowsClipboardRequested;

        if (TextEditorCore.ContextFlyout is TextEditorContextFlyout contextFlyout)
        {
            contextFlyout.Dispose();
            TextEditorCore.ContextFlyout = null;
        }

        Unloaded?.Invoke(this, new RoutedEventArgs());

        base.Loaded -= TextEditor_Loaded;
        base.Unloaded -= TextEditor_Unloaded;
        base.PreviewKeyDown -= TextEditor_PreviewKeyDown;
        base.KeyDown -= TextEditor_KeyDown;

        TextEditorCore.FontZoomFactorChanged -= TextEditorCore_OnFontZoomFactorChanged;

        _contentPreviewExtension?.Dispose();

        if (SplitPanel != null)
        {
            SplitPanel.KeyDown -= SplitPanel_OnKeyDown;
            UnloadObject(SplitPanel);
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
    }

    private static void BeginShutdown(Action action, List<Exception> failures)
    {
        try { action(); }
        catch (Exception failure) { failures.Add(failure); }
    }

    public string GetText()
    {
        return TextEditorCore.GetText();
    }

    public Task<string> GetTextAsync() => TextEditorCore.GetTextAsync();

    // Capture on the owning UI thread before asynchronous persistence work.
    public DocumentMetadata GetTextEditorStateMetaData()
    {
        TextEditorCore.GetScrollViewerPosition(out var horizontalOffset, out var verticalOffset);
        TextEditorCore.GetTextSelectionPosition(out var textSelectionStartPosition, out var textSelectionEndPosition);

        var metaData = new DocumentMetadata
        {
            FileNamePlaceholder = FileNamePlaceholder,
            LanguageOverride = LanguageOverride,
            LastSavedEncoding = EncodingCatalog.GetEncodingName(LastSavedSnapshot.Encoding),
            LastSavedLineEnding = LineEndingUtility.GetLineEndingName(LastSavedSnapshot.LineEnding),
            DateModifiedFileTime = LastSavedSnapshot.DateModifiedFileTime,
            HasEditingFile = EditingFile != null,
            RequiresSaveAs = _requiresSaveAs,
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

        if (_requestedEncoding != null)
        {
            metaData.RequestedEncoding = EncodingCatalog.GetEncodingName(_requestedEncoding);
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

        // Insert "Legacy Windows Notepad" style date and time if document starts with ".LOG"
        TextEditorCore.TryInsertNewLogEntry();
    }

    private void TextEditor_Unloaded(object sender, RoutedEventArgs e)
    {
        Unloaded?.Invoke(this, e);
        StopCheckingFileStatus();
    }

    public void StartCheckingFileStatusPeriodically()
    {
        if (_disposed || EditingFile == null) return;
        StopCheckingFileStatus();
        var cancellationTokenSource = new CancellationTokenSource();
        _fileStatusCheckerCancellationTokenSource = cancellationTokenSource;
        _fileStatusCheckerCompletion = CheckFileStatusPeriodicallyAsync(_fileStatusCheckerCompletion, cancellationTokenSource);
    }

    private async Task CheckFileStatusPeriodicallyAsync(Task previous, CancellationTokenSource cancellationTokenSource)
    {
        var cancellationToken = cancellationTokenSource.Token;
        try
        {
            await previous;
            await Task.Run(async () =>
            {
                while (!cancellationToken.IsCancellationRequested)
                {
                    await Task.Delay(TimeSpan.FromSeconds(_fileStatusCheckerDelayInSec), cancellationToken);
                    LoggingService.LogInfo($"[{nameof(TextEditor)}] Checking file status for \"{EditingFile.Path}\".", consoleOnly: true);
                    await CheckAndUpdateFileStatusAsync(cancellationToken);
                    await Task.Delay(TimeSpan.FromSeconds(_fileStatusCheckerPollingRateInSec), cancellationToken);
                }
            }, cancellationToken);
        }
        catch (TaskCanceledException)
        {
            // ignore
        }
        catch (ObjectDisposedException)
        {
            // ignore
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(TextEditor)}] Failed to check status for file [{EditingFile?.Path}]: {ex.Message}");
        }
        finally
        {
            if (ReferenceEquals(_fileStatusCheckerCancellationTokenSource, cancellationTokenSource))
                _fileStatusCheckerCancellationTokenSource = null;
            cancellationTokenSource.Dispose();
        }
    }

    public void StopCheckingFileStatus()
    {
        if (_fileStatusCheckerCancellationTokenSource?.IsCancellationRequested == false)
        {
            _fileStatusCheckerCancellationTokenSource.Cancel();
        }
    }

    private async Task CheckAndUpdateFileStatusAsync(CancellationToken cancellationToken)
    {
        await _fileStatusSemaphoreSlim.WaitAsync(cancellationToken);
        try
        {
            StorageFile file = null;
            DocumentSnapshot savedSnapshot = null;
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                if (_disposed || cancellationToken.IsCancellationRequested) return;
                file = EditingFile;
                savedSnapshot = LastSavedSnapshot;
            });
            if (file == null) return;

            FileModificationState newState;
            if (!await FileStorage.FileExistsAsync(file))
            {
                newState = FileModificationState.RenamedMovedOrDeleted;
            }
            else
            {
                long fileModifiedTime = await FileStorage.GetDateModifiedAsync(file);
                newState = savedSnapshot.DateModifiedFileTime >= 0 && fileModifiedTime != savedSnapshot.DateModifiedFileTime ?
                    FileModificationState.Modified : FileModificationState.Untouched;
            }

            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                // Saving, reloading, or closing can invalidate a check while
                // it waits for the storage provider.
                if (!_disposed && !cancellationToken.IsCancellationRequested &&
                    ReferenceEquals(file, EditingFile) && ReferenceEquals(savedSnapshot, LastSavedSnapshot))
                {
                    FileModificationState = newState;
                }
            });
        }
        finally
        {
            _fileStatusSemaphoreSlim.Release();
        }
    }

    private KeyboardCommandHandler GetKeyboardCommandHandler()
    {
        return new KeyboardCommandHandler(new List<IKeyboardCommand<KeyRoutedEventArgs>>
        {
            new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.F, (args) => ShowFindAndReplaceControl(showReplaceBar: false)),
            new KeyboardCommand<KeyRoutedEventArgs>(true, false, true, VirtualKey.F, (args) => ShowFindAndReplaceControl(showReplaceBar: true)),
            new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.H, (args) => ShowFindAndReplaceControl(showReplaceBar: true)),
            new KeyboardCommand<KeyRoutedEventArgs>(true, false, false, VirtualKey.G, (args) => ShowGoToControl()),
            new KeyboardCommand<KeyRoutedEventArgs>(VirtualKey.F3, async (args) =>
                await InitiateFindAndReplaceAsync(new FindAndReplaceEventArgs (_lastSearchContext, string.Empty, FindAndReplaceMode.FindOnly, SearchDirection.Next))),
            new KeyboardCommand<KeyRoutedEventArgs>(false, false, true, VirtualKey.F3, async (args) =>
                await InitiateFindAndReplaceAsync(new FindAndReplaceEventArgs (_lastSearchContext, string.Empty, FindAndReplaceMode.FindOnly, SearchDirection.Previous))),
            new KeyboardCommand<KeyRoutedEventArgs>(VirtualKey.Escape, (args) => { OnEscapeKeyDown(); }, shouldHandle: false, shouldSwallow: true)
        });
    }

    private void OpenSplitView(IContentPreviewExtension extension)
    {
        _contentPreviewExtension = extension;
        SplitPanel.Content = extension;
        SplitPanelColumnDefinition.Width = new GridLength(1, GridUnitType.Star);
        SplitPanelColumnDefinition.MinWidth = 100.0f;
        SplitPanel.Visibility = Visibility.Visible;
        GridSplitter.Visibility = Visibility.Visible;
        AnalyticsService.TrackEvent("MarkdownContentPreview_Opened");
        _isContentPreviewPanelOpened = true;
    }

    private void CloseSplitView()
    {
        SplitPanelColumnDefinition.Width = new GridLength(0);
        EditorColumnDefinition.Width = new GridLength(1, GridUnitType.Star);
        SplitPanelColumnDefinition.MinWidth = 0.0f;
        SplitPanel.Visibility = Visibility.Collapsed;
        GridSplitter.Visibility = Visibility.Collapsed;
        TextEditorCore.ResetFocusAndScrollToPreviousPosition();
        _isContentPreviewPanelOpened = false;
    }

    public void ShowHideContentPreview() { }

    /// <summary>
    /// Returns 1-based indexing values
    /// </summary>
    public void GetLineColumnSelection(
        out int startLine,
        out int endLine,
        out int startColumn,
        out int endColumn,
        out int selected,
        out int lineCount)
    {
        TextEditorCore.GetLineColumnSelection(
            out startLine,
            out endLine,
            out startColumn,
            out endColumn,
            out selected,
            out lineCount,
            GetLineEnding());
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

    public string GetContentForSharing()
    {
        return !TextEditorCore.HasSelection ?
            TextEditorCore.GetText() :
            TextEditorCore.GetSelectedText();
    }

    public void TypeText(string text)
    {
        if (TextEditorCore.IsEnabled)
        {
            TextEditorCore.TypeText(text);
        }
    }

    public void Focus()
    {
        if (Mode == TextEditorMode.Editing)
        {
            TextEditorCore.ResetFocusAndScrollToPreviousPosition();
        }
        else _diffGeneration?.Viewer.FocusEditor();
    }

    public FlyoutBase GetContextFlyout()
    {
        return TextEditorCore.ContextFlyout;
    }

    public void CopyTextToWindowsClipboard(TextControlCopyingToClipboardEventArgs args)
    {
        if (args != null)
        {
            args.Handled = true;
        }

        if (ApplicationPreferences.IsSmartCopyEnabled)
        {
            TextEditorCore.SmartlyTrimTextSelection();
        }

        CopyTextToWindowsClipboardInternal(true);
    }

    public void CutSelectedTextToWindowsClipboard(TextControlCuttingToClipboardEventArgs args)
    {
        if (args != null)
        {
            args.Handled = true;
        }

        if (CopyTextToWindowsClipboardInternal(false))
        {
            TextEditorCore.DeleteSelection();
        }
    }

    private bool CopyTextToWindowsClipboardInternal(bool clearLineSelection)
    {
        try
        {
            DataPackage dataPackage = new DataPackage { RequestedOperation = DataPackageOperation.Copy };

            var isTextSelected = TextEditorCore.HasSelection;
            TextEditorCore.GetTextSelectionPosition(out var cursorPosition, out _);

            if (!isTextSelected)
            {
                TextEditorCore.SelectCurrentLine();
            }

            var text = LineEndingUtility.ApplyLineEnding(TextEditorCore.GetSelectedText(), GetLineEnding());
            dataPackage.SetText(text);

            if (clearLineSelection && !isTextSelected)
            {
                TextEditorCore.SetTextSelectionPosition(cursorPosition, cursorPosition);
            }

            if (!Clipboard.SetContentWithOptions(dataPackage, new ClipboardContentOptions() { IsAllowedInHistory = true, IsRoamable = true }))
            {
                return false;
            }
            Clipboard.Flush(); // This method allows the content to remain available after the application shuts down.
            return true;
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(TextEditor)}] Failed to copy plain text to Windows clipboard: {ex.Message}");
            return false;
        }
    }

    public bool NoChangesSinceLastSaved(bool compareTextOnly = false)
    {
        if (!_loaded) return true;

        if (!compareTextOnly)
        {
            if (_requiresSaveAs) return false;
            if (RequestedLineEnding != null)
            {
                return false;
            }

            if (_requestedEncoding != null)
            {
                return false;
            }
        }

        return !TextEditorCore.IsDocumentModified;
    }

    private void OnEscapeKeyDown()
    {
        CancelSearch();
        if (_diffGeneration != null)
        {
            CloseSideBySideDiffViewer();
            return;
        }
        if (FindAndReplacePlaceholder != null && FindAndReplacePlaceholder.Visibility == Visibility.Visible)
        {
            HideFindAndReplaceControl();
            TextEditorCore.Focus(FocusState.Programmatic);
        }
        else if (GoToPlaceholder != null && GoToPlaceholder.Visibility == Visibility.Visible)
        {
            HideGoToControl();
            TextEditorCore.Focus(FocusState.Programmatic);
        }
        else if (_isContentPreviewPanelOpened)
        {
            _contentPreviewExtension.IsExtensionEnabled = false;
            CloseSplitView();
        }
    }

    private void LoadSplitView()
    {
        FindName("SplitPanel");
        FindName("GridSplitter");
        SplitPanel.Visibility = Visibility.Collapsed;
        GridSplitter.Visibility = Visibility.Collapsed;
        SplitPanel.KeyDown += SplitPanel_OnKeyDown;
    }

    private void SplitPanel_OnKeyDown(object sender, KeyRoutedEventArgs e)
    {
        var result = _keyboardCommandHandler.Handle(e);
        if (result.ShouldHandle)
        {
            e.Handled = true;
        }
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
        var ctrl = Window.Current.CoreWindow.GetKeyState(VirtualKey.Control);
        var alt = Window.Current.CoreWindow.GetKeyState(VirtualKey.Menu);

        if (FindAndReplacePlaceholder?.Visibility == Visibility.Visible && !ctrl.HasFlag(CoreVirtualKeyStates.Down) && !alt.HasFlag(CoreVirtualKeyStates.Down))
        {
            if (e.Key == VirtualKey.F3)
            {
                return;
            }
        }

        var result = _keyboardCommandHandler.Handle(e);
        if (result.ShouldHandle)
        {
            e.Handled = true;
        }
    }

    private void TextEditorCore_OnModificationStateChanged(object sender, EventArgs args)
    {
        if (_loaded) IsModified = TextEditorCore.IsDocumentModified || _requestedEncoding != null || RequestedLineEnding != null;
    }

    private void TextEditorCore_OnTextChanging(object textEditor, EventArgs args)
    {
        if (!_loaded) return;
        IsModified = !NoChangesSinceLastSaved();
        TextChanging?.Invoke(this, EventArgs.Empty);

        GoToPlaceholder?.Dismiss();
    }

    private void TextEditorCore_CopyTextToWindowsClipboardRequested(object sender, EventArgs e)
    {
        CopyTextToWindowsClipboard(null);
    }

    private void TextEditorCore_CutSelectedTextToWindowsClipboardRequested(object sender, EventArgs e)
    {
        CutSelectedTextToWindowsClipboard(null);
    }

    private void FindAndReplaceControl_OnToggleReplaceModeButtonClicked(object sender, bool showReplaceBar)
    {
        ShowFindAndReplaceControl(showReplaceBar);
    }

    private void TextEditorCore_OnSizeChanged(object sender, SizeChangedEventArgs e)
    {
        UpdateFindAndReplaceWidth(e.NewSize.Width);
    }

    private void UpdateFindAndReplaceWidth(double editorWidth)
    {
        if (FindAndReplacePlaceholder?.Content is FindAndReplaceControl control)
        {
            // Leave room for the notification's horizontal offset and the editor edge.
            control.Width = Math.Min(390, Math.Max(300, editorWidth - 44));
        }
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

        UpdateFindAndReplaceWidth(TextEditorCore.ActualWidth);
        FindAndReplacePlaceholder.Height = findAndReplace.GetHeight(showReplaceBar);
        findAndReplace.ShowReplaceBar(showReplaceBar);

        if (FindAndReplacePlaceholder.Visibility == Visibility.Collapsed)
        {
            FindAndReplacePlaceholder.Show();
        }

        findAndReplace.Focus(TextEditorCore.GetSearchString(), FindAndReplaceMode.FindOnly);
    }

    public void HideFindAndReplaceControl()
    {
        CancelSearch();
        FindAndReplacePlaceholder?.Dismiss();
    }

    private async void FindAndReplaceControl_OnFindAndReplaceButtonClicked(object sender, FindAndReplaceEventArgs e)
    {
        TextEditorCore.Focus(FocusState.Programmatic);
        var search = InitiateFindAndReplaceAsync(e);
        var version = _searchRequestVersion;
        var found = await search;
        if (_disposed || version != _searchRequestVersion) return;

        // In case user hit "enter" key in search box instead of clicking on search button or hit F3
        // We should re-focus on FindAndReplaceControl to make the next search "flows"
        if (sender is not Button)
        {
            if (found)
            {
                // Wait for layout to refresh (ScrollViewer scroll to the found text) before focusing
                await Task.Delay(10);
            }
            if (!_disposed && version == _searchRequestVersion && FindAndReplacePlaceholder?.Visibility == Visibility.Visible)
                FindAndReplaceControl.Focus(string.Empty, e.FindAndReplaceMode);
        }
    }

    private void FindAndReplacePlaceholder_Closed(object sender, InAppNotificationClosedEventArgs e)
    {
        CancelSearch();
        FindAndReplacePlaceholder.Visibility = Visibility.Collapsed;
    }

    private void FindAndReplaceControl_OnDismissKeyDown(object sender, RoutedEventArgs e)
    {
        CancelSearch();
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

        GetLineColumnSelection(out var startLine, out _, out _, out _, out _, out var lineCount);
        goToControl.SetLineData(startLine, lineCount);
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

    private void GoToPlaceholder_Closed(object sender, InAppNotificationClosedEventArgs e)
    {
        GoToPlaceholder.Visibility = Visibility.Collapsed;
    }

    private void GoToControl_OnDismissKeyDown(object sender, RoutedEventArgs e)
    {
        GoToPlaceholder.Dismiss();
        TextEditorCore.Focus(FocusState.Programmatic);
    }
}
