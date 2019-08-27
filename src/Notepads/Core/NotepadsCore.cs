
namespace Notepads.Core
{
    using Newtonsoft.Json;
    using Notepads.Controls.TextEditor;
    using Notepads.Extensions;
    using Notepads.Services;
    using Notepads.Settings;
    using Notepads.Utilities;
    using SetsView;
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using System.Text;
    using System.Threading.Tasks;
    using Windows.ApplicationModel.DataTransfer;
    using Windows.Foundation.Collections;
    using Windows.Storage;
    using Windows.UI;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;

    public class NotepadsCore : INotepadsCore
    {
        public SetsView Sets;

        public readonly string DefaultNewFileName;

        public event EventHandler<TextEditor> TextEditorLoaded;

        public event EventHandler<TextEditor> TextEditorUnloaded;

        public event EventHandler<TextEditor> TextEditorEditorModificationStateChanged;

        public event EventHandler<TextEditor> TextEditorFileModificationStateChanged;

        public event EventHandler<TextEditor> TextEditorSaved;

        public event EventHandler<TextEditor> TextEditorClosingWithUnsavedContent;

        public event EventHandler<TextEditor> TextEditorSelectionChanged;

        public event EventHandler<TextEditor> TextEditorEncodingChanged;

        public event EventHandler<TextEditor> TextEditorLineEndingChanged;

        public event EventHandler<TextEditor> TextEditorModeChanged;

        public event EventHandler<IReadOnlyList<IStorageItem>> StorageItemsDropped;

        public event KeyEventHandler TextEditorKeyDown;

        private readonly INotepadsExtensionProvider _extensionProvider;

        private TextEditor _selectedTextEditor;

        private TextEditor[] _allTextEditors;

        private const string SetDragAndDropActionStatus = "SetDragAndDropActionStatus";
        private const string NotepadsTextEditorMetaData = "NotepadsTextEditorMetaData";
        private const string NotepadsTextEditorGuid = "NotepadsTextEditorGuid";
        private const string NotepadsInstanceId = "NotepadsInstanceId";
        private const string NotepadsTextEditorLastSavedContent = "NotepadsTextEditorLastSavedContent";
        private const string NotepadsTextEditorPendingContent = "NotepadsTextEditorPendingContent";
        private const string NotepadsTextEditorEditingFilePath = "NotepadsTextEditorEditingFilePath";

        public NotepadsCore(SetsView sets,
            string defaultNewFileName,
            INotepadsExtensionProvider extensionProvider)
        {
            Sets = sets;
            Sets.SelectionChanged += SetsView_OnSelectionChanged;
            Sets.Items.VectorChanged += SetsView_OnItemsChanged;
            Sets.SetClosing += SetsView_OnSetClosing;
            Sets.SetTapped += (sender, args) => { FocusOnTextEditor(args.Item as TextEditor); };
            Sets.SetDraggedOutside += Sets_SetDraggedOutside;
            Sets.DragOver += Sets_DragOver;
            Sets.Drop += Sets_Drop;
            Sets.DragItemsStarting += Sets_DragItemsStarting;
            Sets.DragItemsCompleted += Sets_DragItemsCompleted;

            _extensionProvider = extensionProvider;
            DefaultNewFileName = defaultNewFileName;
            ThemeSettingsService.OnAccentColorChanged += OnAppAccentColorChanged;
        }

