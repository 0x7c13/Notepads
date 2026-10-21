// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Text.Json;
using System.Threading.Tasks;
using Notepads.Controls;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.IO;
using Notepads.Features.Preferences;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Storage;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Controls;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Helpers;
using Notepads.Presentation.PreviewExtensions;
using Notepads.Presentation.Theming;
using Notepads.Presentation.Workspace.Activation;
using Windows.ApplicationModel.DataTransfer;
using Windows.ApplicationModel.Resources;
using Windows.Storage;
using Windows.UI;
using Windows.UI.Core;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Media;
using WinUIEditor;

namespace Notepads.Presentation.Workspace;

public sealed partial class NotepadsCore : INotepadsCore
{
    public event EventHandler<WorkspaceTab> TabOpened;
    public event EventHandler<WorkspaceTab> TabClosed;
    public event EventHandler<WorkspaceTab> TabClosing;
    public event EventHandler<WorkspaceTab> TabSelected;
    public event EventHandler<WorkspaceTab> TabStateChanged;
    public event EventHandler<WorkspaceTab> EditorAttached;
    public event EventHandler<ITextEditor> TextEditorLoaded;
    public event EventHandler<ITextEditor> TextEditorEditorModificationStateChanged;
    public event EventHandler<ITextEditor> TextEditorFileModificationStateChanged;
    public event EventHandler<ITextEditor> TextEditorSaved;
    public event EventHandler<ITextEditor> TextEditorRenamed;
    public event EventHandler<ITextEditor> TextEditorTitleChanged;
    public event EventHandler<ITextEditor> TextEditorSelectionChanged;
    public event EventHandler<ITextEditor> TextEditorFontZoomFactorChanged;
    public event EventHandler<ITextEditor> TextEditorLanguageChanged;
    public event EventHandler<ITextEditor> TextEditorEncodingChanged;
    public event EventHandler<ITextEditor> TextEditorLineEndingChanged;
    public event EventHandler<ITextEditor> TextEditorMovedToAnotherAppInstance;
    public event EventHandler<IReadOnlyList<IStorageItem>> StorageItemsDropped;

    public event KeyEventHandler TextEditorKeyDown;

    private readonly SetsView _sets;

    private readonly INotepadsExtensionProvider _extensionProvider;

    private readonly Func<ITextEditor, Task> _renameTextEditorAsync;

    private readonly Func<ISessionController> _sessionController;

    private readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

    private readonly WindowContext _context;
    private readonly CoreDispatcher _dispatcher;

    private readonly Dictionary<Guid, TextEditorTransfer> _transfers = new();
    private readonly HashSet<ITextEditor> _ownedEditors = new();

    private bool _disposed;
    private readonly HashSet<Task> _pendingWork = new();
    private readonly TaskCompletionSource<object> _disposalCompletion = new(
        TaskCreationOptions.RunContinuationsAsynchronously);
    private Exception _shutdownFailure;

    internal NotepadsCore(SetsView sets,
        INotepadsExtensionProvider extensionProvider,
        CoreDispatcher dispatcher, WindowContext context,
        Func<ITextEditor, Task> renameTextEditorAsync, Func<ISessionController> sessionController)
    {
        _sets = sets;
        _sets.SetClosing += SetsView_OnSetClosing;
        _sets.SetTapped += SetsView_OnSetTapped;
        _sets.SelectionChanged += SetsView_OnSelectionChanged;
        _sets.SetDraggedOutside += Sets_SetDraggedOutside;
        _sets.DragOver += Sets_DragOver;
        _sets.Drop += Sets_Drop;
        _sets.DragItemsStarting += Sets_DragItemsStarting;
        _sets.DragItemsCompleted += Sets_DragItemsCompleted;

        _dispatcher = dispatcher;
        _context = context ?? throw new ArgumentNullException(nameof(context));
        _extensionProvider = extensionProvider;
        // The page owns the guarded rename flow; tab menus route to it.
        _renameTextEditorAsync = renameTextEditorAsync ?? throw new ArgumentNullException(nameof(renameTextEditorAsync));
        _sessionController = sessionController ?? throw new ArgumentNullException(nameof(sessionController));

        ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;
    }

