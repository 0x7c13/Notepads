namespace Notepads.Services
{
    using System;
    using System.Text;
    using Notepads.Settings;
    using Notepads.Utilities;
    using Windows.UI.Text;
    using Windows.UI.Xaml;

    public static class AppSettingsService
    {
        public static event EventHandler<string> OnFontFamilyChanged;
        public static event EventHandler<FontStyle> OnFontStyleChanged;
        public static event EventHandler<FontWeight> OnFontWeightChanged;
        public static event EventHandler<int> OnFontSizeChanged;
        public static event EventHandler<TextWrapping> OnDefaultTextWrappingChanged;
        public static event EventHandler<bool> OnDefaultLineHighlighterViewStateChanged;
        public static event EventHandler<bool> OnDefaultDisplayLineNumbersViewStateChanged;
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
                ApplicationSettingsStore.Write(SettingsKey.EditorFontFamilyStr, value);
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
                ApplicationSettingsStore.Write(SettingsKey.EditorFontSizeInt, value);
            }
        }

        private static FontStyle _editorFontStyle;

        public static FontStyle EditorFontStyle
        {
            get => _editorFontStyle;
            set
            {
                _editorFontStyle = value;
                OnFontStyleChanged?.Invoke(null, value);
                ApplicationSettingsStore.Write(SettingsKey.EditorFontStyleStr, value.ToString());
            }
        }

        private static FontWeight _editorFontWeight;

        public static FontWeight EditorFontWeight
        {
            get => _editorFontWeight;
            set
            {
                _editorFontWeight = value;
                OnFontWeightChanged?.Invoke(null, value);
                ApplicationSettingsStore.Write(SettingsKey.EditorFontWeightUshort, value.Weight);
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
                ApplicationSettingsStore.Write(SettingsKey.EditorDefaultTextWrappingStr, value.ToString());
            }
        }

        private static bool _editorDisplayLineHighlighter;

        public static bool EditorDisplayLineHighlighter
        {
            get => _editorDisplayLineHighlighter;
            set
            {
                _editorDisplayLineHighlighter = value;
                OnDefaultLineHighlighterViewStateChanged?.Invoke(null, value);
                ApplicationSettingsStore.Write(SettingsKey.EditorDefaultLineHighlighterViewStateBool, value);
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
                ApplicationSettingsStore.Write(SettingsKey.EditorDefaultLineEndingStr, value.ToString());
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
                        Equals(value, new UTF8Encoding(true)));
                }

                OnDefaultEncodingChanged?.Invoke(null, value);
                ApplicationSettingsStore.Write(SettingsKey.EditorDefaultEncodingCodePageInt, value.CodePage);
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
                ApplicationSettingsStore.Write(SettingsKey.EditorDefaultDecodingCodePageInt, codePage);
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
                ApplicationSettingsStore.Write(SettingsKey.EditorDefaultTabIndentsInt, value);
            }
        }

        private static SearchEngine _editorDefaultSearchEngine;

        public static SearchEngine EditorDefaultSearchEngine
        {
            get => _editorDefaultSearchEngine;
            set
            {
                _editorDefaultSearchEngine = value;
                ApplicationSettingsStore.Write(SettingsKey.EditorDefaultSearchEngineStr, value.ToString());
            }
        }

        private static string _editorCustomMadeSearchUrl;

        public static string EditorCustomMadeSearchUrl
        {
            get => _editorCustomMadeSearchUrl;
            set
            {
                _editorCustomMadeSearchUrl = value;
                ApplicationSettingsStore.Write(SettingsKey.EditorCustomMadeSearchUrlStr, value);
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
                ApplicationSettingsStore.Write(SettingsKey.EditorShowStatusBarBool, value);
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
                ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, value);
            }
        }

        private static bool _isHighlightMisspelledWordsEnabled;

        public static bool IsHighlightMisspelledWordsEnabled
        {
            get => _isHighlightMisspelledWordsEnabled;
            set
            {
                _isHighlightMisspelledWordsEnabled = value;
                OnHighlightMisspelledWordsChanged?.Invoke(null, value);
                ApplicationSettingsStore.Write(SettingsKey.EditorHighlightMisspelledWordsBool, value);
            }
        }

        private static bool _alwaysOpenNewWindow;

        public static bool AlwaysOpenNewWindow
        {
            get => _alwaysOpenNewWindow;
            set
            {
                _alwaysOpenNewWindow = value;
                ApplicationSettingsStore.Write(SettingsKey.AlwaysOpenNewWindowBool, value);
            }
        }

        private static bool _displayLineNumbers;

        public static bool EditorDisplayLineNumbers
        {
            get => _displayLineNumbers;
            set
            {
                _displayLineNumbers = value;
                OnDefaultDisplayLineNumbersViewStateChanged?.Invoke(null, value);
                ApplicationSettingsStore.Write(SettingsKey.EditorDefaultDisplayLineNumbersBool, value);
            }
        }

        private static bool _isSmartCopyEnabled;

        public static bool IsSmartCopyEnabled
        {
            get => _isSmartCopyEnabled;
            set
            {
                _isSmartCopyEnabled = value;
                ApplicationSettingsStore.Write(SettingsKey.EditorEnableSmartCopyBool, value);
            }
        }

        public static void Initialize(bool shouldInvokeChangedEvent = false)
        {
            InitializeFontFamilySettings(shouldInvokeChangedEvent);
            InitializeFontSizeSettings(shouldInvokeChangedEvent);
            InitializeFontStyleSettings(shouldInvokeChangedEvent);
            InitializeFontWeightSettings(shouldInvokeChangedEvent);

            InitializeTextWrappingSettings(shouldInvokeChangedEvent);

            InitializeSpellingSettings(shouldInvokeChangedEvent);

            InitializeDisplayLineHighlighterSettings(shouldInvokeChangedEvent);
            InitializeDisplayLineNumbersSettings(shouldInvokeChangedEvent);

            InitializeSmartCopySettings(shouldInvokeChangedEvent);

            InitializeLineEndingSettings(shouldInvokeChangedEvent);

            InitializeEncodingSettings(shouldInvokeChangedEvent);

            InitializeDecodingSettings(shouldInvokeChangedEvent);

            InitializeTabIndentsSettings(shouldInvokeChangedEvent);

            InitializeSearchEngineSettings(shouldInvokeChangedEvent);
            InitializeCustomSearchUrlSettings(shouldInvokeChangedEvent);

            InitializeStatusBarSettings(shouldInvokeChangedEvent);

            InitializeSessionSnapshotSettings(shouldInvokeChangedEvent);

            InitializeAppOpeningPreferencesSettings(shouldInvokeChangedEvent);
        }

        public static void InitializeStatusBarSettings(bool invokeChangedEvent = false)
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorShowStatusBarBool) is bool showStatusBar)
            {
                _showStatusBar = showStatusBar;
            }
            else
            {
                _showStatusBar = true;
            }

            if (invokeChangedEvent) OnStatusBarVisibilityChanged?.Invoke(null, _showStatusBar);
        }

        private static void InitializeSessionSnapshotSettings(bool invokeChangedEvent = false)
        {
            // We should disable session snapshot feature on multi instances
            if (!App.IsPrimaryInstance)
            {
                _isSessionSnapshotEnabled = false;
            }
            else if (App.IsGameBarWidget)
            {
                _isSessionSnapshotEnabled = true;
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

        public static void InitializeLineEndingSettings(bool invokeChangedEvent = false)
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorDefaultLineEndingStr) is string lineEndingStr &&
                Enum.TryParse(typeof(LineEnding), lineEndingStr, out var lineEnding))
            {
                _editorDefaultLineEnding = (LineEnding)lineEnding;
            }
            else
            {
                _editorDefaultLineEnding = LineEnding.Crlf;
            }

            if (invokeChangedEvent) OnDefaultLineEndingChanged?.Invoke(null, _editorDefaultLineEnding);
        }

        public static void InitializeTextWrappingSettings(bool invokeChangedEvent = false)
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorDefaultTextWrappingStr) is string textWrappingStr &&
                Enum.TryParse(typeof(TextWrapping), textWrappingStr, out var textWrapping))
            {
                _editorDefaultTextWrapping = (TextWrapping)textWrapping;
            }
            else
            {
                _editorDefaultTextWrapping = TextWrapping.NoWrap;
            }

            if (invokeChangedEvent) OnDefaultTextWrappingChanged?.Invoke(null, _editorDefaultTextWrapping);
        }

        public static void InitializeSpellingSettings(bool invokeChangedEvent = false)
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorHighlightMisspelledWordsBool) is bool highlightMisspelledWords)
            {
                _isHighlightMisspelledWordsEnabled = highlightMisspelledWords;
            }
            else
            {
                _isHighlightMisspelledWordsEnabled = false;
            }

            if (invokeChangedEvent) OnHighlightMisspelledWordsChanged?.Invoke(null, _isHighlightMisspelledWordsEnabled);
        }

        public static void InitializeDisplayLineHighlighterSettings(bool invokeChangedEvent = false)
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorDefaultLineHighlighterViewStateBool) is bool displayLineHighlighter)
            {
                _editorDisplayLineHighlighter = displayLineHighlighter;
            }
            else
            {
                _editorDisplayLineHighlighter = true;
            }

            if (invokeChangedEvent) OnDefaultLineHighlighterViewStateChanged?.Invoke(null, _editorDisplayLineHighlighter);
        }

        public static void InitializeDisplayLineNumbersSettings(bool invokeChangedEvent = false)
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorDefaultDisplayLineNumbersBool) is bool displayLineNumbers)
            {
                _displayLineNumbers = displayLineNumbers;
            }
            else
            {
                _displayLineNumbers = true;
            }

            if (invokeChangedEvent) OnDefaultDisplayLineNumbersViewStateChanged?.Invoke(null, _displayLineNumbers);
        }

        public static void InitializeSmartCopySettings(bool invokeChangedEvent = false)
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorEnableSmartCopyBool) is bool enableSmartCopy)
            {
                _isSmartCopyEnabled = enableSmartCopy;
            }
            else
            {
                _isSmartCopyEnabled = false;
            }
        }

        public static void InitializeEncodingSettings(bool invokeChangedEvent = false)
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

            if (invokeChangedEvent) OnDefaultEncodingChanged?.Invoke(null, _editorDefaultEncoding);
        }

        public static void InitializeDecodingSettings(bool invokeChangedEvent = false)
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
                    LoggingService.LogError($"[{nameof(AppSettingsService)}] Failed to get encoding, code page: {decodingCodePage}, ex: {ex.Message}");
                    _editorDefaultDecoding = null;
                }
            }
            else
            {
                _editorDefaultDecoding = null; // Default to null
            }
        }

        public static void InitializeTabIndentsSettings(bool invokeChangedEvent = false)
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorDefaultTabIndentsInt) is int tabIndents)
            {
                _editorDefaultTabIndents = tabIndents;
            }
            else
            {
                _editorDefaultTabIndents = -1;
            }

            if (invokeChangedEvent) OnDefaultTabIndentsChanged?.Invoke(null, _editorDefaultTabIndents);
        }

        public static void InitializeSearchEngineSettings(bool invokeChangedEvent = false)
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorDefaultSearchEngineStr) is string searchEngineStr &&
                Enum.TryParse(typeof(SearchEngine), searchEngineStr, out var searchEngine))
            {
                _editorDefaultSearchEngine = (SearchEngine)searchEngine;
            }
            else
            {
                _editorDefaultSearchEngine = SearchEngine.Bing;
            }
        }

        public static void InitializeCustomSearchUrlSettings(bool invokeChangedEvent = false)
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorCustomMadeSearchUrlStr) is string customMadeSearchUrl)
            {
                _editorCustomMadeSearchUrl = customMadeSearchUrl;
            }
            else
            {
                _editorCustomMadeSearchUrl = string.Empty;
            }
        }

        public static void InitializeFontFamilySettings(bool invokeChangedEvent = false)
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorFontFamilyStr) is string fontFamily)
            {
                _editorFontFamily = fontFamily;
            }
            else
            {
                _editorFontFamily = "Consolas";
            }

            if (invokeChangedEvent) OnFontFamilyChanged?.Invoke(null, _editorFontFamily);
        }

        public static void InitializeFontSizeSettings(bool invokeChangedEvent = false)
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorFontSizeInt) is int fontSize)
            {
                _editorFontSize = fontSize;
            }
            else
            {
                _editorFontSize = 14;
            }

            if (invokeChangedEvent) OnFontSizeChanged?.Invoke(null, _editorFontSize);
        }

        public static void InitializeFontStyleSettings(bool invokeChangedEvent = false)
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorFontStyleStr) is string fontStyleStr &&
                Enum.TryParse(typeof(FontStyle), fontStyleStr, out var fontStyle))
            {
                _editorFontStyle = (FontStyle)fontStyle;
            }
            else
            {
                _editorFontStyle = FontStyle.Normal;
            }

            if (invokeChangedEvent) OnFontStyleChanged?.Invoke(null, _editorFontStyle);
        }

        public static void InitializeFontWeightSettings(bool invokeChangedEvent = false)
        {
            if (ApplicationSettingsStore.Read(SettingsKey.EditorFontWeightUshort) is ushort fontWeight)
            {
                _editorFontWeight = new FontWeight()
                {
                    Weight = fontWeight
                };
            }
            else
            {
                _editorFontWeight = FontWeights.Normal;
            }

            if (invokeChangedEvent) OnFontWeightChanged?.Invoke(null, _editorFontWeight);
        }

        public static void InitializeAppOpeningPreferencesSettings(bool invokeChangedEvent = false)
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
    }
}