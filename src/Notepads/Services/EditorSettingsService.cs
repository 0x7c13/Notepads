namespace Notepads.Services
{
    using System;
    using System.Text;
    using Notepads.Settings;
    using Notepads.Utilities;
    using Windows.UI.Xaml;

    public static class EditorSettingsService
    {
        public static event EventHandler<string> OnFontFamilyChanged;

        public static event EventHandler<int> OnFontSizeChanged;

        public static event EventHandler<TextWrapping> OnDefaultTextWrappingChanged;

        public static event EventHandler<bool> OnDefaultLineHighlighterViewStateChanged;

        public static event EventHandler<LineEnding> OnDefaultLineEndingChanged;

        public static event EventHandler<Encoding> OnDefaultEncodingChanged;

        public static event EventHandler<int> OnDefaultTabIndentsChanged;

        public static event EventHandler<bool> OnStatusBarVisibilityChanged;

        public static event EventHandler<bool> OnSessionBackupAndRestoreOptionChanged;

        public static event EventHandler<bool> OnHighlightMisspelledWordsChanged;

        private static string _editorFontFamily;

        public static string EditorFontFamily
        {
            get => _editorFontFamily;
            set
            {
                _editorFontFamily = value;
                OnFontFamilyChanged?.Invoke(null, value);
                ApplicationSettingsStore.Write(SettingsKey.EditorFontFamilyStr, value, true);
            }
        }

        private static int _editorFontSize;

        public static int EditorFontSize
        {
            get => _editorFontSize;
            set
            {
                _editorFontSize = value;
                OnFontSizeChanged?.Invoke(null, value);
                ApplicationSettingsStore.Write(SettingsKey.EditorFontSizeInt, value, true);
            }
        }

        private static TextWrapping _editorDefaultTextWrapping;

        public static TextWrapping EditorDefaultTextWrapping
        {
            get => _editorDefaultTextWrapping;
            set
            {
                _editorDefaultTextWrapping = value;
                OnDefaultTextWrappingChanged?.Invoke(null, value);
                ApplicationSettingsStore.Write(SettingsKey.EditorDefaultTextWrappingStr, value.ToString(), true);
            }
        }

        private static bool _isLineHighlighterEnabled;

        public static bool IsLineHighlighterEnabled
        {
            get => _isLineHighlighterEnabled;
            set
            {
                _isLineHighlighterEnabled = value;
                OnDefaultLineHighlighterViewStateChanged?.Invoke(null, value);
                ApplicationSettingsStore.Write(SettingsKey.EditorDefaultLineHighlighterViewStateBool, value, true);
            }
        }

        private static LineEnding _editorDefaultLineEnding;

        public static LineEnding EditorDefaultLineEnding
        {
            get => _editorDefaultLineEnding;
            set
            {
                _editorDefaultLineEnding = value;
                OnDefaultLineEndingChanged?.Invoke(null, value);
                ApplicationSettingsStore.Write(SettingsKey.EditorDefaultLineEndingStr, value.ToString(), true);
            }
        }

        private static Encoding _editorDefaultEncoding;

        public static Encoding EditorDefaultEncoding
        {
            get => _editorDefaultEncoding;
            set
            {
                _editorDefaultEncoding = value;

                if (value is UTF8Encoding)
                {
                    ApplicationSettingsStore.Write(SettingsKey.EditorDefaultUtf8EncoderShouldEmitByteOrderMarkBool,
                        Equals(value, new UTF8Encoding(true)), true);
                }

                OnDefaultEncodingChanged?.Invoke(null, value);
                ApplicationSettingsStore.Write(SettingsKey.EditorDefaultEncodingCodePageInt, value.CodePage, true);
            }
        }

        private static Encoding _editorDefaultDecoding;

        public static Encoding EditorDefaultDecoding
        {
            get
            {
                if (_editorDefaultDecoding == null)
                {
                    return null;
                }
                // If it is not UTF-8 meaning user is using ANSI decoding,
                // We should always try get latest system ANSI code page.
                else if (!(_editorDefaultDecoding is UTF8Encoding))
                {
                    if (EncodingUtility.TryGetSystemDefaultANSIEncoding(out var systemDefaultANSIEncoding))
                    {
                        _editorDefaultDecoding = systemDefaultANSIEncoding;
                    }
                    else if (EncodingUtility.TryGetCurrentCultureANSIEncoding(out var currentCultureANSIEncoding))
                    {
                        _editorDefaultDecoding = currentCultureANSIEncoding;
                    }
                    else
                    {
                        _editorDefaultDecoding = new UTF8Encoding(false); // Fall back to UTF-8 (no BOM)
                    }
                }
                return _editorDefaultDecoding;
            }
            set
            {
                _editorDefaultDecoding = value;
                var codePage = value?.CodePage ?? -1;
                ApplicationSettingsStore.Write(SettingsKey.EditorDefaultDecodingCodePageInt, codePage, true);
            }
        }

        private static int _editorDefaultTabIndents;

        public static int EditorDefaultTabIndents
        {
            get => _editorDefaultTabIndents;
            set
            {
                _editorDefaultTabIndents = value;
                OnDefaultTabIndentsChanged?.Invoke(null, value);
                ApplicationSettingsStore.Write(SettingsKey.EditorDefaultTabIndentsInt, value, true);
            }
        }

        private static SearchEngine _editorDefaultSearchEngine;

        public static SearchEngine EditorDefaultSearchEngine
        {
            get => _editorDefaultSearchEngine;
            set
            {
                _editorDefaultSearchEngine = value;
                ApplicationSettingsStore.Write(SettingsKey.EditorDefaultSearchEngineStr, value.ToString(), true);
            }
        }

        private static string _editorCustomMadeSearchUrl;

        public static string EditorCustomMadeSearchUrl
        {
            get => _editorCustomMadeSearchUrl;
            set
            {
                _editorCustomMadeSearchUrl = value;
                ApplicationSettingsStore.Write(SettingsKey.EditorCustomMadeSearchUrlStr, value, true);
            }
        }

        private static bool _showStatusBar;

        public static bool ShowStatusBar
        {
            get => _showStatusBar;
            set
            {
                _showStatusBar = value;
                OnStatusBarVisibilityChanged?.Invoke(null, value);
                ApplicationSettingsStore.Write(SettingsKey.EditorShowStatusBarBool, value, true);
            }
        }

        private static bool _isSessionSnapshotEnabled;

        public static bool IsSessionSnapshotEnabled
        {
            get => _isSessionSnapshotEnabled;
            set
            {
                _isSessionSnapshotEnabled = value;
                OnSessionBackupAndRestoreOptionChanged?.Invoke(null, value);
                ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, value, true);
            }
        }

        public static bool _isHighlightMisspelledWordsEnabled;

        public static bool IsHighlightMisspelledWordsEnabled
        {
            get => _isHighlightMisspelledWordsEnabled;
            set
            {
                _isHighlightMisspelledWordsEnabled = value;
                OnHighlightMisspelledWordsChanged?.Invoke(null, value);
                ApplicationSettingsStore.Write(SettingsKey.EditorHighlightMisspelledWordsBool, value, true);
            }
        }

        public static bool _alwaysOpenNewWindow;

        public static bool AlwaysOpenNewWindow
        {
            get => _alwaysOpenNewWindow;
            set
            {
                _alwaysOpenNewWindow = value;
                ApplicationSettingsStore.Write(SettingsKey.AlwaysOpenNewWindowBool, value, true);
            }
        }

        private static bool _enableLogEntry;

        public static bool EnableLogEntry
        {
            get => _enableLogEntry;
            set
            {
                _enableLogEntry = value;
                ApplicationSettingsStore.Write(SettingsKey.EditorEnableLogEntryBool, value, true);
            }
        }

        public static void Initialize()
        {
            InitializeFontSettings();

            InitializeTextWrappingSettings();

            InitializeSpellingSettings();

            InitializeLineHighlighterSettings();

            InitializeLineEndingSettings();

            InitializeEncodingSettings();

            InitializeDecodingSettings();

            InitializeTabIndentsSettings();

            InitializeSearchEngineSettings();

            InitializeStatusBarSettings();

            InitializeSessionSnapshotSettings();

            InitializeAppOpeningPreferencesSettings();

            InitializeLogEntrySettings();
        }

        private static void InitializeStatusBarSettings()
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorShowStatusBarBool) is bool showStatusBar)
            {
                _showStatusBar = showStatusBar;
            }
            else
            {
                _showStatusBar = true;
            }
        }

        private static void InitializeSessionSnapshotSettings()
        {
            // We should disable session snapshot feature on multi instances
            if (!App.IsFirstInstance)
            {
                _isSessionSnapshotEnabled = false;
            }
            else
            {
                if (ApplicationSettingsStore.Read(SettingsKey.EditorEnableSessionBackupAndRestoreBool) is bool enableSessionBackupAndRestore)
                {
                    _isSessionSnapshotEnabled = enableSessionBackupAndRestore;
                }
                else
                {
                    _isSessionSnapshotEnabled = false;
                }
            }
        }

        private static void InitializeLineEndingSettings()
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorDefaultLineEndingStr) is string lineEndingStr)
            {
                Enum.TryParse(typeof(LineEnding), lineEndingStr, out var lineEnding);
                _editorDefaultLineEnding = (LineEnding)lineEnding;
            }
            else
            {
                _editorDefaultLineEnding = LineEnding.Crlf;
            }
        }

        private static void InitializeTextWrappingSettings()
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorDefaultTextWrappingStr) is string textWrappingStr)
            {
                Enum.TryParse(typeof(TextWrapping), textWrappingStr, out var textWrapping);
                _editorDefaultTextWrapping = (TextWrapping)textWrapping;
            }
            else
            {
                _editorDefaultTextWrapping = TextWrapping.NoWrap;
            }
        }

        private static void InitializeSpellingSettings()
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorHighlightMisspelledWordsBool) is bool highlightMisspelledWords)
            {
                _isHighlightMisspelledWordsEnabled = highlightMisspelledWords;
            }
            else
            {
                _isHighlightMisspelledWordsEnabled = false;
            }
        }

        private static void InitializeLineHighlighterSettings()
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorDefaultLineHighlighterViewStateBool) is bool isLineHighlighterEnabled)
            {
                _isLineHighlighterEnabled = isLineHighlighterEnabled;
            }
            else
            {
                _isLineHighlighterEnabled = true;
            }
        }

        private static void InitializeEncodingSettings()
        {
            Encoding.RegisterProvider(System.Text.CodePagesEncodingProvider.Instance);

            if (ApplicationSettingsStore.Read(SettingsKey.EditorDefaultEncodingCodePageInt) is int encodingCodePage)
            {
                var encoding = Encoding.GetEncoding(encodingCodePage);

                if (encoding is UTF8Encoding)
                {
                    if (ApplicationSettingsStore.Read(SettingsKey.EditorDefaultUtf8EncoderShouldEmitByteOrderMarkBool) is bool shouldEmitBom)
                    {
                        encoding = new UTF8Encoding(shouldEmitBom);
                    }
                    else
                    {
                        encoding = new UTF8Encoding(false);
                    }
                }

                _editorDefaultEncoding = encoding;
            }
            else
            {
                _editorDefaultEncoding = new UTF8Encoding(false);
            }
        }

        private static void InitializeDecodingSettings()
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorDefaultDecodingCodePageInt) is int decodingCodePage)
            {
                try
                {
                    if (decodingCodePage == -1)
                    {
                        _editorDefaultDecoding = null; // Meaning we should guess encoding during runtime
                    }
                    else
                    {
                        Encoding.RegisterProvider(System.Text.CodePagesEncodingProvider.Instance);
                        _editorDefaultDecoding = Encoding.GetEncoding(decodingCodePage);
                        if (_editorDefaultDecoding is UTF8Encoding)
                        {
                            _editorDefaultDecoding = new UTF8Encoding(false);
                        }
                    }
                }
                catch (Exception ex)
                {
                    LoggingService.LogError($"[EditorSettingsService] Failed to get encoding, code page: {decodingCodePage}, ex: {ex.Message}");
                    _editorDefaultDecoding = null;
                }
            }
            else
            {
                _editorDefaultDecoding = null; // Default to null
            }
        }

        private static void InitializeTabIndentsSettings()
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorDefaultTabIndentsInt) is int tabIndents)
            {
                _editorDefaultTabIndents = tabIndents;
            }
            else
            {
                _editorDefaultTabIndents = -1;
            }
        }

        private static void InitializeSearchEngineSettings()
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorDefaultSearchEngineStr) is string searchEngineStr && ApplicationSettingsStore.Read(SettingsKey.EditorCustomMadeSearchUrlStr) is string customMadesearchUrl)
            {
                if(Enum.TryParse(typeof(SearchEngine), searchEngineStr, out var searchEngine))
                    _editorDefaultSearchEngine = (SearchEngine)searchEngine;
                else
                    _editorDefaultSearchEngine = SearchEngine.Bing;
                _editorCustomMadeSearchUrl = customMadesearchUrl;
            }
            else
            {
                _editorDefaultSearchEngine = SearchEngine.Bing;
                _editorCustomMadeSearchUrl = string.Empty;
            }
        }

        private static void InitializeFontSettings()
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorFontFamilyStr) is string fontFamily)
            {
                _editorFontFamily = fontFamily;
            }
            else
            {
                _editorFontFamily = "Consolas";
            }

            if (ApplicationSettingsStore.Read(SettingsKey.EditorFontSizeInt) is int fontSize)
            {
                _editorFontSize = fontSize;
            }
            else
            {
                _editorFontSize = 14;
            }
        }

        private static void InitializeAppOpeningPreferencesSettings()
        {
            if (ApplicationSettingsStore.Read(SettingsKey.AlwaysOpenNewWindowBool) is bool alwaysOpenNewWindow)
            {
                _alwaysOpenNewWindow = alwaysOpenNewWindow;
            }
            else
            {
                _alwaysOpenNewWindow = false;
            }
        }

        private static void InitializeLogEntrySettings()
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorEnableLogEntryBool) is bool enableLogEntry)
            {
                _enableLogEntry = enableLogEntry;
            }
            else
            {
                _enableLogEntry = true;
            }
        }
    }
}