    public void Dispose()
    {
        if (_disposed) return;
        var items = _sets.Items?.Cast<SetsViewItem>().ToArray() ?? [];
        _disposed = true;
        _fileOperationLifetime.Cancel();
        ThemeSettingsService.OnAccentColorChanged -= ThemeSettingsService_OnAccentColorChanged;
        _sets.SetClosing -= SetsView_OnSetClosing;
        _sets.SetTapped -= SetsView_OnSetTapped;
        _sets.SelectionChanged -= SetsView_OnSelectionChanged;
        _sets.SetDraggedOutside -= Sets_SetDraggedOutside;
        _sets.DragOver -= Sets_DragOver;
        _sets.Drop -= Sets_Drop;
        _sets.DragItemsStarting -= Sets_DragItemsStarting;
        _sets.DragItemsCompleted -= Sets_DragItemsCompleted;

        var transfers = _transfers.Values.ToArray();
        _transfers.Clear();
        foreach (var transfer in transfers)
            _ = TrackWorkAsync(transfer.CompleteAsync(TextEditorTransferResult.Cancelled, sourceIsAlive: false));

        // View shutdown does not mean the user discarded these documents.
        // Leave durable recovery roots for the next activation.
        foreach (var item in items)
        {
            if (item.Tag is WorkspaceTab tab)
            {
                tab.StateChanged -= OnTabStateChanged;
                tab.MarkClosed();
            }
            try { if (item.ContextFlyout is TabContextFlyout flyout) flyout.Dispose(); }
            catch (Exception ex) { LoggingService.LogException(ex); }
        }
        foreach (var editor in _ownedEditors.ToArray())
        {
            try { DisposeTextEditor(editor); }
            catch (Exception ex) { _shutdownFailure ??= ex; LoggingService.LogException(ex); }
        }
        _ = CompleteDisposalAsync();
    }

    public ValueTask DisposeAsync()
    {
        Dispose();
        return new ValueTask(_disposalCompletion.Task);
    }

    private Task TrackWorkAsync(Task work, bool requiresShutdownProof = false)
    {
        var completion = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
        _pendingWork.Add(completion.Task);
        _ = CompleteTrackedWorkAsync(work, completion, requiresShutdownProof);
        return work;
    }

    private Task<T> TrackWorkAsync<T>(Task<T> work)
    {
        TrackWorkAsync((Task)work);
        return work;
    }

    private async Task CompleteTrackedWorkAsync(Task work, TaskCompletionSource<object> completion, bool requiresShutdownProof)
    {
        try
        {
            await work;
        }
        catch (Exception ex)
        {
            if (requiresShutdownProof) _shutdownFailure ??= ex;
            LoggingService.LogError($"[{nameof(NotepadsCore)}] Workspace work did not drain safely: {ex}");
        }
        finally
        {
            _pendingWork.Remove(completion.Task);
            completion.TrySetResult(null);
        }
    }

    private async Task CompleteDisposalAsync()
    {
        try
        {
            // Finishing a drop can add its unattached editor's shutdown task.
            while (_pendingWork.Count != 0) await Task.WhenAll(_pendingWork.ToArray());
            _fileOperationLifetime.Dispose();
            if (_shutdownFailure != null) throw new InvalidOperationException("Native workspace shutdown could not be proved.", _shutdownFailure);
            _disposalCompletion.TrySetResult(null);
        }
        catch (Exception ex) { _disposalCompletion.TrySetException(ex); }
    }

    private void ThrowIfDisposed()
    {
        if (_disposed) throw new ObjectDisposedException(nameof(NotepadsCore));
    }

    private void ThrowIfDisposedOrClosing()
    {
        ThrowIfDisposed();
        if (IsClosing) throw new OperationCanceledException("The view is closing.");
    }

    public bool IsClosing
    {
        get => _isClosing;
        set
        {
            if (_isClosing == value) return;
            _isClosing = value;
            if (value) _fileOperationLifetime.Cancel();
            else if (!_disposed)
            {
                _fileOperationLifetime.Dispose();
                _fileOperationLifetime = new();
            }
        }
    }

    private bool _isEditorPresentationPaused;
    public bool IsEditorPresentationPaused
    {
        get => _isEditorPresentationPaused;
        set
        {
            if (_isEditorPresentationPaused == value) return;
            _isEditorPresentationPaused = value;
            if (!value) return;
            // A background Ready editor may be in an off-tree host. A close
            // dialog can select that tab while input is disabled; defer its
            // first Loaded (.LOG and polling) until presentation resumes.
            foreach (var tab in GetTabs())
                if (tab.ReadyEditor != null && !tab.HasBeenPresented &&
                    GetTabSetsViewItem(tab)?.Content is ContentControl host)
                    host.Content = CreatePendingContent(tab);
        }
    }

    private async void ThemeSettingsService_OnAccentColorChanged(object sender, Color color)
    {
        if (_disposed) return;
        try
        {
            await _dispatcher.CallOnUIThreadAsync(() =>
            {
                if (_disposed || _sets.Items == null) return;
                foreach (SetsViewItem item in _sets.Items)
                {
                    item.Icon.Foreground = new SolidColorBrush(color);
                    item.SelectionIndicatorForeground = new SolidColorBrush(color);
                }
            });
        }
        catch (Exception ex) when (_disposed) { LoggingService.LogException(ex); }
    }

    public Guid DocumentOwnerId => _context.DocumentOwnerId;

    internal Func<Task> BeforeNewDocumentPreparation { get; set; }

    public Task<ITextEditor> CreateNewTextEditorAsync(string fileNamePlaceholder) =>
        TrackWorkAsync(CreateNewTextEditorCoreAsync(fileNamePlaceholder));

