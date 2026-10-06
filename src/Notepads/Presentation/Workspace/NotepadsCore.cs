// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Text.Json;
using System.Threading.Tasks;
using Notepads.Controls;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.IO;
using Notepads.Features.Documents.Text;
using Notepads.Features.Preferences;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Storage;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Threading;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Helpers;
using Notepads.Presentation.PreviewExtensions;
using Notepads.Presentation.Theming;
using Notepads.Presentation.Workspace.Activation;
using Windows.ApplicationModel.DataTransfer;
using Windows.ApplicationModel.Resources;
using Windows.Foundation.Collections;
using Windows.Storage;
using Windows.UI;
using Windows.UI.Core;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Media;
using WinUIEditor;

namespace Notepads.Presentation.Workspace;

public sealed partial class NotepadsCore : INotepadsCore
{
    public event EventHandler<ITextEditor> TextEditorLoaded;
    public event EventHandler<ITextEditor> TextEditorUnloaded;
    public event EventHandler<ITextEditor> TextEditorOpened;
    public event EventHandler<ITextEditor> TextEditorClosed;
    public event EventHandler<ITextEditor> TextEditorEditorModificationStateChanged;
    public event EventHandler<ITextEditor> TextEditorFileModificationStateChanged;
    public event EventHandler<ITextEditor> TextEditorSaved;
    public event EventHandler<ITextEditor> TextEditorRenamed;
    public event EventHandler<ITextEditor> TextEditorClosing;
    public event EventHandler<ITextEditor> TextEditorSelectionChanged;
    public event EventHandler<ITextEditor> TextEditorFontZoomFactorChanged;
    public event EventHandler<ITextEditor> TextEditorLanguageChanged;
    public event EventHandler<ITextEditor> TextEditorEncodingChanged;
    public event EventHandler<ITextEditor> TextEditorLineEndingChanged;
    public event EventHandler<ITextEditor> TextEditorModeChanged;
    public event EventHandler<ITextEditor> TextEditorMovedToAnotherAppInstance;
    public event EventHandler<IReadOnlyList<IStorageItem>> StorageItemsDropped;

    public event KeyEventHandler TextEditorKeyDown;

    private readonly SetsView _sets;

    private readonly INotepadsExtensionProvider _extensionProvider;

    private ITextEditor _selectedTextEditor;

    private ITextEditor[] _allTextEditors;

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

    public NotepadsCore(SetsView sets,
        INotepadsExtensionProvider extensionProvider,
        CoreDispatcher dispatcher, WindowContext context)
    {
        _sets = sets;
        _sets.SelectionChanged += SetsView_OnSelectionChanged;
        _sets.Items.VectorChanged += SetsView_OnItemsChanged;
        _sets.SetClosing += SetsView_OnSetClosing;
        _sets.SetTapped += SetsView_OnSetTapped;
        _sets.SetDraggedOutside += Sets_SetDraggedOutside;
        _sets.DragOver += Sets_DragOver;
        _sets.Drop += Sets_Drop;
        _sets.DragItemsStarting += Sets_DragItemsStarting;
        _sets.DragItemsCompleted += Sets_DragItemsCompleted;

        _dispatcher = dispatcher;
        _context = context ?? throw new ArgumentNullException(nameof(context));
        _extensionProvider = extensionProvider;

        ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;
    }

    public void Dispose()
    {
        if (_disposed) return;
        var items = _sets.Items?.Cast<SetsViewItem>().ToArray() ?? [];
        _disposed = true;
        ThemeSettingsService.OnAccentColorChanged -= ThemeSettingsService_OnAccentColorChanged;
        _sets.SelectionChanged -= SetsView_OnSelectionChanged;
        if (_sets.Items != null) _sets.Items.VectorChanged -= SetsView_OnItemsChanged;
        _sets.SetClosing -= SetsView_OnSetClosing;
        _sets.SetTapped -= SetsView_OnSetTapped;
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
            try { if (item.ContextFlyout is TabContextFlyout flyout) flyout.Dispose(); }
            catch (Exception ex) { LoggingService.LogException(ex); }
        }
        foreach (var editor in _ownedEditors.ToArray())
        {
            try { DisposeTextEditor(editor); }
            catch (Exception ex) { _shutdownFailure ??= ex; LoggingService.LogException(ex); }
        }
        _selectedTextEditor = null;
        _allTextEditors = [];
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

    public bool IsClosing { get; set; }

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

