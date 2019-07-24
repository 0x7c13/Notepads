
namespace Notepads.Controls.TextEditor
{
    using Notepads.Commands;
    using Notepads.Extensions;
    using Notepads.Services;
    using Notepads.Utilities;
    using System;
    using System.Collections.Generic;
    using System.Text;
    using System.Threading.Tasks;
    using Windows.Foundation;
    using Windows.Storage;
    using Windows.System;
    using Windows.UI.Core;
    using Windows.UI.Text;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;

    public sealed partial class TextEditor : Page
    {
        public FileType FileType { get; private set; }

        private StorageFile _editingFile;

        public StorageFile EditingFile
        {
            get => _editingFile;
            set
            {
                FileType = value == null ? FileType.TextFile : FileTypeUtility.GetFileTypeByFileName(value.Name);
                _editingFile = value;
            }
        }

        public Encoding Encoding { get; set; }

        public LineEnding LineEnding { get; set; }

        public bool Saved { get; set; }

        public INotepadsExtensionProvider ExtensionProvider;

        public event EventHandler<KeyRoutedEventArgs> OnEditorClosingKeyDown;

        private readonly IKeyboardCommandHandler<KeyRoutedEventArgs> _keyboardCommandHandler;

        private IContentPreviewExtension _contentPreviewExtension;

        public event TypedEventHandler<TextEditor, bool> TextChanging;

        public event RoutedEventHandler SelectionChanged;

        public string OriginalContent;

        public TextEditor()
        {
            InitializeComponent();

            TextEditorCore.TextChanging += (sender, args) => { TextChanging?.Invoke(this, args.IsContentChanging); };
            TextEditorCore.SelectionChanged += (sender, args) => { SelectionChanged?.Invoke(this, args); };
            TextEditorCore.KeyDown += TextEditorCore_OnKeyDown;
            TextEditorCore.ContextFlyout = new TextEditorContextFlyout(this);

            // Init shortcuts
            _keyboardCommandHandler = GetKeyboardCommandHandler();
        }

        private void LoadSplitView()
        {
            FindName("SplitPanel");
            FindName("GridSplitter");
            SplitPanel.Visibility = Visibility.Collapsed;
            GridSplitter.Visibility = Visibility.Collapsed;
            SplitPanel.KeyDown += SplitPanel_OnKeyDown;
        }

        public void OpenSplitView(IContentPreviewExtension extension)
        {
            SplitPanel.Content = extension;
            SplitPanelColumnDefinition.Width = new GridLength(ActualWidth / 2.0f);
            SplitPanelColumnDefinition.MinWidth = 100.0f;
            SplitPanel.Visibility = Visibility.Visible;
            GridSplitter.Visibility = Visibility.Visible;
        }

        public void CloseSplitView()
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
                _contentPreviewExtension.Bind(this);
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

        public void LoadSideBySideDiffViewer()
        {
            FindName("SideBySideDiffViewer");
            SideBySideDiffViewer.Visibility = Visibility.Collapsed;
            SideBySideDiffViewer.OnCloseEvent += (sender, args) => CloseSideBySideDiffViewer();
        }

        public void OpenSideBySideDiffViewer()
        {
            TextEditorCore.IsEnabled = false;
            EditorRowDefinition.Height = new GridLength(0);
            SideBySideDiffViewRowDefinition.Height = new GridLength(1, GridUnitType.Star);
            SideBySideDiffViewer.Visibility = Visibility.Visible;
            SideBySideDiffViewer.RenderDiff(OriginalContent, TextEditorCore.GetText());
            Task.Factory.StartNew(
                () => Dispatcher.RunAsync(CoreDispatcherPriority.Low,
                    () => SideBySideDiffViewer.Focus()));
        }

        public void CloseSideBySideDiffViewer()
        {
            TextEditorCore.IsEnabled = true;
            EditorRowDefinition.Height = new GridLength(1, GridUnitType.Star);
            SideBySideDiffViewRowDefinition.Height = new GridLength(0);
            SideBySideDiffViewer.Visibility = Visibility.Collapsed;
            TextEditorCore.Focus(FocusState.Programmatic);
        }

        public void ShowHideSideBySideDiffViewer()
        {
            if (SideBySideDiffViewer == null) LoadSideBySideDiffViewer();

            if (SideBySideDiffViewer.Visibility == Visibility.Collapsed)
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

        private KeyboardCommandHandler GetKeyboardCommandHandler()
        {
            return new KeyboardCommandHandler(new List<IKeyboardCommand<KeyRoutedEventArgs>>
            {
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.W, (args) => OnEditorClosingKeyDown?.Invoke(this, args)),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, false, false, VirtualKey.P, (args) => ShowHideContentPreview()),
                new KeyboardShortcut<KeyRoutedEventArgs>(true, true, false, VirtualKey.D, (args) => ShowHideSideBySideDiffViewer()),
                new KeyboardShortcut<KeyRoutedEventArgs>(false, false, false, VirtualKey.Escape, (args) =>
                {
                    if (SplitPanel != null && SplitPanel.Visibility == Visibility.Visible)
                    {
                        _contentPreviewExtension.IsExtensionEnabled = false;
                        CloseSplitView();
                    }
                }),
            });
        }

        public async Task SaveToFile(StorageFile file)
        {
            Encoding encoding = Encoding ?? new UTF8Encoding(false);
            var text = TextEditorCore.GetText();
            text = LineEndingUtility.ApplyLineEnding(text, LineEnding);
            await FileSystemUtility.WriteToFile(text, encoding, file);
            EditingFile = file;
            Encoding = encoding;
            Saved = true;
        }