    private async Task<ITextEditor> CreateNewTextEditorCoreAsync(string fileNamePlaceholder)
    {
        var newEditor = CreateTextEditor(Guid.NewGuid(), null, fileNamePlaceholder);
        try
        {
            if (BeforeNewDocumentPreparation != null) await BeforeNewDocumentPreparation();
            await newEditor.InitializeNewAsync();
            ThrowIfDisposedOrClosing();
            return newEditor;
        }
        catch { DisposeTextEditor(newEditor); throw; }
    }

    public void OpenTextEditor(ITextEditor textEditor, int atIndex = -1)
    {
        ThrowIfDisposedOrClosing();
        ArgumentNullException.ThrowIfNull(textEditor);
        if (GetTab(textEditor) is WorkspaceTab existing) { SwitchTo(existing); return; }
        var placeholder = GetTabs().Length == 1 && atIndex == -1 && textEditor.EditingFile != null
            ? GetTabs()[0] : null;
        if (placeholder?.ReadyEditor is not ITextEditor empty || empty.EditingFile != null ||
            empty.IsModified || !empty.IsDocumentEmpty) placeholder = null;
        OpenTab(new WorkspaceTab(textEditor), atIndex);
        if (placeholder != null) TabClosing?.Invoke(this, placeholder);
    }

    public void OpenTab(WorkspaceTab tab, int atIndex = -1, bool select = true, bool allowWhileClosing = false)
    {
        ThrowIfDisposed();
        if (IsClosing && !allowWhileClosing) throw new OperationCanceledException("The view is closing.");
        ArgumentNullException.ThrowIfNull(tab);
        if (tab.State == WorkspaceTabState.Closed) throw new InvalidOperationException("The tab is closed.");
        if (ContainsTab(tab)) { if (select) SwitchTo(tab); return; }
        if (GetTabs().Any(current => current.Id == tab.Id)) throw new InvalidOperationException("Duplicate document identity.");
        var item = CreateTabSetsViewItem(tab);
        tab.StateChanged += OnTabStateChanged;
        if (atIndex < 0) _sets.Items.Add(item);
        else _sets.Items.Insert(Math.Min(atIndex, _sets.Items.Count), item);
        TabOpened?.Invoke(this, tab);
        if (tab.ReadyEditor != null)
        {
            EditorAttached?.Invoke(this, tab);
            if (!IsClosing && !IsEditorPresentationPaused) ((ContentControl)item.Content).Content = tab.ReadyEditor;
        }
        if (select && !IsClosing)
        {
            _sets.SelectedItem = item;
            if (atIndex < 0) _sets.ScrollToLastSet();
        }
    }

    public void OpenTabs(WorkspaceTab[] tabs, Guid? selectedTabId = null, bool allowWhileClosing = false)
    {
        ThrowIfDisposed();
        if (IsClosing && !allowWhileClosing) throw new OperationCanceledException("The view is closing.");
        foreach (var tab in tabs) OpenTab(tab, select: false, allowWhileClosing: allowWhileClosing);
        var selected = GetTabs().FirstOrDefault(tab => tab.Id == selectedTabId) ?? tabs.LastOrDefault();
        if (selected != null) SwitchTo(selected);
    }

    public bool AttachEditor(WorkspaceTab tab, ITextEditor editor, bool showEditor = true)
    {
        if (_disposed || !ContainsTab(tab) || !tab.TrySetReady(editor)) return false;
        // Subscribers bind revision and persistence events before the first visual Loaded.
        EditorAttached?.Invoke(this, tab);
        if (showEditor && !IsClosing && !IsEditorPresentationPaused && GetTabSetsViewItem(tab)?.Content is ContentControl host)
            host.Content = editor;
        tab.CompleteLoading();
        return true;
    }

    public void ResumeEditorPresentation()
    {
        if (_disposed || IsClosing || IsEditorPresentationPaused) return;
        foreach (var tab in GetTabs())
            if (tab.ReadyEditor is ITextEditor editor && GetTabSetsViewItem(tab)?.Content is ContentControl host &&
                !ReferenceEquals(host.Content, editor)) host.Content = editor;
    }

    public Task<ITextEditor> CreateTextEditorAsync(Guid id, StorageFile file, Encoding encoding = null) =>
        TrackWorkAsync(CreateTextEditorCoreAsync(id, file, encoding));

    private async Task<ITextEditor> CreateTextEditorCoreAsync(
        Guid id,
        StorageFile file,
        Encoding encoding = null)
    {
        ThrowIfDisposedOrClosing();
        var options = new DocumentLoadOptions(encoding, ApplicationPreferences.EditorDefaultDecoding);
        using var snapshot = await DocumentTextPipeline.DecodeFileAsync(file, options, DocumentOwnerId);
        var editor = CreateTextEditor(id, file, file.Name);
        try { await editor.InitAsync(snapshot, file); ThrowIfDisposedOrClosing(); return editor; }
        catch { DisposeTextEditor(editor); throw; }
    }