    public Task<ITextEditor> CreateNewTextEditorAsync(string fileNamePlaceholder) =>
        TrackWorkAsync(CreateNewTextEditorCoreAsync(fileNamePlaceholder));

    private async Task<ITextEditor> CreateNewTextEditorCoreAsync(string fileNamePlaceholder)
    {
        var newEditor = CreateTextEditor(Guid.NewGuid(), null, fileNamePlaceholder);
        try
        {
            await newEditor.InitializeNewAsync();
            ThrowIfDisposedOrClosing();
            return newEditor;
        }
        catch { DisposeTextEditor(newEditor); throw; }
    }

    public void OpenTextEditor(ITextEditor textEditor, int atIndex = -1)
    {
        ThrowIfDisposedOrClosing();
        if (textEditor == null) throw new ArgumentNullException(nameof(textEditor));
        if (GetTextEditorSetsViewItem(textEditor) != null)
        {
            SwitchTo(textEditor);
            return;
        }
        SetsViewItem textEditorSetsViewItem = CreateTextEditorSetsViewItem(textEditor);
        ITextEditor placeholderToReplace = null;

        // Notepads should replace current "Untitled.txt" with open file if it is empty and it is the only tab that has been created.
        // If index != -1, it means set was created after a drag and drop, we should skip this logic
        if (GetNumberOfOpenedTextEditors() == 1 && textEditor.EditingFile != null && atIndex == -1)
        {
            var selectedEditor = GetAllTextEditors().First();
            if (selectedEditor.EditingFile == null && !selectedEditor.IsModified && selectedEditor.IsDocumentEmpty)
            {
                placeholderToReplace = selectedEditor;
            }
        }

        if (atIndex == -1)
        {
            if (_sets.Items == null) return;
            _sets.Items.Add(textEditorSetsViewItem);
        }
        else
        {
            if (_sets.Items == null) return;
            _sets.Items.Insert(atIndex, textEditorSetsViewItem);
        }

        TextEditorOpened?.Invoke(this, textEditor);
        // Keep a live tab throughout replacement and close the old document
        // through its normal lifetime path, including disposal and subscribers.
        if (placeholderToReplace != null) TextEditorClosing?.Invoke(this, placeholderToReplace);

        if (GetNumberOfOpenedTextEditors() > 1)
        {
            _sets.SelectedItem = textEditorSetsViewItem;
            if (atIndex == -1)
            {
                _sets.ScrollToLastSet();
            }
        }
    }

    public void OpenTextEditors(ITextEditor[] editors, Guid? selectedEditorId = null)
    {
        ThrowIfDisposedOrClosing();
        bool selectedEditorFound = false;

        foreach (var textEditor in editors)
        {
            if (GetTextEditorSetsViewItem(textEditor) != null) continue;
            var editorSetsViewItem = CreateTextEditorSetsViewItem(textEditor);
            if (_sets.Items == null) return;
            _sets.Items.Add(editorSetsViewItem);
            TextEditorOpened?.Invoke(this, textEditor);
            if (selectedEditorId.HasValue && textEditor.Id == selectedEditorId.Value)
            {
                _sets.SelectedItem = editorSetsViewItem;
                selectedEditorFound = true;
            }
        }

        if (selectedEditorId == null || !selectedEditorFound)
        {
            _sets.SelectedIndex = editors.Length - 1;
            _sets.ScrollToLastSet();
        }
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
        using (var snapshot = await DocumentTextPipeline.DecodeFileAsync(file, options, DocumentOwnerId))
        {
            var editor = CreateTextEditor(id, file, file.Name);
            try { await editor.InitAsync(snapshot, file); ThrowIfDisposedOrClosing(); return editor; }
            catch { DisposeTextEditor(editor); throw; }
        }
    }

