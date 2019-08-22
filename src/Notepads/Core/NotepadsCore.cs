
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

        public event KeyEventHandler TextEditorKeyDown;

        private readonly INotepadsExtensionProvider _extensionProvider;

        private TextEditor _selectedTextEditor;

        private TextEditor[] _allTextEditors;

        private const string NotepadsTextEditorMetaData = "NotepadsTextEditorMetaData";
        private const string NotepadsTextEditorGuid = "NotepadsTextEditorGuid";
        private const string NotepadsInstanceId = "NotepadsInstanceId";

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

        private void Sets_DragOver(object sender, DragEventArgs e)
        {
            if (e.DataView.Properties.ContainsKey(NotepadsTextEditorMetaData))
            {
                e.AcceptedOperation = DataPackageOperation.Copy;
            }
        }

        private void Sets_DragItemsStarting(object sender, DragItemsStartingEventArgs args)
        {
            // In Initial Window we need to serialize our tab data.
            var item = args.Items.FirstOrDefault();

            if (item is TextEditor editor)
            {
                var data = JsonConvert.SerializeObject(editor.GetTextEditorMetaData());

                args.Data.Properties.Add(NotepadsTextEditorMetaData, data);

                args.Data.Properties.Add(NotepadsTextEditorGuid, editor.Id.ToString());

                args.Data.Properties.Add(NotepadsInstanceId, App.Id.ToString());

                args.Data.Properties.ApplicationName = "Notepads";

                // Add Editing File
                if (editor.EditingFile != null)
                {
                    args.Data.Properties.FileTypes.Add(StandardDataFormats.StorageItems);
                    args.Data.SetStorageItems(new List<IStorageItem>() { editor.EditingFile }, readOnly: false);
                }

                ApplicationSettings.Write("SetDroppedToAnotherInstance", false);
            }
        }

        private async void Sets_Drop(object sender, DragEventArgs e)
        {
            if (!(sender is SetsView))
            {
                return;
            }

            var deferral = e.GetDeferral();

            if (e.DataView.Properties.TryGetValue(NotepadsTextEditorMetaData, out object dataObj) && dataObj is string data)
            {
                var metadata = JsonConvert.DeserializeObject<TextEditorMetaData>(data);
                if (metadata == null) return;

                StorageFile editingFile = null;

                if (metadata.HasEditingFile && e.DataView.Contains(StandardDataFormats.StorageItems))
                {
                    try
                    {
                        var storageItems = await e.DataView.GetStorageItemsAsync();
                        if (storageItems.Count == 1 && storageItems[0] is StorageFile file)
                        {
                            editingFile = file;
                        }
                    }
                    catch (Exception ex)
                    {
                        LoggingService.LogError($"Failed to read storage file from dropped set: {ex.Message}");
                        e.Handled = false;
                        deferral.Complete();
                        return;
                    }
                }

                // First we need to get the position in the List to drop to
                var sets = sender as SetsView;
                var index = -1;

                // Determine which items in the list our pointer is in between.
                for (int i = 0; i < sets.Items.Count; i++)
                {
                    var item = sets.ContainerFromIndex(i) as SetsViewItem;

                    if (e.GetPosition(item).X - item.ActualWidth < 0)
                    {
                        index = i;
                        break;
                    }
                }

                var atIndex = index == -1 ? sets.Items.Count : index;

                OpenNewTextEditor(
                    Guid.NewGuid(),
                    metadata.OriginalText,
                    editingFile,
                    metadata.DateModifiedFileTime,
                    EncodingUtility.GetEncodingByName(metadata.OriginalEncoding),
                    LineEndingUtility.GetLineEndingByName(metadata.OriginalLineEncoding),
                    metadata.IsModified,
                    atIndex).Init(metadata);

                ApplicationSettings.Write("SetDroppedToAnotherInstance", true);

                e.Handled = true;
                deferral.Complete();
            }
            else
            {
                e.Handled = false;
                deferral.Complete();
            }
        }

        private void Sets_DragItemsCompleted(ListViewBase sender, DragItemsCompletedEventArgs args)
        {
            if (args.DropResult == DataPackageOperation.None) return;

            var setDroppedToAnotherInstance = ApplicationSettings.Read("SetDroppedToAnotherInstance") as bool?;

            if (setDroppedToAnotherInstance.HasValue && setDroppedToAnotherInstance.Value)
            {
                if (args.Items.FirstOrDefault() is TextEditor editor)
                {
                    DeleteTextEditor(editor);
                }
            }

            if (setDroppedToAnotherInstance.HasValue)
            {
                ApplicationSettings.Write("SetDroppedToAnotherInstance", null);
            }
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

        public async Task<TextEditor> OpenNewTextEditor(StorageFile file, Guid? id = null, int atIndex = -1)
        {
            if (FileOpened(file))
            {
                SwitchTo(file);
                return GetSelectedTextEditor();
            }

            var textFile = await FileSystemUtility.ReadFile(file);
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
                Header = file == null ? DefaultNewFileName : file.Name,
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
            await textEditor.SaveContentToFileAndUpdateEditorState(file);
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
            title = textEditor.EditingFile != null ? textEditor.EditingFile.Name : DefaultNewFileName;
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
                if (setsItem.Content is TextEditor textEditor)
                {
                    if (string.Equals(textEditor.EditingFile?.Path, file.Path))
                    {
                        return setsItem;
                    }
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
                if (textEditor.EditingFile != null)
                {
                    item.Header = textEditor.EditingFile.Name;
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
