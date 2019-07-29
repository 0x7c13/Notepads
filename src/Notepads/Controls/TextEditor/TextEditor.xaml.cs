
namespace Notepads.Controls.TextEditor
{
    using Notepads.Commands;
    using Notepads.Controls.FindAndReplace;
    using Notepads.EventArgs;
    using Notepads.Extensions;
    using Notepads.Services;
    using Notepads.Utilities;
    using System;
    using System.Collections.Generic;
    using System.Text;
    using System.Threading.Tasks;
    using Windows.ApplicationModel.Resources;
    using Windows.Storage;
    using Windows.System;
    using Windows.UI.Core;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;

    public enum TextEditorMode
    {
        Editing,
        DiffPreview
    }

    public sealed partial class TextEditor : UserControl
    {
        public FileType FileType { get; private set; }

        private StorageFile _editingFile;

        public StorageFile EditingFile
        {
            get => _editingFile;
            private set
            {
                FileType = value == null ? FileType.TextFile : FileTypeUtility.GetFileTypeByFileName(value.Name);
                _editingFile = value;
            }
        }

        private readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

        public INotepadsExtensionProvider ExtensionProvider;

        private readonly IKeyboardCommandHandler<KeyRoutedEventArgs> _keyboardCommandHandler;

        private IContentPreviewExtension _contentPreviewExtension;

        public event EventHandler ModeChanged;

        public event EventHandler ModificationStateChanged;

        public event EventHandler LineEndingChanged;

        public event EventHandler EncodingChanged;

        public event RoutedEventHandler SelectionChanged;

        public TextFile OriginalSnapshot { get; private set; }

        public LineEnding? TargetLineEnding { get; private set; }

        public Encoding TargetEncoding { get; private set; }

        private bool _loaded;

        private bool _isModified;

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

        private TextEditorMode _editorMode = TextEditorMode.Editing;