    public ITextEditor CreateTextEditor(
        Guid id,
        StorageFile editingFile,
        string fileNamePlaceholder,
        bool allowWhileClosing = false)
    {
        ThrowIfDisposed();
        if (IsClosing && !allowWhileClosing) throw new OperationCanceledException("The view is closing.");
        ITextEditor textEditor = new TextEditor
        {
            Id = id,
            DocumentOwnerId = DocumentOwnerId,
            ExtensionProvider = _extensionProvider,
            FileNamePlaceholder = fileNamePlaceholder
        };

        textEditor.InitEmpty(ApplicationPreferences.EditorDefaultEncoding,
            ApplicationPreferences.EditorDefaultLineEnding, editingFile);
        textEditor.Loaded += TextEditor_Loaded;
        textEditor.SelectionChanged += TextEditor_OnSelectionChanged;
        textEditor.FontZoomFactorChanged += TextEditor_OnFontZoomFactorChanged;
        textEditor.LanguageChanged += TextEditor_OnLanguageChanged;
        textEditor.KeyDown += TextEditor_OnKeyDown;
        textEditor.ModificationStateChanged += TextEditor_OnEditorModificationStateChanged;
        textEditor.FileModificationStateChanged += TextEditor_OnFileModificationStateChanged;
        textEditor.LineEndingChanged += TextEditor_OnLineEndingChanged;
        textEditor.EncodingChanged += TextEditor_OnEncodingChanged;
        textEditor.FileRenamed += TextEditor_OnFileRenamed;
        textEditor.DisplayTitleChanged += TextEditor_OnDisplayTitleChanged;
        _ownedEditors.Add(textEditor);

        return textEditor;
    }

    public async Task SaveContentToFileAndUpdateEditorStateAsync(ITextEditor textEditor, StorageFile file)
    {
        await textEditor.SaveContentToFileAndUpdateEditorStateAsync(file); // Will throw if not succeeded
        if (_disposed || GetTextEditorSetsViewItem(textEditor) == null) return;
        if (textEditor.IsModified) MarkTextEditorSetNotSaved(textEditor);
        else MarkTextEditorSetSaved(textEditor);
        TextEditorSaved?.Invoke(this, textEditor);
    }

    public void DeleteTab(WorkspaceTab tab)
    {
        if (_disposed || tab == null) return;
        var item = GetTabSetsViewItem(tab);
        if (item == null) return;
        tab.StateChanged -= OnTabStateChanged;
        tab.MarkClosed();
        item.IsEnabled = false;
        item.PrepareForClosing();
        _sets.Items.Remove(item);
        if (item.ContextFlyout is TabContextFlyout flyout) flyout.Dispose();
        TabClosed?.Invoke(this, tab);
        if (tab.AttachedEditor != null) DisposeTextEditor(tab.AttachedEditor);
    }

    private void DisposeTextEditor(ITextEditor textEditor)
    {
        textEditor.Loaded -= TextEditor_Loaded;
        textEditor.KeyDown -= TextEditor_OnKeyDown;
        textEditor.SelectionChanged -= TextEditor_OnSelectionChanged;
        textEditor.FontZoomFactorChanged -= TextEditor_OnFontZoomFactorChanged;
        textEditor.LanguageChanged -= TextEditor_OnLanguageChanged;
        textEditor.ModificationStateChanged -= TextEditor_OnEditorModificationStateChanged;
        textEditor.FileModificationStateChanged -= TextEditor_OnFileModificationStateChanged;
        textEditor.LineEndingChanged -= TextEditor_OnLineEndingChanged;
        textEditor.EncodingChanged -= TextEditor_OnEncodingChanged;
        textEditor.FileRenamed -= TextEditor_OnFileRenamed;
        textEditor.DisplayTitleChanged -= TextEditor_OnDisplayTitleChanged;
        textEditor.Dispose();
        _ownedEditors.Remove(textEditor);
        _ = TrackWorkAsync(textEditor.DisposalCompletion, requiresShutdownProof: true);
    }

    public async Task DiscardUnattachedEditorAsync(ITextEditor editor)
    {
        if (editor == null) return;
        if (GetTab(editor) != null) throw new InvalidOperationException("An attached editor must close through its tab.");
        DisposeTextEditor(editor);
        await editor.DisposalCompletion;
    }

    public int GetTabCount() => _disposed ? 0 : _sets.Items?.Count ?? 0;

    public bool TryGetSharingContent(ITextEditor textEditor, out string title, out string content)
    {
        title = textEditor?.DisplayTitle;
        content = textEditor?.GetContentForSharing();
        return !string.IsNullOrEmpty(content);
    }

    // Persisted clean metadata does not establish whether the file grant is still usable.
    public bool HasTabsRequiringSave() => GetTabs().Any(tab => tab.ReadyEditor == null || tab.IsModified);

    public bool HasPrintableTabs() => GetTabs().Any(tab => tab.ReadyEditor == null || !tab.ReadyEditor.IsDocumentEmpty);