        public string GetContentForSharing()
        {
            string content;

            if (TextEditorCore.Document.Selection.StartPosition == TextEditorCore.Document.Selection.EndPosition)
            {
                content = TextEditorCore.GetText();
            }
            else
            {
                content = TextEditorCore.Document.Selection.Text;
            }

            return content;
        }

        public bool FindNextAndReplace(string searchText, string replaceText, bool matchCase, bool matchWholeWord)
        {
            if (FindNextAndSelect(searchText, matchCase, matchWholeWord))
            {
                TextEditorCore.Document.Selection.SetText(TextSetOptions.None, replaceText);
                return true;
            }

            return false;
        }

        public bool FindAndReplaceAll(string searchText, string replaceText, bool matchCase, bool matchWholeWord)
        {
            var found = false;

            var pos = 0;
            var searchTextLength = searchText.Length;
            var replaceTextLength = replaceText.Length;

            var text = TextEditorCore.GetText();

            StringComparison comparison = matchCase ? StringComparison.Ordinal : StringComparison.OrdinalIgnoreCase;

            pos = matchWholeWord ? IndexOfWholeWord(text, pos, searchText, comparison) : text.IndexOf(searchText, pos, comparison);

            while (pos != -1)
            {
                found = true;
                text = text.Remove(pos, searchTextLength).Insert(pos, replaceText);
                pos += replaceTextLength;
                pos = matchWholeWord ? IndexOfWholeWord(text, pos, searchText, comparison) : text.IndexOf(searchText, pos, comparison);
            }

            if (found)
            {
                SetText(text);
                TextEditorCore.Document.Selection.StartPosition = Int32.MaxValue;
                TextEditorCore.Document.Selection.EndPosition = TextEditorCore.Document.Selection.StartPosition;
            }

            return found;
        }

        public bool FindNextAndSelect(string searchText, bool matchCase, bool matchWholeWord, bool stopAtEof = true)
        {
            if (string.IsNullOrEmpty(searchText))
            {
                return false;
            }

            var text = TextEditorCore.GetText();

            StringComparison comparison = matchCase ? StringComparison.Ordinal : StringComparison.OrdinalIgnoreCase;

            var index = matchWholeWord ? IndexOfWholeWord(text, TextEditorCore.Document.Selection.EndPosition, searchText, comparison) : text.IndexOf(searchText, TextEditorCore.Document.Selection.EndPosition, comparison);

            if (index != -1)
            {
                TextEditorCore.Document.Selection.StartPosition = index;
                TextEditorCore.Document.Selection.EndPosition = index + searchText.Length;
            }
            else
            {
                if (!stopAtEof)
                {
                    index = matchWholeWord ? IndexOfWholeWord(text, 0, searchText, comparison) : text.IndexOf(searchText, 0, comparison);

                    if (index != -1)
                    {
                        TextEditorCore.Document.Selection.StartPosition = index;
                        TextEditorCore.Document.Selection.EndPosition = index + searchText.Length;
                    }
                }
            }

            if (index == -1)
            {
                TextEditorCore.Document.Selection.StartPosition = TextEditorCore.Document.Selection.EndPosition;
                return false;
            }

            return true;
        }

        public void Focus()
        {
            if (SideBySideDiffViewer != null && SideBySideDiffViewer.Visibility == Visibility.Visible)
            {
                SideBySideDiffViewer.Focus();
            }
            else
            {
                TextEditorCore.Focus(FocusState.Programmatic);
            }
        }

        private void TextEditorCore_OnKeyDown(object sender, KeyRoutedEventArgs e)
        {
            _keyboardCommandHandler.Handle(e);
        }

        private void SplitPanel_OnKeyDown(object sender, KeyRoutedEventArgs e)
        {
            _keyboardCommandHandler.Handle(e);
        }

        public void SetText(string text)
        {
            TextEditorCore.Document.SetText(TextSetOptions.None, text);
        }

        public void TypeTab()
        {
            var tabStr = EditorSettingsService.EditorDefaultTabIndents == -1 ? "\t" : new string(' ', EditorSettingsService.EditorDefaultTabIndents);
            TextEditorCore.Document.Selection.TypeText(tabStr);
        }

        public void ClearUndoQueue()
        {
            // Clear UndoQueue by setting its limit to 0 and set it back
            var undoLimit = TextEditorCore.Document.UndoLimit;

            // Check to prevent the undo limit stuck on zero
            // because it returns 0 even if the undo limit isn't set yet
            if (undoLimit != 0)
            {
                TextEditorCore.Document.UndoLimit = 0;
                TextEditorCore.Document.UndoLimit = undoLimit;
            }
        }

        private static int IndexOfWholeWord(string target, int startIndex, string value, StringComparison comparison)
        {
            int pos = startIndex;
            while (pos < target.Length && (pos = target.IndexOf(value, pos, comparison)) != -1)
            {
                bool startBoundary = true;
                if (pos > 0)
                    startBoundary = !Char.IsLetterOrDigit(target[pos - 1]);

                bool endBoundary = true;
                if (pos + value.Length < target.Length)
                    endBoundary = !Char.IsLetterOrDigit(target[pos + value.Length]);

                if (startBoundary && endBoundary)
                    return pos;

                pos++;
            }
            return -1;
        }
    }
}