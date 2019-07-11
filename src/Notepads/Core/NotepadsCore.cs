
namespace Notepads.Core
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using System.Text;
    using System.Threading.Tasks;
    using Windows.System;
    using Windows.Storage;
    using Windows.UI;
    using Windows.UI.Core;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media;
    using Windows.UI.Xaml.Input;
    using Notepads.EventArgs;
    using Notepads.Services;
    using Notepads.Controls.TextEditor;
    using Notepads.Utilities;
    using SetsView;

    public class NotepadsCore : INotepadsCore
    {
        public SetsView Sets;

        public readonly string DefaultNewFileName;

        public event EventHandler<TextEditor> OnActiveTextEditorLoaded;

        public event EventHandler<TextEditor> OnActiveTextEditorUnloaded;

        public event EventHandler<TextEditor> OnActiveTextEditorSelectionChanged;

        public event EventHandler<TextEditor> OnActiveTextEditorEncodingChanged;

        public event EventHandler<TextEditor> OnActiveTextEditorLineEndingChanged;

        public event KeyEventHandler OnActiveTextEditorKeyDown;

        public NotepadsCore(SetsView sets, 
            string defaultNewFileName)
        {
            Sets = sets;
            DefaultNewFileName = defaultNewFileName;

            ThemeSettingsService.OnAccentColorChanged += OnAppAccentColorChanged;
            EditorSettingsService.OnDefaultLineEndingChanged += EditorSettingsService_OnDefaultLineEndingChanged;
            EditorSettingsService.OnDefaultEncodingChanged += EditorSettingsService_OnDefaultEncodingChanged;
        }

        public void CreateNewTextEditor()
        {
            CreateNewTextEditor(string.Empty,
                null,
                EditorSettingsService.EditorDefaultEncoding,
                EditorSettingsService.EditorDefaultLineEnding);
        }

        private void CreateNewTextEditor(string text, 
            StorageFile file, 
            Encoding encoding, 
            LineEnding lineEnding)
        {
            var textEditor = new TextEditor()
            {
                EditingFile = file,
                Encoding = encoding,
                LineEnding = lineEnding,
                Saved = true,
            };

            textEditor.SetText(text);
            textEditor.ClearUndoQueue();
            textEditor.Loaded += TextEditor_Loaded;
            textEditor.Unloaded += TextEditor_Unloaded;
            textEditor.OnSetClosingKeyDown += TextEditor_OnSetClosingKeyDown;

            var newItem = new SetsViewItem
            {
                Header = file == null ? DefaultNewFileName : file.Name,
                Content = textEditor,
                SelectionIndicatorForeground = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush,
                Icon = new SymbolIcon(Symbol.Save)
                {
                    Foreground = Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush,
                }
            };
            newItem.Icon.Visibility = Visibility.Collapsed;

            // Notepads should replace current "New Document.txt" with open file if it is empty and it is the only tab that has been created.
            if (Sets.Items?.Count == 1 && file != null)
            {
                if (((Sets.SelectedItem as SetsViewItem)?.Content is TextEditor editor))
                {
                    if (editor.Saved && editor.EditingFile == null)
                    {
                        Sets.Items.Clear();
                    }
                }
            }

            Sets.Items?.Add(newItem);

            if (Sets.Items?.Count > 1)
            {
                Sets.SelectedItem = newItem;
                Sets.ScrollToLastSet();
            }
        }

        public bool TryGetSharingContent(TextEditor textEditor, out string title, out string content)
        {
            title = null;
            content = null;

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
                if (textEditor.Saved) continue;
                return true;
            }
            return false;
        }

        public void ChangeLineEnding(TextEditor textEditor, LineEnding lineEnding)
        {
            if (lineEnding == textEditor.LineEnding) return;
            MarkTextEditorSetNotSaved((SetsViewItem)Sets.SelectedItem, textEditor);
            textEditor.LineEnding = lineEnding;
        }

        public void ChangeEncoding(TextEditor textEditor, Encoding encoding)
        {
            if (EncodingUtility.Equals(textEditor.Encoding, encoding)) return;
            MarkTextEditorSetNotSaved((SetsViewItem)Sets.SelectedItem, textEditor);
            textEditor.Encoding = encoding;
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

        private void SwitchTo(StorageFile file)
        {
            var item = GetTextEditorSetsViewItem(file);
            Sets.SelectedItem = item;
            Sets.ScrollIntoView(item);
            FocusOnActiveTextEditor();
        }

        public TextEditor GetActiveTextEditor()
        {
            if ((!((Sets.SelectedItem as SetsViewItem)?.Content is TextEditor textEditor))) return null;
            return textEditor;
        }

        public void FocusOnActiveTextEditor()
        {
            GetActiveTextEditor()?.Focus(FocusState.Programmatic);
        }

        public async Task Open(StorageFile file)
        {
            if (FileOpened(file))
            {
                SwitchTo(file);
                return;
            }

            var textFile = await FileSystemUtility.ReadFile(file);

            CreateNewTextEditor(textFile.Content,
                file,
                textFile.Encoding,
                textFile.LineEnding);
        }

        public async Task<bool> Save(TextEditor textEditor, StorageFile file)
        {
            var success = await textEditor.SaveToFile(file);

            if (success)
            {
                var item = GetTextEditorSetsViewItem(textEditor);
                if (item != null)
                {
                    item.Header = file.Name;
                    item.Icon.Visibility = Visibility.Collapsed;
                }
            }

            return success;
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

        private void MarkTextEditorSetNotSaved(SetsViewItem item, TextEditor textEditor)
        {
            textEditor.Saved = false;
            item.Icon.Visibility = Visibility.Visible;
        }

        private void TextEditor_Loaded(object sender, RoutedEventArgs e)
        {
            if (!(sender is TextEditor textEditor)) return;

            textEditor.KeyDown += OnActiveTextEditorKeyDown;
            textEditor.TextChanging += TextEditor_TextChanging;
            textEditor.SelectionChanged += TextEditor_SelectionChanged;
            textEditor.Focus(FocusState.Programmatic);

            OnActiveTextEditorLoaded?.Invoke(this, textEditor);
        }

        private void TextEditor_Unloaded(object sender, RoutedEventArgs e)
        {
            if (!(sender is TextEditor textEditor)) return;

            textEditor.KeyDown -= OnActiveTextEditorKeyDown;
            textEditor.TextChanging -= TextEditor_TextChanging;
            textEditor.SelectionChanged -= TextEditor_SelectionChanged;

            if (Sets.Items?.Count == 0)
            {
                Application.Current.Exit();
            }

            OnActiveTextEditorUnloaded?.Invoke(this, textEditor);
        }

        private void TextEditor_OnSetClosingKeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (!(sender is TextEditor textEditor)) return;

            if (Sets.Items == null) return;

            foreach (SetsViewItem setsItem in Sets.Items)
            {
                if (setsItem.Content != textEditor) continue;
                setsItem.Close();
                e.Handled = true;
                break;
            }
        }

        private void TextEditor_TextChanging(object sender, RichEditBoxTextChangingEventArgs args)
        {
            if (!(sender is TextEditor textEditor) || !args.IsContentChanging) return;

            if (textEditor.Saved)
            {
                MarkTextEditorSetNotSaved(GetTextEditorSetsViewItem(textEditor), textEditor);
            }
        }

        private void TextEditor_SelectionChanged(object sender, RoutedEventArgs e)
        {
            if (!(sender is TextEditor textEditor)) return;
            OnActiveTextEditorSelectionChanged?.Invoke(this, textEditor);
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

        private void EditorSettingsService_OnDefaultEncodingChanged(object sender, Encoding encoding)
        {
            if (Sets.Items == null) return;
            foreach (SetsViewItem setItem in Sets.Items)
            {
                if (!(setItem.Content is TextEditor textEditor)) continue;
                if (textEditor.EditingFile != null) continue;

                textEditor.Encoding = encoding;
                if (textEditor == ((SetsViewItem)Sets.SelectedItem)?.Content)
                {
                    OnActiveTextEditorEncodingChanged?.Invoke(this, textEditor);
                }
            }
        }                                     

        private void EditorSettingsService_OnDefaultLineEndingChanged(object sender, LineEnding lineEnding)
        {
            if (Sets.Items == null) return;
            foreach (SetsViewItem setItem in Sets.Items)
            {
                if (!(setItem.Content is TextEditor textEditor)) continue;
                if (textEditor.EditingFile != null) continue;

                textEditor.LineEnding = lineEnding;
                if (textEditor == ((SetsViewItem)Sets.SelectedItem)?.Content)
                {
                    OnActiveTextEditorLineEndingChanged?.Invoke(this, textEditor);
                }
            }
        }
    }
}