    public void ChangeLineEnding(ITextEditor textEditor, LineEnding lineEnding)
    {
        textEditor.TryChangeLineEnding(lineEnding);
    }

    public void SwitchTo(bool next)
    {
        if (_sets.Items == null) return;
        if (_sets.Items.Count < 2) return;

        var setsCount = _sets.Items.Count;
        var selected = _sets.SelectedIndex;

        if (next && setsCount > 1)
        {
            if (selected == setsCount - 1)
            {
                _sets.SelectedIndex = 0;
            }
            else
            {
                _sets.SelectedIndex += 1;
            }
        }
        else if (!next && setsCount > 1)
        {
            if (selected == 0)
            {
                _sets.SelectedIndex = setsCount - 1;
            }
            else
            {
                _sets.SelectedIndex -= 1;
            }
        }
    }

    public void SwitchTo(int index)
    {
        if (_sets.Items == null || index < 0 || index >= _sets.Items.Count) return;
        _sets.SelectedIndex = index;
    }

    public void SwitchTo(ITextEditor textEditor)
    {
        var item = GetTextEditorSetsViewItem(textEditor);
        if (item == null) return;
        if (!ReferenceEquals(_sets.SelectedItem, item))
        {
            _sets.SelectedItem = item;
            _sets.ScrollSetIntoView(item);
        }
    }

    public void SwitchTo(WorkspaceTab tab)
    {
        var item = GetTabSetsViewItem(tab);
        if (item == null || ReferenceEquals(_sets.SelectedItem, item)) return;
        _sets.SelectedItem = item;
        _sets.ScrollSetIntoView(item);
    }

    public WorkspaceTab GetSelectedTab() => _disposed ? null : (_sets.SelectedItem as SetsViewItem)?.Tag as WorkspaceTab;

    public ITextEditor GetSelectedReadyEditor() => GetSelectedTab()?.ReadyEditor;

    public WorkspaceTab[] GetTabs() => _disposed || _sets.Items == null ? [] :
        _sets.Items.Cast<SetsViewItem>().Select(item => (WorkspaceTab)item.Tag).ToArray();

    public WorkspaceTab GetTab(ITextEditor editor) => editor == null ? null :
        GetTabs().FirstOrDefault(tab => ReferenceEquals(tab.ReadyEditor, editor));

    public bool ContainsTab(WorkspaceTab tab) => tab != null && GetTabSetsViewItem(tab) != null;

    public void FocusOnSelectedTextEditor() => FocusOnTextEditor(GetSelectedReadyEditor());

    public void FocusOnTextEditor(ITextEditor textEditor)
    {
        if (!_disposed && !IsClosing && !IsEditorPresentationPaused) textEditor?.Focus();
    }

    public void CloseTab(WorkspaceTab tab) => GetTabSetsViewItem(tab)?.Close();

    public double GetTabScrollViewerHorizontalOffset()
    {
        return _sets.ScrollViewerHorizontalOffset;
    }

    public void SetTabScrollViewerHorizontalOffset(double offset)
    {
        _sets.ScrollTo(offset);
    }

    private SetsViewItem CreateTabSetsViewItem(WorkspaceTab tab)
    {
        var item = new SetsViewItem
        {
            Tag = tab,
            CloseButtonAutomationName = _resourceLoader.GetString("Tab_ContextFlyout_CloseButtonDisplayText"),
            Content = new ContentControl
            {
                HorizontalContentAlignment = HorizontalAlignment.Stretch,
                VerticalContentAlignment = VerticalAlignment.Stretch,
                Content = CreatePendingContent(tab)
            },
            SelectionIndicatorForeground = new SolidColorBrush(ThemeSettingsService.AppAccentColor),
            Icon = new FontIcon
            {
                Glyph = "\uF127", FontSize = 1.5, Width = 3, Height = 3,
                Foreground = new SolidColorBrush(ThemeSettingsService.AppAccentColor),
                Visibility = tab.IsModified ? Visibility.Visible : Visibility.Collapsed
            },
            ContextFlyout = new TabContextFlyout(this, tab, _renameTextEditorAsync)
        };
        SetTabTitle(item, tab.Title);
        return item;
    }

    private UIElement CreatePendingContent(WorkspaceTab tab) => tab.State != WorkspaceTabState.Failed ?
        new DocumentLoadingPlaceholder(tab.RecoverySource?.DocumentByteLength,
            tab.RecoverySource?.Metadata?.RightToLeftReadingOrder == true) : new TextBlock
    {
        Text = _resourceLoader.GetString("WorkspaceTab_LoadFailed"),
        HorizontalAlignment = HorizontalAlignment.Center,
        VerticalAlignment = VerticalAlignment.Center,
        TextWrapping = TextWrapping.Wrap,
        Margin = new Thickness(24)
    };

    private SetsViewItem GetTabSetsViewItem(WorkspaceTab tab) => _disposed || tab == null || _sets.Items == null ? null :
        _sets.Items.Cast<SetsViewItem>().FirstOrDefault(item => ReferenceEquals(item.Tag, tab));