    public ITextEditor CreateTextEditor(
        Guid id,
        StorageFile editingFile,
        string fileNamePlaceholder)
    {
        ThrowIfDisposedOrClosing();
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
        textEditor.Unloaded += TextEditor_Unloaded;
        textEditor.SelectionChanged += TextEditor_OnSelectionChanged;
        textEditor.FontZoomFactorChanged += TextEditor_OnFontZoomFactorChanged;
        textEditor.LanguageChanged += TextEditor_OnLanguageChanged;
        textEditor.KeyDown += TextEditor_OnKeyDown;
        textEditor.ModificationStateChanged += TextEditor_OnEditorModificationStateChanged;
        textEditor.ModeChanged += TextEditor_OnModeChanged;
        textEditor.FileModificationStateChanged += TextEditor_OnFileModificationStateChanged;
        textEditor.LineEndingChanged += TextEditor_OnLineEndingChanged;
        textEditor.EncodingChanged += TextEditor_OnEncodingChanged;
        textEditor.FileRenamed += TextEditor_OnFileRenamed;
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

    public void DeleteTextEditor(ITextEditor textEditor)
    {
        RemoveTextEditor(textEditor);
    }

    private void RemoveTextEditor(ITextEditor textEditor)
    {
        if (_disposed || textEditor == null) return;
        var item = GetTextEditorSetsViewItem(textEditor);
        if (item == null) return;
        item.IsEnabled = false;
        item.PrepareForClosing();
        _sets.Items?.Remove(item);

        if (item.ContextFlyout is TabContextFlyout tabContextFlyout)
        {
            tabContextFlyout.Dispose();
        }

        TextEditorClosed?.Invoke(this, textEditor);

        DisposeTextEditor(textEditor);
        // Explicit closure or source-move authority has already committed.
        // Recovery retirement is awaited by the session/transfer protocol.
    }

    private void DisposeTextEditor(ITextEditor textEditor)
    {
        textEditor.Loaded -= TextEditor_Loaded;
        textEditor.Unloaded -= TextEditor_Unloaded;
        textEditor.KeyDown -= TextEditor_OnKeyDown;
        textEditor.SelectionChanged -= TextEditor_OnSelectionChanged;
        textEditor.FontZoomFactorChanged -= TextEditor_OnFontZoomFactorChanged;
        textEditor.LanguageChanged -= TextEditor_OnLanguageChanged;
        textEditor.ModificationStateChanged -= TextEditor_OnEditorModificationStateChanged;
        textEditor.ModeChanged -= TextEditor_OnModeChanged;
        textEditor.FileModificationStateChanged -= TextEditor_OnFileModificationStateChanged;
        textEditor.LineEndingChanged -= TextEditor_OnLineEndingChanged;
        textEditor.EncodingChanged -= TextEditor_OnEncodingChanged;
        textEditor.FileRenamed -= TextEditor_OnFileRenamed;
        textEditor.Dispose();
        _ownedEditors.Remove(textEditor);
        _ = TrackWorkAsync(textEditor.DisposalCompletion, requiresShutdownProof: true);
    }

    public int GetNumberOfOpenedTextEditors()
    {
        return _disposed ? 0 : _sets.Items?.Count ?? 0;
    }

    public bool TryGetSharingContent(ITextEditor textEditor, out string title, out string content)
    {
        title = textEditor.EditingFileName ?? textEditor.FileNamePlaceholder;
        content = textEditor.GetContentForSharing();
        return !string.IsNullOrEmpty(content);
    }

    public bool HaveUnsavedTextEditor()
    {
        if (_sets.Items == null || _sets.Items.Count == 0) return false;
        foreach (SetsViewItem setsItem in _sets.Items)
        {
            if (setsItem.Content is not ITextEditor textEditor) continue;
            if (!textEditor.IsModified) continue;
            return true;
        }
        return false;
    }

    public bool HaveNonemptyTextEditor()
    {
        if (_sets.Items == null || _sets.Items.Count <= 1) return false;
        foreach (SetsViewItem setsItem in _sets.Items)
        {
            if (setsItem.Content is not ITextEditor textEditor) continue;
            if (textEditor.IsDocumentEmpty) continue;
            return true;
        }
        return false;
    }

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
        if (!ReferenceEquals(_sets.SelectedItem, item))
        {
            _sets.SelectedItem = item;
            _sets.ScrollIntoView(item);
        }
    }

    public ITextEditor GetSelectedTextEditor()
    {
        if (_disposed) return null;
        if (ThreadUtility.IsOnUIThread())
        {
            if (((_sets.SelectedItem as SetsViewItem)?.Content is not ITextEditor textEditor)) return null;
            return textEditor;
        }
        return _selectedTextEditor;
    }

    public ITextEditor[] GetAllTextEditors()
    {
        if (_disposed) return [];
        if (!ThreadUtility.IsOnUIThread()) return _allTextEditors;
        if (_sets.Items == null) return [];
        var editors = new List<ITextEditor>();
        foreach (SetsViewItem item in _sets.Items)
        {
            if (item.Content is ITextEditor textEditor)
            {
                editors.Add(textEditor);
            }
        }
        return editors.ToArray();
    }

