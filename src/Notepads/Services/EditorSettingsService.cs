
namespace Notepads.Services
{
    using Notepads.Settings;
    using Notepads.Utilities;
    using System;
    using System.Text;
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

        private static string _editorFontFamily;

        public static string EditorFontFamily
        {
            get => _editorFontFamily;
            set
            {
                _editorFontFamily = value;
                OnFontFamilyChanged?.Invoke(null, value);
                ApplicationSettings.Write(SettingsKey.EditorFontFamilyStr, value, true);
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
                ApplicationSettings.Write(SettingsKey.EditorFontSizeInt, value, true);
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
                ApplicationSettings.Write(SettingsKey.EditorDefaultTextWrappingStr, value.ToString(), true);
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
                ApplicationSettings.Write(SettingsKey.EditorDefaultLineEndingStr, value.ToString(), true);
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
                    ApplicationSettings.Write(SettingsKey.EditorDefaultUtf8EncoderShouldEmitByteOrderMarkBool,
                        Equals(value, new UTF8Encoding(true)), true);
                }

                OnDefaultEncodingChanged?.Invoke(null, value);
                ApplicationSettings.Write(SettingsKey.EditorDefaultEncodingCodePageInt, value.CodePage, true);
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
                    _editorDefaultDecoding = EncodingUtility.GetSystemCurrentANSIEncoding();
                }
                return _editorDefaultDecoding;
            }
            set
            {
                _editorDefaultDecoding = value;
                ApplicationSettings.Write(SettingsKey.EditorDefaultDecodingCodePageInt, value.CodePage, true);
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
                ApplicationSettings.Write(SettingsKey.EditorDefaultTabIndentsInt, value, true);
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
                ApplicationSettings.Write(SettingsKey.EditorShowStatusBarBool, value, true);
            }
        }

        private static bool _isSessionBackupAndRestoreEnabled;

        public static bool IsSessionBackupAndRestoreEnabled
        {
            get => _isSessionBackupAndRestoreEnabled;
            set
            {
                _isSessionBackupAndRestoreEnabled = value;
                OnSessionBackupAndRestoreOptionChanged?.Invoke(null, value);
                ApplicationSettings.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, value, true);
            }
        }


        public static void Initialize()
        {
            InitializeFontSettings();

            InitializeTextWrappingSettings();

            InitializeLineEndingSettings();

            InitializeEncodingSettings();

            InitializeDecodingSettings();

            InitializeTabIndentsSettings();

            InitializeStatusBarSettings();

            InitializeSessionBackupAndRestoreSettings();
        }

        private static void InitializeStatusBarSettings()
        {
            if (ApplicationSettings.Read(SettingsKey.EditorShowStatusBarBool) is bool showStatusBar)
            {
                _showStatusBar = showStatusBar;
            }
            else
            {
                _showStatusBar = true;
            }
        }

        private static void InitializeSessionBackupAndRestoreSettings()
        {
            if (ApplicationSettings.Read(SettingsKey.EditorEnableSessionBackupAndRestoreBool) is bool enableSessionBackupAndRestore)
            {
                _isSessionBackupAndRestoreEnabled = enableSessionBackupAndRestore;
            }
            else
            {
                _isSessionBackupAndRestoreEnabled = false;
            }
        }

        private static void InitializeLineEndingSettings()
        {
            if (ApplicationSettings.Read(SettingsKey.EditorDefaultLineEndingStr) is string lineEndingStr)
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
            if (ApplicationSettings.Read(SettingsKey.EditorDefaultTextWrappingStr) is string textWrappingStr)
            {
                Enum.TryParse(typeof(TextWrapping), textWrappingStr, out var textWrapping);
                _editorDefaultTextWrapping = (TextWrapping)textWrapping;
            }
            else
            {
                _editorDefaultTextWrapping = TextWrapping.NoWrap;
            }
        }

        private static void InitializeEncodingSettings()
        {
            Encoding.RegisterProvider(System.Text.CodePagesEncodingProvider.Instance);

            if (ApplicationSettings.Read(SettingsKey.EditorDefaultEncodingCodePageInt) is int encodingCodePage)
            {
                var encoding = Encoding.GetEncoding(encodingCodePage);

                if (encoding is UTF8Encoding)
                {
                    if (ApplicationSettings.Read(SettingsKey.EditorDefaultUtf8EncoderShouldEmitByteOrderMarkBool) is bool shouldEmitBom)
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

            if (ApplicationSettings.Read(SettingsKey.EditorDefaultDecodingCodePageInt) is int decodingCodePage)
            {
                try
                {
                    _editorDefaultDecoding = Encoding.GetEncoding(decodingCodePage);
                    if (_editorDefaultDecoding is UTF8Encoding)
                    {
                        _editorDefaultDecoding = new UTF8Encoding(false);
                    }
                }
                catch (Exception)
                {
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
            if (ApplicationSettings.Read(SettingsKey.EditorDefaultTabIndentsInt) is int tabIndents)
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
            if (ApplicationSettings.Read(SettingsKey.EditorFontFamilyStr) is string fontFamily)
            {
                _editorFontFamily = fontFamily;
            }
            else
            {
                _editorFontFamily = "Consolas";
            }

            if (ApplicationSettings.Read(SettingsKey.EditorFontSizeInt) is int fontSize)
            {
                _editorFontSize = fontSize;
            }
            else
            {
                _editorFontSize = 14;
            }
        }
    }
}