    private SetsViewItem GetTextEditorSetsViewItem(ITextEditor textEditor) => GetTabSetsViewItem(GetTab(textEditor));

    private void OnTabStateChanged(object sender, EventArgs e)
    {
        if (sender is not WorkspaceTab tab || GetTabSetsViewItem(tab) is not SetsViewItem item) return;
        SetTabTitle(item, tab.Title);
        item.Icon.Visibility = tab.IsModified ? Visibility.Visible : Visibility.Collapsed;
        if (tab.ReadyEditor == null && item.Content is ContentControl host &&
            (tab.State == WorkspaceTabState.Failed || host.Content is not DocumentLoadingPlaceholder))
            host.Content = CreatePendingContent(tab);
        TabStateChanged?.Invoke(this, tab);
    }

    private void SetsView_OnSelectionChanged(object sender, SelectionChangedEventArgs e) =>
        TabSelected?.Invoke(this, GetSelectedTab());

    private void MarkTextEditorSetNotSaved(ITextEditor textEditor)
    {
        if (textEditor == null) return;
        var item = GetTextEditorSetsViewItem(textEditor);
        if (item != null)
        {
            item.Icon.Visibility = Visibility.Visible;
        }
    }

    private void MarkTextEditorSetSaved(ITextEditor textEditor)
    {
        if (textEditor == null) return;
        var item = GetTextEditorSetsViewItem(textEditor);
        if (item != null)
        {
            if (textEditor.EditingFileName != null)
            {
                SetTabTitle(item, textEditor.DisplayTitle);
            }
            item.Icon.Visibility = Visibility.Collapsed;
        }
    }

    private void SetsView_OnSetTapped(object sender, SetSelectedEventArgs e)
    {
        FocusOnSelectedTextEditor();
    }