    public void FocusOnSelectedTextEditor()
    {
        FocusOnTextEditor(GetSelectedTextEditor());
    }

    public void FocusOnTextEditor(ITextEditor textEditor)
    {
        if (!_disposed) textEditor?.Focus();
    }

    public void CloseTextEditor(ITextEditor textEditor)
    {
        var item = GetTextEditorSetsViewItem(textEditor);
        item?.Close();
    }

    public ITextEditor GetTextEditor(StorageFile file)
    {
        if (_disposed) return null;
        var item = GetTextEditorSetsViewItem(file);
        return item?.Content as ITextEditor;
    }

    public double GetTabScrollViewerHorizontalOffset()
    {
        return _sets.ScrollViewerHorizontalOffset;
    }

    public void SetTabScrollViewerHorizontalOffset(double offset)
    {
        _sets.ScrollTo(offset);
    }

    private SetsViewItem CreateTextEditorSetsViewItem(ITextEditor textEditor)
    {
        var modifierIcon = new FontIcon()
        {
            Glyph = "\uF127",
            FontSize = 1.5,
            Width = 3,
            Height = 3,
            Foreground = new SolidColorBrush(ThemeSettingsService.AppAccentColor),
        };

        var textEditorSetsViewItem = new SetsViewItem
        {
            Header = textEditor.EditingFileName ?? textEditor.FileNamePlaceholder,
            Content = textEditor,
            SelectionIndicatorForeground = new SolidColorBrush(ThemeSettingsService.AppAccentColor),
            Icon = modifierIcon
        };

        if (textEditorSetsViewItem.Content == null || textEditorSetsViewItem.Content is Page)
        {
            throw new Exception("Content should not be null and type should not be Page (SetsView does not work well with Page controls)");
        }

        textEditorSetsViewItem.Icon.Visibility = textEditor.IsModified ? Visibility.Visible : Visibility.Collapsed;
        textEditorSetsViewItem.ContextFlyout = new TabContextFlyout(this, textEditor);

        return textEditorSetsViewItem;
    }

    private SetsViewItem GetTextEditorSetsViewItem(StorageFile file)
    {
        if (file == null || _sets.Items == null) return null;
        foreach (SetsViewItem setsItem in _sets.Items)
        {
            if (setsItem.Content is not ITextEditor textEditor) continue;
            if (textEditor.EditingFile != null && textEditor.EditingFile.IsEqual(file))
            {
                return setsItem;
            }
        }
        return null;
    }