        private async void Sets_DragOver(object sender, DragEventArgs args)
        {
            var deferral = args.GetDeferral();

            bool canHandle = false;
            string dragUICaption = null;

            if (!string.IsNullOrEmpty(args.DataView?.Properties?.ApplicationName) &&
                string.Equals(args.DataView?.Properties?.ApplicationName, App.ApplicationName))
            {
                canHandle = true;
                dragUICaption = "Move tab here";
            }

            if (!canHandle && args.DataView.Contains(Windows.ApplicationModel.DataTransfer.StandardDataFormats.StorageItems))
            {
                try
                {
                    var items = await args.DataView.GetStorageItemsAsync();
                    if (items.Count > 0 && items.Any(i => i is StorageFile))
                    {
                        canHandle = true;
                        dragUICaption = "Open with Notepads";
                    }
                }
                catch { }
            }

            if (canHandle)
            {
                args.Handled = true;
                args.AcceptedOperation = DataPackageOperation.Link;
                try
                {
                    if (args.DragUIOverride != null && dragUICaption != null)
                    {
                        args.DragUIOverride.Caption = dragUICaption;
                        args.DragUIOverride.IsCaptionVisible = true;
                        args.DragUIOverride.IsGlyphVisible = false;
                    }
                }
                catch { }
            }

            deferral.Complete();
        }

        private void Sets_DragItemsStarting(object sender, DragItemsStartingEventArgs args)
        {
            // In Initial Window we need to serialize our tab data.
            var item = args.Items.FirstOrDefault();
            if (!(item is TextEditor editor)) return;

            try
            {
                var data = JsonConvert.SerializeObject(editor.GetTextEditorMetaData());

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
                args.Data.Properties.Add(NotepadsInstanceId, App.Id.ToString());
                args.Data.Properties.ApplicationName = App.ApplicationName;

                ApplicationSettingsStore.Write(SetDragAndDropActionStatus, "Started");
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"Failed to prepare editor meta data for drag and drop: {ex.Message}");
            }
        }