    private void TextEditor_OnKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (!_disposed) TextEditorKeyDown?.Invoke(sender, e);
    }

    private void SetsView_OnSetClosing(object sender, SetClosingEventArgs e)
    {
        if (e.Set.Tag is not WorkspaceTab tab) return;
        e.Cancel = true;
        TabClosing?.Invoke(this, tab);
    }

    private void TextEditor_Loaded(object sender, RoutedEventArgs e)
    {
        if (sender is not ITextEditor textEditor) return;
        if (GetTab(textEditor) is WorkspaceTab tab) tab.HasBeenPresented = true;
        TextEditorLoaded?.Invoke(this, textEditor);
    }

    private void TextEditor_OnSelectionChanged(object sender, EventArgs e)
    {
        if (sender is not ITextEditor textEditor) return;
        TextEditorSelectionChanged?.Invoke(this, textEditor);
    }

    private void TextEditor_OnFontZoomFactorChanged(object sender, EventArgs e)
    {
        if (sender is not ITextEditor textEditor) return;
        TextEditorFontZoomFactorChanged?.Invoke(this, textEditor);
    }

    private void TextEditor_OnEditorModificationStateChanged(object sender, EventArgs e)
    {
        if (sender is not ITextEditor textEditor) return;
        if (textEditor.IsModified)
        {
            MarkTextEditorSetNotSaved(textEditor);
        }
        else
        {
            MarkTextEditorSetSaved(textEditor);
        }
        TextEditorEditorModificationStateChanged?.Invoke(this, textEditor);
    }

    private void TextEditor_OnFileModificationStateChanged(object sender, EventArgs e)
    {
        if (sender is not ITextEditor textEditor) return;
        TextEditorFileModificationStateChanged?.Invoke(this, textEditor);
    }

    private void TextEditor_OnEncodingChanged(object sender, EventArgs e)
    {
        if (sender is not ITextEditor textEditor) return;
        TextEditorEncodingChanged?.Invoke(this, textEditor);
    }

    private void TextEditor_OnLineEndingChanged(object sender, EventArgs e)
    {
        if (sender is not ITextEditor textEditor) return;
        TextEditorLineEndingChanged?.Invoke(this, textEditor);
    }

    private void TextEditor_OnLanguageChanged(object sender, EventArgs e)
    {
        if (sender is ITextEditor editor) TextEditorLanguageChanged?.Invoke(this, editor);
    }

    private void TextEditor_OnFileRenamed(object sender, EventArgs e)
    {
        if (sender is not ITextEditor textEditor) return;
        var item = GetTextEditorSetsViewItem(textEditor);
        if (item == null) return;
        SetTabTitle(item, textEditor.DisplayTitle);
        TextEditorRenamed?.Invoke(this, textEditor);
    }

    private void TextEditor_OnDisplayTitleChanged(object sender, EventArgs e)
    {
        if (sender is not ITextEditor editor || GetTextEditorSetsViewItem(editor) is not SetsViewItem item) return;
        SetTabTitle(item, editor.DisplayTitle);
        TextEditorTitleChanged?.Invoke(this, editor);
    }

    private static void SetTabTitle(SetsViewItem item, string title)
    {
        item.Header = title;
        AutomationProperties.SetName(item, title ?? string.Empty);
    }

    #region DragAndDrop

    private static ITextEditor GetDraggedEditor(object item)
    {
        return (item as WorkspaceTab)?.ReadyEditor ?? ((item as SetsViewItem)?.Tag as WorkspaceTab)?.ReadyEditor;
    }

    private bool IsLocalTransfer(DataPackageView data)
    {
        return data.Properties.TryGetValue(TextEditorTransfer.InstanceProperty, out var instance) &&
            string.Equals(instance as string, _context.InstanceId.ToString(), StringComparison.Ordinal);
    }

    private async void Sets_DragOver(object sender, DragEventArgs args)
    {
        if (_disposed || IsClosing || args.DataView == null || IsLocalTransfer(args.DataView)) return;
        var deferral = args.GetDeferral();
        try
        {
            bool transfer = args.DataView.Contains(TextEditorTransfer.DataFormat);
            bool files = !transfer && args.DataView.Contains(StandardDataFormats.StorageItems) &&
                (await args.DataView.GetStorageItemsAsync()).Any(item => item is StorageFile);
            if (!transfer && !files) return;
            args.Handled = true;
            args.AcceptedOperation = transfer ? DataPackageOperation.Move : DataPackageOperation.Link;
            if (args.DragUIOverride != null)
            {
                args.DragUIOverride.Caption = _resourceLoader.GetString(transfer ?
                    "App_DragAndDrop_UIOverride_Caption_MoveTabHere" : "App_DragAndDrop_UIOverride_Caption_OpenWithNotepads");
                args.DragUIOverride.IsCaptionVisible = true;
                args.DragUIOverride.IsGlyphVisible = false;
            }
        }
        catch (Exception ex) { LoggingService.LogException(ex); }
        finally { deferral.Complete(); }
    }

    private void Sets_DragItemsStarting(object sender, DragItemsStartingEventArgs args)
    {
        if (_disposed || IsClosing) { args.Cancel = true; return; }
        var editor = GetDraggedEditor(args.Items.FirstOrDefault());
        if (editor == null) return;
        TextEditorTransfer transfer = null;
        try
        {
            transfer = new TextEditorTransfer(editor, _context);
            var captured = transfer;
            transfer.Populate(args.Data, async received =>
            {
                try
                {
                    await _dispatcher.CallOnUIThreadAsync(async () =>
                        await CompleteTransferAsync(captured, received));
                }
                catch (Exception ex) { LoggingService.LogException(ex); }
            });
            _transfers.Add(editor.Id, transfer);
        }
        catch (Exception ex)
        {
            args.Cancel = true;
            LoggingService.LogException(ex);
            if (transfer != null) _ = TrackWorkAsync(transfer.CompleteAsync(TextEditorTransferResult.Cancelled, true));
        }
    }

    private Task CompleteTransferAsync(TextEditorTransfer transfer, bool received) =>
        TrackWorkAsync(CompleteTransferCoreAsync(transfer, received));

    private async Task CompleteTransferCoreAsync(TextEditorTransfer transfer, bool received)
    {
        if (!_transfers.TryGetValue(transfer.Editor.Id, out var active) || active != transfer) return;
        _transfers.Remove(transfer.Editor.Id);
        bool alive = GetTab(transfer.Editor) != null;
        var result = received ? TextEditorTransferResult.UnverifiedMove : TextEditorTransferResult.Cancelled;
        try
        {
            if (!_disposed && received && transfer.HasPublishedData && await transfer.HasReceiptAsync())
            {
                if (await transfer.TryCommitSourceMoveAsync(
                    () => !_disposed && !IsClosing && GetTab(transfer.Editor) != null,
                    () => TextEditorMovedToAnotherAppInstance?.Invoke(this, transfer.Editor)))
                {
                    result = TextEditorTransferResult.Received;
                }

                alive = !_disposed && GetTab(transfer.Editor) != null;
            }
        }
        catch (Exception ex) { LoggingService.LogException(ex); }
        finally { await transfer.CompleteAsync(result, alive); }
    }

    private int GetDropIndex(SetsView sets, DragEventArgs args)
    {
        for (int index = 0; index < sets.Items.Count; index++)
        {
            if (sets.ContainerFromIndex(index) is SetsViewItem item && args.GetPosition(item).X < item.ActualWidth)
                return index;
        }
        return sets.Items.Count;
    }

    private async void Sets_Drop(object sender, DragEventArgs args)
    {
        if (_disposed || IsClosing || sender is not SetsView sets || args.DataView == null || IsLocalTransfer(args.DataView)) return;
        try { await TrackWorkAsync(DropAsync(sets, args)); }
        catch (Exception ex) { LoggingService.LogException(ex); }
    }

    private async Task DropAsync(SetsView sets, DragEventArgs args)
    {
        var deferral = args.GetDeferral();
        IDisposable sourcePin = null;
        bool published = false;
        try
        {
            if (!args.DataView.Contains(TextEditorTransfer.DataFormat))
            {
                if (args.DataView.Contains(StandardDataFormats.StorageItems))
                {
                    var items = await args.DataView.GetStorageItemsAsync();
                    if (!_disposed && !IsClosing) StorageItemsDropped?.Invoke(this, items);
                }
                return;
            }
            args.Handled = true;
            using var lifetime = CaptureFileOperation();
            var expectedTarget = _context.Sessions.CurrentStamp;
            var serialized = await args.DataView.GetDataAsync(TextEditorTransfer.DataFormat) as string;
            var token = JsonSerializer.Deserialize<TransferToken>(serialized ?? throw new InvalidOperationException("Missing transfer token."), SessionJsonContext.Default.TransferToken);
            if (token == null ||
                !args.DataView.Properties.TryGetValue(TextEditorTransfer.TransferProperty, out var transferValue) ||
                !Guid.TryParse(transferValue as string, out var transferId) || transferId != token.TransferId ||
                !args.DataView.Properties.TryGetValue(TextEditorTransfer.InstanceProperty, out var sourceValue) ||
                !Guid.TryParse(sourceValue as string, out var sourceId) || sourceId != token.SourceInstanceId)
            {
                throw new InvalidOperationException("Invalid transfer identity or metadata.");
            }

            token.Validate();
            TextEditorSessionDataV2 data;
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                sourcePin = SessionScopeLease.AcquireReader(token.SourceOwnerId);
                data = await TransferRootReader.ReadSourceTransferAsync(token);
            }
            StorageFile editingFile = null;
            if (data.StateMetaData.HasEditingFile)
            {
                var items = await args.DataView.GetStorageItemsAsync();
                if (items.Count != 1 || items[0] is not StorageFile file) throw new InvalidOperationException("Missing transferred file permission.");
                if (!string.Equals(file.Path, data.EditingFilePath, StringComparison.OrdinalIgnoreCase) ||
                    !string.Equals(file.Name, data.EditingFileName, StringComparison.Ordinal))
                {
                    throw new InvalidOperationException("The transferred file permission does not match its source descriptor.");
                }

                editingFile = file;
            }
            var result = await OpenTransferredFileAsync(editingFile,
                () => PrepareTransferredEditorAsync(data, editingFile),
                editor => ProtectTransferredEditorAsync(editor, token, expectedTarget),
                GetDropIndex(sets, args), lifetime.Token);
            if (result.Kind == WorkspaceFileOpenKind.Existing)
            {
                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_FileAlreadyOpened"), 2500);
                return;
            }
            if (result.Kind == WorkspaceFileOpenKind.Failed) throw result.Error;
            if (result.Kind != WorkspaceFileOpenKind.Created) return;
            published = true;
            args.DataView.ReportOperationCompleted(DataPackageOperation.Move);
            if (!_disposed && !IsClosing && result.Tab.ReadyEditor is ITextEditor candidate)
            {
                if (data.StateMetaData.IsContentPreviewPanelOpened) candidate.ShowHideContentPreview();
                if (data.StateMetaData.IsInDiffPreviewMode) await candidate.OpenSideBySideDiffViewerAsync();
            }
            AnalyticsService.TrackEvent("OnSetDropped");
        }
        catch (Exception ex) { LoggingService.LogException(ex); }
        finally
        {
            if (!published)
            {
                if (args.DataView.Contains(TextEditorTransfer.DataFormat)) args.DataView.ReportOperationCompleted(DataPackageOperation.None);
            }
            try { sourcePin?.Dispose(); }
            finally { deferral.Complete(); }
        }
    }

    private async void Sets_DragItemsCompleted(ListViewBase sender, DragItemsCompletedEventArgs args)
    {
        var editor = GetDraggedEditor(args.Items.FirstOrDefault());
        if (editor == null || !_transfers.TryGetValue(editor.Id, out var transfer)) return;
        // Same-window reorder does not request our recovery descriptor and
        // retains the existing native document and undo history.
        if (args.DropResult == DataPackageOperation.None || !transfer.HasPublishedData)
        {
            try { await CompleteTransferAsync(transfer, false); }
            catch (Exception ex) { LoggingService.LogException(ex); }
        }
    }

    private async void Sets_SetDraggedOutside(object sender, SetDraggedOutsideEventArgs e)
    {
        if (_sets.Items?.Count > 1 && (e.Set?.Tag as WorkspaceTab)?.ReadyEditor is ITextEditor textEditor)
        {
            // Only allow untitled empty document to be dragged outside for now
            if (!textEditor.IsModified && textEditor.EditingFile == null && textEditor.IsDocumentEmpty)
            {
                TabClosing?.Invoke(this, GetTab(textEditor));
                await NotepadsProtocolService.LaunchProtocolAsync(NotepadsOperationProtocol.OpenNewInstance);
            }
        }
    }

    #endregion
}
