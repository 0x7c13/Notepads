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
                // If it is not UTF-8 meaning user is using ANSI decoding,
                // We should always try get latest system ANSI code page.
                if (!(_editorDefaultDecoding is UTF8Encoding))
                {
                    _editorDefaultDecoding = EncodingUtility.GetSystemCurrentANSIEncoding() ?? new UTF8Encoding(false);
                }
                return _editorDefaultDecoding;
            }
            set
            {
                _editorDefaultDecoding = value;
                ApplicationSettingsStore.Write(SettingsKey.EditorDefaultDecodingCodePageInt, value.CodePage, true);
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

        public static void Initialize()
        {
            InitializeFontSettings();

            InitializeTextWrappingSettings();

            InitializeSpellingSettings();

            InitializeLineEndingSettings();

            InitializeEncodingSettings();

            InitializeDecodingSettings();

            InitializeTabIndentsSettings();

            InitializeStatusBarSettings();

            InitializeSessionSnapshotSettings();

            InitializeAppOpeningPreferencesSettings();
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
            Encoding.RegisterProvider(System.Text.CodePagesEncodingProvider.Instance);

            if (ApplicationSettingsStore.Read(SettingsKey.EditorDefaultDecodingCodePageInt) is int decodingCodePage)
            {
                try
                {
                    _editorDefaultDecoding = Encoding.GetEncoding(decodingCodePage);
                    if (_editorDefaultDecoding is UTF8Encoding)
                    {
                        _editorDefaultDecoding = new UTF8Encoding(false);
                    }
                }
                catch (Exception ex)
                {
                    LoggingService.LogError($"[EditorSettingsService] Failed to get encoding, code page: {decodingCodePage}, ex: {ex.Message}");
                    _editorDefaultDecoding = new UTF8Encoding(false);
                }
            }
            else
            {
                _editorDefaultDecoding = new UTF8Encoding(false);
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
    }
}