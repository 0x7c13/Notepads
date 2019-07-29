
namespace Notepads.Core
{
    using Notepads.Controls.TextEditor;
    using Notepads.Extensions;
    using Notepads.Services;
    using Notepads.Utilities;
    using SetsView;
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using System.Text;
    using System.Threading.Tasks;
    using Windows.Storage;
    using Windows.UI;
    using Windows.UI.ViewManagement;
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

        public event EventHandler<TextEditor> TextEditorModificationStateChanged;

        public event EventHandler<TextEditor> TextEditorSaved;

        public event EventHandler<TextEditor> TextEditorClosingWithUnsavedContent;

        public event EventHandler<TextEditor> TextEditorSelectionChanged;

        public event EventHandler<TextEditor> TextEditorEncodingChanged;

        public event EventHandler<TextEditor> TextEditorLineEndingChanged;

        public event EventHandler<TextEditor> TextEditorModeChanged;

        public event KeyEventHandler TextEditorKeyDown;

        private readonly INotepadsExtensionProvider _extensionProvider;

        public NotepadsCore(SetsView sets,
            string defaultNewFileName,
            INotepadsExtensionProvider extensionProvider)
        {
            Sets = sets;
            Sets.SetClosing += SetsView_OnSetClosing;
            Sets.SetTapped += (sender, args) => { FocusOnTextEditor(args.Item as TextEditor); };
            Sets.SetDoubleTapped += (sender, args) =>
            {
                if (ApplicationView.GetForCurrentView().IsFullScreenMode)
                {
                    ApplicationView.GetForCurrentView().ExitFullScreenMode();
                }
                else
                {
                    ApplicationView.GetForCurrentView().TryEnterFullScreenMode();
                }
            };
            _extensionProvider = extensionProvider;

            DefaultNewFileName = defaultNewFileName;

            ThemeSettingsService.OnAccentColorChanged += OnAppAccentColorChanged;
        }

        public void OpenNewTextEditor()
        {
            OpenNewTextEditor(string.Empty,
                null,
                EditorSettingsService.EditorDefaultEncoding,
                EditorSettingsService.EditorDefaultLineEnding);
        }

        public async Task OpenNewTextEditor(StorageFile file)
        {
            if (FileOpened(file))
            {
                SwitchTo(file);
                return;
            }

            var textFile = await FileSystemUtility.ReadFile(file);

            OpenNewTextEditor(textFile.Content,
                file,
                textFile.Encoding,
                textFile.LineEnding);
        }

        private void OpenNewTextEditor(string text,
            StorageFile file,
            Encoding encoding,
            LineEnding lineEnding)
        {
            var textEditor = new TextEditor
            {
                ExtensionProvider = _extensionProvider
            };

            textEditor.Init(new TextFile(text, encoding, lineEnding), file);
            textEditor.Loaded += TextEditor_Loaded;
            textEditor.Unloaded += TextEditor_Unloaded;
            textEditor.SelectionChanged += TextEditor_SelectionChanged;
            textEditor.KeyDown += TextEditorKeyDown;
            textEditor.ModificationStateChanged += TextEditor_OnModificationStateChanged;
            textEditor.ModeChanged += TextEditor_ModeChanged;
            textEditor.LineEndingChanged += (sender, args) => { TextEditorLineEndingChanged?.Invoke(this, sender as TextEditor); };
            textEditor.EncodingChanged += (sender, args) => { TextEditorEncodingChanged?.Invoke(this, sender as TextEditor); };

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

            if (newItem.Content == null || newItem.Content is Page)
            {
                throw new Exception("Content should not be null and type should not be Page (SetsView does not work well with Page controls)");
            }

            newItem.Icon.Visibility = Visibility.Collapsed;
            newItem.ContextFlyout = new TabContextFlyout(this, textEditor);

            // Notepads should replace current "Untitled.txt" with open file if it is empty and it is the only tab that has been created.
            if (GetNumberOfOpenedTextEditors() == 1 && file != null)
            {
                var selectedEditor = GetAllTextEditors().First();
                if (selectedEditor.EditingFile == null && !selectedEditor.IsModified)
                {
                    Sets.Items?.Clear();
                }
            }

            Sets.Items?.Add(newItem);

            if (GetNumberOfOpenedTextEditors() > 1)
            {
                Sets.SelectedItem = newItem;
                Sets.ScrollToLastSet();
            }
        }

        public async Task SaveTextEditorContentToFile(TextEditor textEditor, StorageFile file)
        {
            await textEditor.SaveToFile(file);
            TextEditorSaved?.Invoke(this, textEditor);
        }

        public void DeleteTextEditor(TextEditor textEditor)
        {
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
            Sets.SelectedItem = item;
            Sets.ScrollIntoView(item);
        }

        private void SwitchTo(StorageFile file)
        {
            var item = GetTextEditorSetsViewItem(file);
            Sets.SelectedItem = item;
            Sets.ScrollIntoView(item);
        }

        public TextEditor GetSelectedTextEditor()
        {
            if ((!((Sets.SelectedItem as SetsViewItem)?.Content is TextEditor textEditor))) return null;
            return textEditor;
        }

        public TextEditor[] GetAllTextEditors()
        {
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

        private void TextEditor_OnModificationStateChanged(object sender, EventArgs e)
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
            TextEditorModificationStateChanged?.Invoke(this, textEditor);
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