    private SetsViewItem GetTextEditorSetsViewItem(ITextEditor textEditor)
    {
        if (_sets.Items == null) return null;
        foreach (SetsViewItem setsItem in _sets.Items)
        {
            if (setsItem.Content is ITextEditor editor)
            {
                if (textEditor == editor) return setsItem;
            }
        }
        return null;
    }

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
                item.Header = textEditor.EditingFileName;
            }
            item.Icon.Visibility = Visibility.Collapsed;
        }
    }

    private void SetsView_OnSelectionChanged(object sender, RoutedEventArgs e)
    {
        _selectedTextEditor = GetSelectedTextEditor();
    }

    private void SetsView_OnSetTapped(object sender, SetSelectedEventArgs e)
    {
        FocusOnTextEditor(e.Item as ITextEditor);
    }

    private void TextEditor_OnKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (!_disposed) TextEditorKeyDown?.Invoke(sender, e);
    }

    private void SetsView_OnItemsChanged(object sender, IVectorChangedEventArgs e)
    {
        _allTextEditors = GetAllTextEditors();
    }

    private void SetsView_OnSetClosing(object sender, SetClosingEventArgs e)
    {
        if (e.Set.Content is not ITextEditor textEditor) return;

        if (TextEditorClosing != null)
        {
            e.Cancel = true;
            TextEditorClosing.Invoke(this, textEditor);
        }
    }

    private void TextEditor_Loaded(object sender, RoutedEventArgs e)
    {
        if (sender is not ITextEditor textEditor) return;
        TextEditorLoaded?.Invoke(this, textEditor);
    }

    private void TextEditor_Unloaded(object sender, RoutedEventArgs e)
    {
        if (sender is not ITextEditor textEditor) return;
        TextEditorUnloaded?.Invoke(this, textEditor);
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

    private void TextEditor_OnModeChanged(object sender, EventArgs e)
    {
        if (sender is not ITextEditor textEditor) return;
        TextEditorModeChanged?.Invoke(this, textEditor);
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
        item.Header = textEditor.EditingFileName ?? textEditor.FileNamePlaceholder;
        TextEditorRenamed?.Invoke(this, textEditor);
    }

    #region DragAndDrop

    private static ITextEditor GetDraggedEditor(object item)
    {
        return item as ITextEditor ?? (item as SetsViewItem)?.Content as ITextEditor;
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
        bool alive = GetAllTextEditors().Contains(transfer.Editor);
        var result = received ? TextEditorTransferResult.UnverifiedMove : TextEditorTransferResult.Cancelled;
        try
        {
            if (!_disposed && received && transfer.HasPublishedData && await transfer.HasReceiptAsync())
            {
                if (await transfer.TryCommitSourceMoveAsync(
                    () => !_disposed && !IsClosing && GetAllTextEditors().Contains(transfer.Editor),
                    () => TextEditorMovedToAnotherAppInstance?.Invoke(this, transfer.Editor)))
                {
                    result = TextEditorTransferResult.Received;
                }

                alive = !_disposed && GetAllTextEditors().Contains(transfer.Editor);
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
        ITextEditor candidate = null;
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
            if (editingFile != null && GetTextEditor(editingFile) is ITextEditor existing)
            {
                SwitchTo(existing);
                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_FileAlreadyOpened"), 2500);
                return;
            }
            using (var savedSource = await SessionDocumentStore.OpenBaselineAsync(data.SavedBaseline))
            using (var recoverySource = await SessionDocumentStore.OpenBaselineAsync(data.RecoveryBaseline))
            using (var journal = await SessionDocumentStore.OpenJournalAsync(data.Journal))
            using (var checkpoint = await journal.OpenCheckpointAsync(
                data.Journal.BaselineSequence, data.Journal.CommittedSequence,
                data.Journal.CommittedByteLength, data.Journal.PrefixSha256, data.Journal.DocumentByteLength))
            using (var saved = await SessionDocumentStore.ForkBaselineAsync(savedSource, DocumentOwnerId))
            using (var recovery = data.SavedBaseline.Matches(data.RecoveryBaseline) ?
                saved.Retain() : await SessionDocumentStore.ForkBaselineAsync(recoverySource, DocumentOwnerId))
            using (var snapshot = new DocumentSnapshot(saved,
                EncodingCatalog.GetEncodingByName(data.StateMetaData.LastSavedEncoding),
                LineEndingUtility.GetLineEndingByName(data.StateMetaData.LastSavedLineEnding), data.StateMetaData.DateModifiedFileTime))
            {
                candidate = CreateTextEditor(Guid.NewGuid(), editingFile, data.StateMetaData.FileNamePlaceholder);
                await candidate.RestoreV2Async(snapshot, recovery, journal, checkpoint,
                    editingFile, data.StateMetaData, data.TextDirty);
            }
            // The destination owns independent baselines and an imported
            // journal before the source is allowed to close its editor.
            ThrowIfDisposedOrClosing();
            await ProtectTransferredEditorAsync(candidate, token, expectedTarget);
            OpenTextEditor(candidate, GetDropIndex(sets, args));
            published = true;
            args.DataView.ReportOperationCompleted(DataPackageOperation.Move);
            if (data.StateMetaData.IsContentPreviewPanelOpened) candidate.ShowHideContentPreview();
            if (data.StateMetaData.IsInDiffPreviewMode) await candidate.OpenSideBySideDiffViewerAsync();
            AnalyticsService.TrackEvent("OnSetDropped");
        }
        catch (Exception ex) { LoggingService.LogException(ex); }
        finally
        {
            if (!published)
            {
                if (candidate != null)
                {
                    // A source may have consumed the durable receipt while
                    // this view was closing. Publication rollback is not an
                    // intentional discard of its independent recovery data.
                    if (GetAllTextEditors().Contains(candidate)) RemoveTextEditor(candidate);
                    else DisposeTextEditor(candidate);
                }
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
        if (_sets.Items?.Count > 1 && e.Set?.Content is ITextEditor textEditor)
        {
            // Only allow untitled empty document to be dragged outside for now
            if (!textEditor.IsModified && textEditor.EditingFile == null && textEditor.IsDocumentEmpty)
            {
                TextEditorClosing?.Invoke(this, textEditor);
                await NotepadsProtocolService.LaunchProtocolAsync(NotepadsOperationProtocol.OpenNewInstance);
            }
        }
    }

    #endregion
}
