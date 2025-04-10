﻿// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Core
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using System.Text;
    using System.Threading.Tasks;
    using System.Text.Json;
    using Notepads.Controls.TextEditor;
    using Notepads.Extensions;
    using Notepads.Models;
    using Notepads.Services;
    using Notepads.Settings;
    using Notepads.Utilities;
    using Notepads.Controls;
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

    public class NotepadsCore : INotepadsCore
    {
        public event EventHandler<ITextEditor> TextEditorLoaded;
        public event EventHandler<ITextEditor> TextEditorUnloaded;
        public event EventHandler<ITextEditor> TextEditorEditorModificationStateChanged;
        public event EventHandler<ITextEditor> TextEditorFileModificationStateChanged;
        public event EventHandler<ITextEditor> TextEditorSaved;
        public event EventHandler<ITextEditor> TextEditorRenamed;
        public event EventHandler<ITextEditor> TextEditorClosing;
        public event EventHandler<ITextEditor> TextEditorSelectionChanged;
        public event EventHandler<ITextEditor> TextEditorFontZoomFactorChanged;
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

        private readonly CoreDispatcher _dispatcher;

        private const string SetDragAndDropActionStatus = "SetDragAndDropActionStatus";
        private const string NotepadsTextEditorMetaData = "NotepadsTextEditorMetaData";
        private const string NotepadsTextEditorGuid = "NotepadsTextEditorGuid";
        private const string NotepadsInstanceId = "NotepadsInstanceId";
        private const string NotepadsTextEditorLastSavedContent = "NotepadsTextEditorLastSavedContent";
        private const string NotepadsTextEditorPendingContent = "NotepadsTextEditorPendingContent";
        private const string NotepadsTextEditorEditingFilePath = "NotepadsTextEditorEditingFilePath";

        public NotepadsCore(SetsView sets,
            INotepadsExtensionProvider extensionProvider,
            CoreDispatcher dispatcher)
        {
            _sets = sets;
            _sets.SelectionChanged += SetsView_OnSelectionChanged;
            _sets.Items.VectorChanged += SetsView_OnItemsChanged;
            _sets.SetClosing += SetsView_OnSetClosing;
            _sets.SetTapped += (sender, args) => { FocusOnTextEditor(args.Item as ITextEditor); };
            _sets.SetDraggedOutside += Sets_SetDraggedOutside;
            _sets.DragOver += Sets_DragOver;
            _sets.Drop += Sets_Drop;
            _sets.DragItemsStarting += Sets_DragItemsStarting;
            _sets.DragItemsCompleted += Sets_DragItemsCompleted;

            _dispatcher = dispatcher;
            _extensionProvider = extensionProvider;

            ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;
        }

        private async void ThemeSettingsService_OnAccentColorChanged(object sender, Color color)
        {
            await _dispatcher.CallOnUIThreadAsync(() =>
            {
                if (_sets.Items == null) return;
                foreach (SetsViewItem item in _sets.Items)
                {
                    item.Icon.Foreground = new SolidColorBrush(color);
                    item.SelectionIndicatorForeground = new SolidColorBrush(color);
                }
            });
        }

        public void OpenNewTextEditor(string fileNamePlaceholder)
        {
            var textFile = new TextFile(string.Empty,
                AppSettingsService.EditorDefaultEncoding,
                AppSettingsService.EditorDefaultLineEnding);
            var newEditor = CreateTextEditor(
                Guid.NewGuid(),
                textFile,
                null,
                fileNamePlaceholder);
            OpenTextEditor(newEditor);
        }

        public void OpenTextEditor(ITextEditor textEditor, int atIndex = -1)
        {
            SetsViewItem textEditorSetsViewItem = CreateTextEditorSetsViewItem(textEditor);

            // Notepads should replace current "Untitled.txt" with open file if it is empty and it is the only tab that has been created.
            // If index != -1, it means set was created after a drag and drop, we should skip this logic
            if (GetNumberOfOpenedTextEditors() == 1 && textEditor.EditingFile != null && atIndex == -1)
            {
                var selectedEditor = GetAllTextEditors().First();
                if (selectedEditor.EditingFile == null && !selectedEditor.IsModified)
                {
                    _sets.Items?.Clear();
                }
            }

            if (atIndex == -1)
            {
                _sets.Items?.Add(textEditorSetsViewItem);
            }
            else
            {
                _sets.Items?.Insert(atIndex, textEditorSetsViewItem);
            }

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
            bool selectedEditorFound = false;

            foreach (var textEditor in editors)
            {
                var editorSetsViewItem = CreateTextEditorSetsViewItem(textEditor);
                _sets.Items?.Add(editorSetsViewItem);
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

        public async Task<ITextEditor> CreateTextEditorAsync(
            Guid id,
            StorageFile file,
            Encoding encoding = null,
            bool ignoreFileSizeLimit = false)
        {
            var textFile = await FileSystemUtility.ReadFileAsync(file, ignoreFileSizeLimit, encoding);
            return CreateTextEditor(id, textFile, file, file.Name);
        }

        public ITextEditor CreateTextEditor(
            Guid id,
            TextFile textFile,
            StorageFile editingFile,
            string fileNamePlaceholder,
            bool isModified = false)
        {
            ITextEditor textEditor = new TextEditor
            {
                Id = id,
                ExtensionProvider = _extensionProvider,
                FileNamePlaceholder = fileNamePlaceholder
            };

            textEditor.Init(textFile, editingFile, isModified: isModified);
            textEditor.Loaded += TextEditor_Loaded;
            textEditor.Unloaded += TextEditor_Unloaded;
            textEditor.SelectionChanged += TextEditor_OnSelectionChanged;
            textEditor.FontZoomFactorChanged += TextEditor_OnFontZoomFactorChanged;
            textEditor.KeyDown += TextEditorKeyDown;
            textEditor.ModificationStateChanged += TextEditor_OnEditorModificationStateChanged;
            textEditor.ModeChanged += TextEditor_OnModeChanged;
            textEditor.FileModificationStateChanged += TextEditor_OnFileModificationStateChanged;
            textEditor.LineEndingChanged += TextEditor_OnLineEndingChanged;
            textEditor.EncodingChanged += TextEditor_OnEncodingChanged;
            textEditor.FileRenamed += TextEditor_OnFileRenamed;

            return textEditor;
        }

        public async Task SaveContentToFileAndUpdateEditorStateAsync(ITextEditor textEditor, StorageFile file)
        {
            await textEditor.SaveContentToFileAndUpdateEditorStateAsync(file); // Will throw if not succeeded
            MarkTextEditorSetSaved(textEditor);
            TextEditorSaved?.Invoke(this, textEditor);
        }

        public void DeleteTextEditor(ITextEditor textEditor)
        {
            if (textEditor == null) return;
            var item = GetTextEditorSetsViewItem(textEditor);
            if (item == null) return;
            item.IsEnabled = false;
            item.PrepareForClosing();
            _sets.Items?.Remove(item);

            if (item.ContextFlyout is TabContextFlyout tabContextFlyout)
            {
                tabContextFlyout.Dispose();
            }

            TextEditorUnloaded?.Invoke(this, textEditor);

            textEditor.Loaded -= TextEditor_Loaded;
            textEditor.Unloaded -= TextEditor_Unloaded;
            textEditor.KeyDown -= TextEditorKeyDown;
            textEditor.SelectionChanged -= TextEditor_OnSelectionChanged;
            textEditor.FontZoomFactorChanged -= TextEditor_OnFontZoomFactorChanged;
            textEditor.ModificationStateChanged -= TextEditor_OnEditorModificationStateChanged;
            textEditor.ModeChanged -= TextEditor_OnModeChanged;
            textEditor.FileModificationStateChanged -= TextEditor_OnFileModificationStateChanged;
            textEditor.LineEndingChanged -= TextEditor_OnLineEndingChanged;
            textEditor.EncodingChanged -= TextEditor_OnEncodingChanged;
            textEditor.FileRenamed -= TextEditor_OnFileRenamed;
            textEditor.Dispose();
        }

        public int GetNumberOfOpenedTextEditors()
        {
            return _sets.Items?.Count ?? 0;
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
                if (!(setsItem.Content is ITextEditor textEditor)) continue;
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
                if (!(setsItem.Content is ITextEditor textEditor)) continue;
                if (string.IsNullOrEmpty(textEditor.GetText())) continue;
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
            if (_sets.SelectedItem != item)
            {
                _sets.SelectedItem = item;
                _sets.ScrollIntoView(item);
            }
        }

        public ITextEditor GetSelectedTextEditor()
        {
            if (ThreadUtility.IsOnUIThread())
            {
                if ((!((_sets.SelectedItem as SetsViewItem)?.Content is ITextEditor textEditor))) return null;
                return textEditor;
            }
            return _selectedTextEditor;
        }

        public ITextEditor GetTextEditor(string editingFilePath)
        {
            if (string.IsNullOrEmpty(editingFilePath)) return null;
            return GetAllTextEditors().FirstOrDefault(editor => editor.EditingFilePath != null &&
                string.Equals(editor.EditingFilePath, editingFilePath, StringComparison.OrdinalIgnoreCase));
        }

        public ITextEditor[] GetAllTextEditors()
        {
            if (!ThreadUtility.IsOnUIThread()) return _allTextEditors;
            if (_sets.Items == null) return Array.Empty<ITextEditor>();
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
            textEditor?.Focus();
        }

        public void CloseTextEditor(ITextEditor textEditor)
        {
            var item = GetTextEditorSetsViewItem(textEditor);
            item?.Close();
        }

        public ITextEditor GetTextEditor(StorageFile file)
        {
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
            if (_sets.Items == null) return null;
            foreach (SetsViewItem setsItem in _sets.Items)
            {
                if (!(setsItem.Content is ITextEditor textEditor)) continue;
                if (textEditor.EditingFilePath != null && string.Equals(textEditor.EditingFilePath, file.Path, StringComparison.OrdinalIgnoreCase))
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

        private void SetsView_OnItemsChanged(object sender, IVectorChangedEventArgs e)
        {
            _allTextEditors = GetAllTextEditors();
        }

        private void SetsView_OnSetClosing(object sender, SetClosingEventArgs e)
        {
            if (!(e.Set.Content is ITextEditor textEditor)) return;

            if (TextEditorClosing != null)
            {
                e.Cancel = true;
                TextEditorClosing.Invoke(this, textEditor);
            }
        }

        private void TextEditor_Loaded(object sender, RoutedEventArgs e)
        {
            if (!(sender is ITextEditor textEditor)) return;
            TextEditorLoaded?.Invoke(this, textEditor);
        }

        private void TextEditor_Unloaded(object sender, RoutedEventArgs e)
        {
            if (!(sender is ITextEditor textEditor)) return;
            TextEditorUnloaded?.Invoke(this, textEditor);
        }

        private void TextEditor_OnSelectionChanged(object sender, EventArgs e)
        {
            if (!(sender is ITextEditor textEditor)) return;
            TextEditorSelectionChanged?.Invoke(this, textEditor);
        }

        private void TextEditor_OnFontZoomFactorChanged(object sender, EventArgs e)
        {
            if (!(sender is ITextEditor textEditor)) return;
            TextEditorFontZoomFactorChanged?.Invoke(this, textEditor);
        }

        private void TextEditor_OnEditorModificationStateChanged(object sender, EventArgs e)
        {
            if (!(sender is ITextEditor textEditor)) return;
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
            if (!(sender is ITextEditor textEditor)) return;
            TextEditorModeChanged?.Invoke(this, textEditor);
        }

        private void TextEditor_OnFileModificationStateChanged(object sender, EventArgs e)
        {
            if (!(sender is ITextEditor textEditor)) return;
            TextEditorFileModificationStateChanged?.Invoke(this, textEditor);
        }

        private void TextEditor_OnEncodingChanged(object sender, EventArgs e)
        {
            if (!(sender is ITextEditor textEditor)) return;
            TextEditorEncodingChanged?.Invoke(this, textEditor);
        }

        private void TextEditor_OnLineEndingChanged(object sender, EventArgs e)
        {
            if (!(sender is ITextEditor textEditor)) return;
            TextEditorLineEndingChanged?.Invoke(this, textEditor);
        }

        private void TextEditor_OnFileRenamed(object sender, EventArgs e)
        {
            if (!(sender is ITextEditor textEditor)) return;
            var item = GetTextEditorSetsViewItem(textEditor);
            if (item == null) return;
            item.Header = textEditor.EditingFileName ?? textEditor.FileNamePlaceholder;
            TextEditorRenamed?.Invoke(this, textEditor);
        }

        #region DragAndDrop

        private async void Sets_DragOver(object sender, DragEventArgs args)
        {
            var deferral = args.GetDeferral();

            bool canHandle = false;
            string dragUICaption = null;

            if (args.DataView == null)
            {
                deferral.Complete();
                return;
            }

            if (!string.IsNullOrEmpty(args.DataView?.Properties?.ApplicationName) &&
                string.Equals(args.DataView?.Properties?.ApplicationName, App.ApplicationName))
            {
                args.DataView.Properties.TryGetValue(NotepadsTextEditorMetaData, out object dataObj);
                if (dataObj is string data)
                {
                    canHandle = true;
                    dragUICaption = _resourceLoader.GetString("App_DragAndDrop_UIOverride_Caption_MoveTabHere");
                }
            }

            if (!canHandle && args.DataView.Contains(StandardDataFormats.StorageItems))
            {
                try
                {
                    var items = await args.DataView.GetStorageItemsAsync();
                    if (items.Count > 0 && items.Any(i => i is StorageFile))
                    {
                        canHandle = true;
                        dragUICaption = _resourceLoader.GetString("App_DragAndDrop_UIOverride_Caption_OpenWithNotepads");
                    }
                }
                catch
                {
                    deferral.Complete();
                    return;
                }
            }

            if (canHandle)
            {
                args.Handled = true;
                args.AcceptedOperation = DataPackageOperation.Link;
                try
                {
                    if (args.DragUIOverride != null)
                    {
                        args.DragUIOverride.Caption = dragUICaption;
                        args.DragUIOverride.IsCaptionVisible = true;
                        args.DragUIOverride.IsGlyphVisible = false;
                    }
                }
                catch
                {
                    // ignored
                }
            }

            deferral.Complete();
        }

        private void Sets_DragItemsStarting(object sender, DragItemsStartingEventArgs args)
        {
            // In Initial Window we need to serialize our tab data.
            var item = args.Items.FirstOrDefault();
            if (!(item is ITextEditor editor)) return;

            try
            {
                var data = JsonSerializer.Serialize(editor.GetTextEditorStateMetaData());

                var lastSavedText = editor.LastSavedSnapshot.Content;
                var pendingText = editor.GetText();

                args.Data.Properties.Add(NotepadsTextEditorLastSavedContent, lastSavedText);

                if (!string.Equals(lastSavedText, pendingText))
                {
                    args.Data.Properties.Add(NotepadsTextEditorPendingContent, pendingText);
                }

                // Add Editing File
                if (editor.EditingFile != null)
                {
                    args.Data.Properties.FileTypes.Add(StandardDataFormats.StorageItems);
                    args.Data.Properties.Add(NotepadsTextEditorEditingFilePath, editor.EditingFilePath);
                    args.Data.SetStorageItems(new List<IStorageItem>() { editor.EditingFile }, readOnly: false);
                }

                args.Data.Properties.Add(NotepadsTextEditorMetaData, data);
                args.Data.Properties.Add(NotepadsTextEditorGuid, editor.Id.ToString());
                args.Data.Properties.Add(NotepadsInstanceId, App.InstanceId.ToString());
                args.Data.Properties.ApplicationName = App.ApplicationName;

                ApplicationSettingsStore.Write(SetDragAndDropActionStatus, "Started");
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(NotepadsCore)}] Failed to prepare editor meta data for drag and drop: {ex.Message}");
            }
        }

        private async void Sets_Drop(object sender, DragEventArgs args)
        {
            if (!(sender is SetsView sets))
            {
                return;
            }

            // Handle non Notepads drop event
            if (string.IsNullOrEmpty(args.DataView?.Properties?.ApplicationName) ||
                !string.Equals(args.DataView?.Properties?.ApplicationName, App.ApplicationName))
            {
                if (args.DataView == null || !args.DataView.Contains(StandardDataFormats.StorageItems)) return;
                var fileDropDeferral = args.GetDeferral();
                var storageItems = await args.DataView.GetStorageItemsAsync();
                StorageItemsDropped?.Invoke(this, storageItems);
                fileDropDeferral.Complete();
                return;
            }

            var deferral = args.GetDeferral();

            try
            {
                args.DataView.Properties.TryGetValue(NotepadsTextEditorMetaData, out object dataObj);

                if (!(dataObj is string data)) throw new Exception("Failed to drop editor set: NotepadsTextEditorMetaData is invalid (Not String).");

                TextEditorStateMetaData metaData = JsonSerializer.Deserialize<TextEditorStateMetaData>(data);

                if (args.DataView.Properties.TryGetValue(NotepadsTextEditorEditingFilePath,
                        out object editingFilePathObj) && editingFilePathObj is string editingFilePath)
                {
                    var editor = GetTextEditor(editingFilePath);
                    if (editor != null)
                    {
                        SwitchTo(editor);
                        NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_FileAlreadyOpened"), 2500);
                        throw new Exception("Failed to drop editor set: File already opened.");
                    }
                }

                StorageFile editingFile = null;

                if (metaData.HasEditingFile && args.DataView.Contains(StandardDataFormats.StorageItems))
                {
                    var storageItems = await args.DataView.GetStorageItemsAsync();
                    if (storageItems.Count == 1 && storageItems[0] is StorageFile file)
                    {
                        editingFile = file;
                    }
                    else
                    {
                        throw new Exception("Failed to read storage file from dropped set: Expecting only one storage file.");
                    }
                }

                string lastSavedText = null;
                string pendingText = null;

                if (args.DataView.Properties.TryGetValue(NotepadsTextEditorLastSavedContent, out object lastSavedContentObj) &&
                    lastSavedContentObj is string lastSavedContent)
                {
                    lastSavedText = lastSavedContent;
                }
                else
                {
                    throw new Exception($"Failed to get last saved content from DataView: NotepadsTextEditorLastSavedContent property Is null");
                }

                if (args.DataView.Properties.TryGetValue(NotepadsTextEditorPendingContent, out object pendingContentObj) &&
                    pendingContentObj is string pendingContent)
                {
                    pendingText = pendingContent;
                }

                ApplicationSettingsStore.Write(SetDragAndDropActionStatus, "Handled");

                var index = -1;

                // Determine which items in the list our pointer is in between.
                for (int i = 0; i < sets.Items?.Count; i++)
                {
                    var item = sets.ContainerFromIndex(i) as SetsViewItem;

                    if (args.GetPosition(item).X - item?.ActualWidth < 0)
                    {
                        index = i;
                        break;
                    }
                }

                var atIndex = index == -1 ? sets.Items.Count : index;

                var textFile = new TextFile(lastSavedText,
                    EncodingUtility.GetEncodingByName(metaData.LastSavedEncoding),
                    LineEndingUtility.GetLineEndingByName(metaData.LastSavedLineEnding),
                    metaData.DateModifiedFileTime);

                var newEditor = CreateTextEditor(Guid.NewGuid(), textFile, editingFile, metaData.FileNamePlaceholder, metaData.IsModified);
                OpenTextEditor(newEditor, atIndex);
                newEditor.ResetEditorState(metaData, pendingText);

                if (metaData.IsContentPreviewPanelOpened)
                {
                    newEditor.ShowHideContentPreview();
                }

                if (metaData.IsInDiffPreviewMode)
                {
                    newEditor.OpenSideBySideDiffViewer();
                }

                deferral.Complete();
                AnalyticsService.TrackEvent("OnSetDropped");
            }
            catch (Exception ex)
            {
                LoggingService.LogException(ex);
                deferral.Complete();
            }
        }

        private void Sets_DragItemsCompleted(ListViewBase sender, DragItemsCompletedEventArgs args)
        {
            if (ApplicationSettingsStore.Read(SetDragAndDropActionStatus) is string setDragAndDropActionStatus && setDragAndDropActionStatus == "Handled")
            {
                if (args.Items.FirstOrDefault() is ITextEditor editor)
                {
                    TextEditorMovedToAnotherAppInstance?.Invoke(this, editor);
                }
            }

            ApplicationSettingsStore.Remove(SetDragAndDropActionStatus);
        }

        private async void Sets_SetDraggedOutside(object sender, SetDraggedOutsideEventArgs e)
        {
            if (_sets.Items?.Count > 1 && e.Set?.Content is ITextEditor textEditor)
            {
                // Only allow untitled empty document to be dragged outside for now
                if (!textEditor.IsModified && textEditor.EditingFile == null)
                {
                    DeleteTextEditor(textEditor);
                    await NotepadsProtocolService.LaunchProtocolAsync(NotepadsOperationProtocol.OpenNewInstance);
                }
            }
        }

        #endregion
    }
}