        private async void Sets_Drop(object sender, DragEventArgs args)
        {
            if (!(sender is SetsView))
            {
                return;
            }

            var sets = sender as SetsView;

            // Handle non Notepads drop event
            if (string.IsNullOrEmpty(args.DataView?.Properties?.ApplicationName) ||
                !string.Equals(args.DataView?.Properties?.ApplicationName, App.ApplicationName))
            {
                if (!args.DataView.Contains(StandardDataFormats.StorageItems)) return;
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

                TextEditorStateMetaData metaData = JsonConvert.DeserializeObject<TextEditorStateMetaData>(data);

                if (args.DataView.Properties.TryGetValue(NotepadsTextEditorEditingFilePath,
                        out object editingFilePathObj) && editingFilePathObj is string editingFilePath)
                {
                    var editor = GetTextEditor(editingFilePath);
                    if (editor != null)
                    {
                        SwitchTo(editor);
                        NotificationCenter.Instance.PostNotification("File already opened!", 2500);
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

                //deferral.Complete();

                var index = -1;

                // Determine which items in the list our pointer is in between.
                for (int i = 0; i < sets.Items.Count; i++)
                {
                    var item = sets.ContainerFromIndex(i) as SetsViewItem;

                    if (args.GetPosition(item).X - item.ActualWidth < 0)
                    {
                        index = i;
                        break;
                    }
                }

                var atIndex = index == -1 ? sets.Items.Count : index;

                var newEditor = OpenNewTextEditor(
                    Guid.NewGuid(),
                    lastSavedText,
                    editingFile,
                    metaData.DateModifiedFileTime,
                    EncodingUtility.GetEncodingByName(metaData.LastSavedEncoding),
                    LineEndingUtility.GetLineEndingByName(metaData.LastSavedLineEnding),
                    metaData.IsModified,
                    atIndex);

                    newEditor.ApplyChangesFrom(metaData, pendingText);

                if (metaData.IsContentPreviewPanelOpened)
                {
                    newEditor.ShowHideContentPreview();
                }

                if (metaData.IsInDiffPreviewMode)
                {
                    newEditor.OpenSideBySideDiffViewer();
                }

                deferral.Complete();
            }
            catch (Exception ex)
            {
                LoggingService.LogException(ex);
                deferral.Complete();
            }
        }

        private void Sets_DragItemsCompleted(ListViewBase sender, DragItemsCompletedEventArgs args)
        {
            var setDragAndDropActionStatus = ApplicationSettingsStore.Read(SetDragAndDropActionStatus) as string;

            if (setDragAndDropActionStatus != null && setDragAndDropActionStatus == "Handled")
            {
                if (args.Items.FirstOrDefault() is TextEditor editor)
                {
                    DeleteTextEditor(editor);
                }
            }

            ApplicationSettingsStore.Remove(SetDragAndDropActionStatus);
        }

        private async void Sets_SetDraggedOutside(object sender, SetDraggedOutsideEventArgs e)
        {
            if (Sets.Items.Count > 1 && e.Set.Content is TextEditor textEditor)
            {
                // Only allow untitled empty document to be dragged outside for now
                if (!textEditor.IsModified && textEditor.EditingFile == null)
                {
                    DeleteTextEditor(textEditor);
                    await NotepadsProtocolService.LaunchProtocolAsync(NotepadsOperationProtocol.OpenNewInstance);
                }
            }
        }

        public TextEditor OpenNewTextEditor(Guid? id = null, int atIndex = -1)
        {
            return OpenNewTextEditor(
                id ?? Guid.NewGuid(),
                string.Empty,
                null,
                -1,
                EditorSettingsService.EditorDefaultEncoding,
                EditorSettingsService.EditorDefaultLineEnding,
                false,
                atIndex);
        }

        public async Task<TextEditor> OpenNewTextEditor(StorageFile file, bool ignoreFileSizeLimit, Guid? id = null, int atIndex = -1)
        {
            if (FileOpened(file))
            {
                SwitchTo(file);
                NotificationCenter.Instance.PostNotification("File already opened!", 2500);
                return GetSelectedTextEditor();
            }

            var textFile = await FileSystemUtility.ReadFile(file, ignoreFileSizeLimit);
            var dateModifiedFileTime = await FileSystemUtility.GetDateModified(file);

            return OpenNewTextEditor(
                id ?? Guid.NewGuid(),
                textFile.Content,
                file,
                dateModifiedFileTime,
                textFile.Encoding,
                textFile.LineEnding,
                false);
        }

        public TextEditor OpenNewTextEditor(
            Guid id,
            string text,
            StorageFile file,
            long dateModifiedFileTime,
            Encoding encoding,
            LineEnding lineEnding,
            bool isModified,
            int atIndex = -1)
        {
            var textEditor = new TextEditor
            {
                Id = id,
                ExtensionProvider = _extensionProvider
            };

            textEditor.Init(new TextFile(text, encoding, lineEnding, dateModifiedFileTime), file, isModified: isModified);
            textEditor.Loaded += TextEditor_Loaded;
            textEditor.Unloaded += TextEditor_Unloaded;
            textEditor.SelectionChanged += TextEditor_SelectionChanged;
            textEditor.KeyDown += TextEditorKeyDown;
            textEditor.EditorModificationStateChanged += TextEditor_OnEditorModificationStateChanged;
            textEditor.ModeChanged += TextEditor_ModeChanged;
            textEditor.FileModificationStateChanged += (sender, args) => { TextEditorFileModificationStateChanged?.Invoke(this, sender as TextEditor); };
            textEditor.LineEndingChanged += (sender, args) => { TextEditorLineEndingChanged?.Invoke(this, sender as TextEditor); };
            textEditor.EncodingChanged += (sender, args) => { TextEditorEncodingChanged?.Invoke(this, sender as TextEditor); };

            var textEditorSetsViewItem = new SetsViewItem
            {
                Header = textEditor.EditingFileName ?? DefaultNewFileName,
                Content = textEditor,
                SelectionIndicatorForeground = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush,
                Icon = new SymbolIcon(Symbol.Save)
                {
                    Foreground = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush,
                }
            };

            if (textEditorSetsViewItem.Content == null || textEditorSetsViewItem.Content is Page)
            {
                throw new Exception("Content should not be null and type should not be Page (SetsView does not work well with Page controls)");
            }

            textEditorSetsViewItem.Icon.Visibility = isModified ? Visibility.Visible : Visibility.Collapsed;
            textEditorSetsViewItem.ContextFlyout = new TabContextFlyout(this, textEditor);

            // Notepads should replace current "Untitled.txt" with open file if it is empty and it is the only tab that has been created.
            // If index != -1, it means set was created after a drag and drop, we should skip this logic
            if (GetNumberOfOpenedTextEditors() == 1 && file != null && atIndex == -1)
            {
                var selectedEditor = GetAllTextEditors().First();
                if (selectedEditor.EditingFile == null && !selectedEditor.IsModified)
                {
                    Sets.Items?.Clear();
                }
            }

            if (atIndex == -1)
            {
                Sets.Items?.Add(textEditorSetsViewItem);
            }
            else
            {
                Sets.Items?.Insert(atIndex, textEditorSetsViewItem);
            }

            if (GetNumberOfOpenedTextEditors() > 1)
            {
                Sets.SelectedItem = textEditorSetsViewItem;
                Sets.ScrollToLastSet();
            }

            return textEditor;
        }

        public async Task SaveContentToFileAndUpdateEditorState(TextEditor textEditor, StorageFile file)
        {
            await textEditor.SaveContentToFileAndUpdateEditorState(file); // Will throw if not succeeded
            MarkTextEditorSetSaved(textEditor);
            TextEditorSaved?.Invoke(this, textEditor);
        }

        public void DeleteTextEditor(TextEditor textEditor)
        {
            if (textEditor == null) return;
            var item = GetTextEditorSetsViewItem(textEditor);
            item.IsEnabled = false;
            Sets.Items?.Remove(item);
        }

        public int GetNumberOfOpenedTextEditors()
        {
            return Sets.Items?.Count ?? 0;
        }

        public bool TryGetSharingContent(TextEditor textEditor, out string title, out string content)
        {
            title = textEditor.EditingFileName ?? DefaultNewFileName;
            content = textEditor.GetContentForSharing();
            return !string.IsNullOrEmpty(content);
        }

        public bool HaveUnsavedTextEditor()
        {
            if (Sets.Items == null || Sets.Items.Count == 0) return false;
            foreach (SetsViewItem setsItem in Sets.Items)
            {
                if (!(setsItem.Content is TextEditor textEditor)) continue;
                if (!textEditor.IsModified) continue;
                return true;
            }
            return false;
        }

        public void ChangeLineEnding(TextEditor textEditor, LineEnding lineEnding)
        {
            textEditor.TryChangeLineEnding(lineEnding);
        }

        public void ChangeEncoding(TextEditor textEditor, Encoding encoding)
        {
            textEditor.TryChangeEncoding(encoding);
        }

        public void SwitchTo(bool next)
        {
            if (Sets.Items == null) return;
            if (Sets.Items.Count < 2) return;

            var setsCount = Sets.Items.Count;
            var selected = Sets.SelectedIndex;

            if (next && setsCount > 1)
            {
                if (selected == setsCount - 1) Sets.SelectedIndex = 0;
                else Sets.SelectedIndex += 1;
            }
            else if (!next && setsCount > 1)
            {
                if (selected == 0) Sets.SelectedIndex = setsCount - 1;
                else Sets.SelectedIndex -= 1;
            }
        }

        public void SwitchTo(TextEditor textEditor)
        {
            var item = GetTextEditorSetsViewItem(textEditor);
            if (Sets.SelectedItem != item)
            {
                Sets.SelectedItem = item;
                Sets.ScrollIntoView(item);
            }
        }

        private void SwitchTo(StorageFile file)
        {
            var item = GetTextEditorSetsViewItem(file);
            Sets.SelectedItem = item;
            Sets.ScrollIntoView(item);
        }

        public TextEditor GetSelectedTextEditor()
        {
            if (ThreadUtility.IsOnUIThread())
            {
                if ((!((Sets.SelectedItem as SetsViewItem)?.Content is TextEditor textEditor))) return null;
                return textEditor;
            }
            else
            {
                return _selectedTextEditor;
            }
        }

        public TextEditor GetTextEditor(string editingFilePath)
        {
            if (string.IsNullOrEmpty(editingFilePath))
            {
                return null;
            }

            return GetAllTextEditors().FirstOrDefault(editor => editor.EditingFilePath != null &&
                string.Equals(editor.EditingFilePath, editingFilePath, StringComparison.OrdinalIgnoreCase));
        }

        public TextEditor[] GetAllTextEditors()
        {
            if (!ThreadUtility.IsOnUIThread()) return _allTextEditors;

            if (Sets.Items == null) return new TextEditor[0];
            var editors = new List<TextEditor>();
            foreach (SetsViewItem item in Sets.Items)
            {
                if (item.Content is TextEditor textEditor)
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

        public void FocusOnTextEditor(TextEditor textEditor)
        {
            textEditor?.Focus();
        }

        public void CloseTextEditor(TextEditor textEditor)
        {
            var item = GetTextEditorSetsViewItem(textEditor);
            item?.Close();
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
            if (!(e.Set.Content is TextEditor textEditor)) return;
            if (!textEditor.IsModified) return;
            if (TextEditorClosingWithUnsavedContent != null)
            {
                e.Cancel = true;
                TextEditorClosingWithUnsavedContent.Invoke(this, textEditor);
            }
        }

        private bool FileOpened(StorageFile file)
        {
            var item = GetTextEditorSetsViewItem(file);
            return item != null;
        }

        private SetsViewItem GetTextEditorSetsViewItem(StorageFile file)
        {
            if (Sets.Items == null) return null;
            foreach (SetsViewItem setsItem in Sets.Items)
            {
                if (!(setsItem.Content is TextEditor textEditor)) continue;
                if (textEditor.EditingFilePath != null && string.Equals(textEditor.EditingFilePath, file.Path, StringComparison.OrdinalIgnoreCase))
                {
                    return setsItem;
                }
            }
            return null;
        }

        private SetsViewItem GetTextEditorSetsViewItem(TextEditor textEditor)
        {
            if (Sets.Items == null) return null;
            foreach (SetsViewItem setsItem in Sets.Items)
            {
                if (setsItem.Content is TextEditor editor)
                {
                    if (textEditor == editor) return setsItem;
                }
            }
            return null;
        }

        private void MarkTextEditorSetNotSaved(TextEditor textEditor)
        {
            if (textEditor == null) return;
            var item = GetTextEditorSetsViewItem(textEditor);
            if (item != null)
            {
                item.Icon.Visibility = Visibility.Visible;
            }
        }

        private void MarkTextEditorSetSaved(TextEditor textEditor)
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

        private void TextEditor_Loaded(object sender, RoutedEventArgs e)
        {
            if (!(sender is TextEditor textEditor)) return;
            TextEditorLoaded?.Invoke(this, textEditor);
        }

        private void TextEditor_Unloaded(object sender, RoutedEventArgs e)
        {
            if (!(sender is TextEditor textEditor)) return;
            TextEditorUnloaded?.Invoke(this, textEditor);
        }

        private void TextEditor_SelectionChanged(object sender, RoutedEventArgs e)
        {
            if (!(sender is TextEditor textEditor)) return;
            TextEditorSelectionChanged?.Invoke(this, textEditor);
        }

        private void TextEditor_OnEditorModificationStateChanged(object sender, EventArgs e)
        {
            if (!(sender is TextEditor textEditor)) return;
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

        private void TextEditor_ModeChanged(object sender, EventArgs e)
        {
            if (!(sender is TextEditor textEditor)) return;
            TextEditorModeChanged?.Invoke(this, textEditor);
        }

        private void OnAppAccentColorChanged(object sender, Color color)
        {
            if (Sets.Items == null) return;
            foreach (SetsViewItem item in Sets.Items)
            {
                item.Icon.Foreground = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
                item.SelectionIndicatorForeground = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            }
        }
    }
}