        public TextEditorMode EditorMode
        {
            get => _editorMode;
            private set
            {
                if (_editorMode != value)
                {
                    _editorMode = value;
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
            TextEditorCore.ContextFlyout = new TextEditorContextFlyout(this, this.TextEditorCore);

            // Init shortcuts
            _keyboardCommandHandler = GetKeyboardCommandHandler();

            ThemeSettingsService.OnThemeChanged += (sender, theme) =>
            {
                if (EditorMode == TextEditorMode.DiffPreview)
                {
                    SideBySideDiffViewer.RenderDiff(OriginalSnapshot.Content, TextEditorCore.GetText());
                    Task.Factory.StartNew(
                        () => Dispatcher.RunAsync(CoreDispatcherPriority.Low,
                            () => SideBySideDiffViewer.Focus()));
                }
            };
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
                new KeyboardShortcut<KeyRoutedEventArgs>(false, false, false, VirtualKey.Escape, (args) =>
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

        public void Init(TextFile textFile, StorageFile file, bool clearUndoQueue = true)
        {
            _loaded = false;
            EditingFile = file;
            TargetEncoding = null;
            TargetLineEnding = null;
            TextEditorCore.SetText(textFile.Content);
            OriginalSnapshot = new TextFile(TextEditorCore.GetText(), textFile.Encoding, textFile.LineEnding);
            if (clearUndoQueue)
            {
                TextEditorCore.ClearUndoQueue();
            }
            IsModified = false;
            _loaded = true;
        }

        public void RevertAllChanges()
        {
            if (EditorMode == TextEditorMode.DiffPreview)
            {
                CloseSideBySideDiffViewer();
            }
            Init(OriginalSnapshot, EditingFile, clearUndoQueue: false);
        }

        public bool TryChangeEncoding(Encoding encoding)
        {
            if (encoding == null) return false;

            if (!EncodingUtility.Equals(OriginalSnapshot.Encoding, encoding))
            {
                TargetEncoding = encoding;
                IsModified = true;
                EncodingChanged?.Invoke(this, EventArgs.Empty);
                return true;
            }

            if (TargetEncoding != null && EncodingUtility.Equals(OriginalSnapshot.Encoding, encoding))
            {
                TargetEncoding = null;
                IsModified = !IsInOriginalState();
                EncodingChanged?.Invoke(this, EventArgs.Empty);
                return true;
            }
            return false;
        }

        public bool TryChangeLineEnding(LineEnding lineEnding)
        {
            if (OriginalSnapshot.LineEnding != lineEnding)
            {
                TargetLineEnding = lineEnding;
                IsModified = true;
                LineEndingChanged?.Invoke(this, EventArgs.Empty);
                return true;
            }

            if (TargetLineEnding != null && OriginalSnapshot.LineEnding == lineEnding)
            {
                TargetLineEnding = null;
                IsModified = !IsInOriginalState();
                LineEndingChanged?.Invoke(this, EventArgs.Empty);
                return true;
            }
            return false;
        }

        public LineEnding GetLineEnding()
        {
            return TargetLineEnding ?? OriginalSnapshot.LineEnding;
        }

        public Encoding GetEncoding()
        {
            return TargetEncoding ?? OriginalSnapshot.Encoding;
        }

        private void OpenSplitView(IContentPreviewExtension extension)
        {
            SplitPanel.Content = extension;
            SplitPanelColumnDefinition.Width = new GridLength(ActualWidth / 2.0f);
            SplitPanelColumnDefinition.MinWidth = 100.0f;
            SplitPanel.Visibility = Visibility.Visible;
            GridSplitter.Visibility = Visibility.Visible;
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
                _contentPreviewExtension.Bind(this.TextEditorCore);
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
            if (string.Equals(OriginalSnapshot.Content, TextEditorCore.GetText())) return;
            if (SideBySideDiffViewer != null && SideBySideDiffViewer.Visibility == Visibility.Visible) return;
            if (SideBySideDiffViewer == null) LoadSideBySideDiffViewer();
            EditorMode = TextEditorMode.DiffPreview;
            TextEditorCore.IsEnabled = false;
            EditorRowDefinition.Height = new GridLength(0);
            SideBySideDiffViewRowDefinition.Height = new GridLength(1, GridUnitType.Star);
            SideBySideDiffViewer.Visibility = Visibility.Visible;
            SideBySideDiffViewer.RenderDiff(OriginalSnapshot.Content, TextEditorCore.GetText());
            Task.Factory.StartNew(
                () => Dispatcher.RunAsync(CoreDispatcherPriority.Low,
                    () => SideBySideDiffViewer.Focus()));
        }

        public void CloseSideBySideDiffViewer()
        {
            if (SideBySideDiffViewer == null || SideBySideDiffViewer.Visibility == Visibility.Collapsed) return;
            EditorMode = TextEditorMode.Editing;
            TextEditorCore.IsEnabled = true;
            EditorRowDefinition.Height = new GridLength(1, GridUnitType.Star);
            SideBySideDiffViewRowDefinition.Height = new GridLength(0);
            SideBySideDiffViewer.Visibility = Visibility.Collapsed;
            SideBySideDiffViewer.StopRenderingAndClearCache();
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

        public bool IsEditorEnabled()
        {
            return TextEditorCore.IsEnabled;
        }

        public async Task SaveToFile(StorageFile file)
        {
            if (SideBySideDiffViewer != null && SideBySideDiffViewer.Visibility == Visibility.Visible)
            {
                CloseSideBySideDiffViewer();
            }
            var text = TextEditorCore.GetText();
            var encoding = TargetEncoding ?? OriginalSnapshot.Encoding;
            var lineEnding = TargetLineEnding ?? OriginalSnapshot.LineEnding;
            await FileSystemUtility.WriteToFile(LineEndingUtility.ApplyLineEnding(text, lineEnding), encoding, file);
            Init(new TextFile(text, encoding, lineEnding), file, clearUndoQueue: false);
        }

        public string GetContentForSharing()
        {
            var content = TextEditorCore.Document.Selection.StartPosition == TextEditorCore.Document.Selection.EndPosition ?
                TextEditorCore.GetText() :
                TextEditorCore.Document.Selection.Text;

            return content;
        }

        public void TypeTab()
        {
            if (TextEditorCore.IsEnabled)
            {
                var tabStr = EditorSettingsService.EditorDefaultTabIndents == -1 ? "\t" : new string(' ', EditorSettingsService.EditorDefaultTabIndents);
                TextEditorCore.Document.Selection.TypeText(tabStr);
            }
        }

        public void Focus()
        {
            if (EditorMode == TextEditorMode.DiffPreview)
            {
                SideBySideDiffViewer.Focus();
            }
            else if (EditorMode == TextEditorMode.Editing)
            {
                TextEditorCore.Focus(FocusState.Programmatic);
            }
        }

        public bool IsInOriginalState(bool compareTextOnly = false)
        {
            if (!_loaded) return true;

            if (!compareTextOnly)
            {
                if (TargetLineEnding != null)
                {
                    return false;
                }

                if (TargetEncoding != null)
                {
                    return false;
                }
            }
            if (!string.Equals(OriginalSnapshot.Content, TextEditorCore.GetText()))
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
                IsModified = !IsInOriginalState();
            }
            else
            {
                IsModified = !IsInOriginalState(compareTextOnly: true);
            }
        }

        public void ShowFindAndReplaceControl(bool showReplaceBar)
        {
            if (!TextEditorCore.IsEnabled || EditorMode != TextEditorMode.Editing)
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

            Task.Factory.StartNew(
                () => Dispatcher.RunAsync(CoreDispatcherPriority.Low,
                    () => findAndReplace.Focus()));
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