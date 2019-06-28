
namespace Notepads.Services
{
    using Microsoft.Toolkit.Uwp.Helpers;
    using Microsoft.Toolkit.Uwp.UI.Helpers;
    using Notepads.Settings;
    using Notepads.Utilities;
    using System;
    using System.Collections.Generic;
    using System.Globalization;
    using System.Linq;
    using System.Runtime.CompilerServices;
    using System.Text;
    using System.Threading.Tasks;
    using Windows.Storage;
    using Windows.UI;
    using Windows.UI.Core;
    using Windows.UI.ViewManagement;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media;

    public static class EditorSettingsService
    {
        public static event EventHandler<string> OnFontFamilyChanged;

        public static event EventHandler<int> OnFontSizeChanged;

        public static event EventHandler<TextWrapping> OnDefaultTextWrappingChanged;

        public static event EventHandler<LineEnding> OnDefaultLineEndingChanged;

        public static event EventHandler<Encoding> OnDefaultEncodingChanged;

        public static event EventHandler<int> OnDefaultTabIndentsChanged;

        private static string _editorFontFamily;

        public static string EditorFontFamily
        {
            get => _editorFontFamily;
            set
            {
                _editorFontFamily = value;
                OnFontFamilyChanged?.Invoke(null, value);
                ApplicationSettings.WriteAsync(SettingsKey.EditorFontFamilyStr, value, true);
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
                ApplicationSettings.WriteAsync(SettingsKey.EditorFontSizeInt, value, true);
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
                ApplicationSettings.WriteAsync(SettingsKey.EditorDefaultTextWrappingStr, value.ToString(), true);
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
                ApplicationSettings.WriteAsync(SettingsKey.EditorDefaultLineEndingStr, value.ToString(), true);
            }
        }

        private static Encoding _editorDefaultEncoding;

        public static Encoding EditorDefaultEncoding
        {
            get => _editorDefaultEncoding;
            set
            {
                _editorDefaultEncoding = value;
                OnDefaultEncodingChanged?.Invoke(null, value);
                ApplicationSettings.WriteAsync(SettingsKey.EditorDefaultEncodingCodePageInt, value.CodePage, true);
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
                ApplicationSettings.WriteAsync(SettingsKey.EditorDefaultTabIndentsInt, value, true);
            }
        }


        public static void Initialize()
        {
            InitializeFontSettings();

            InitializeTextWrappingSettings();

            InitializeLineEndingSettings();

            InitializeEncodingSettings();

            InitializeTabIndentsSettings();
        }

        private static void InitializeLineEndingSettings()
        {
            if (ApplicationSettings.ReadAsync(SettingsKey.EditorDefaultLineEndingStr) is string lineEndingStr)
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
            if (ApplicationSettings.ReadAsync(SettingsKey.EditorDefaultTextWrappingStr) is string textWrappingStr)
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
            System.Text.EncodingProvider provider = System.Text.CodePagesEncodingProvider.Instance;
            Encoding.RegisterProvider(provider);

            if (ApplicationSettings.ReadAsync(SettingsKey.EditorDefaultEncodingCodePageInt) is int encodingCodePage)
            {
                _editorDefaultEncoding = Encoding.GetEncoding(encodingCodePage);
            }
            else
            {
                _editorDefaultEncoding = Encoding.UTF8;
            }
        }

        private static void InitializeTabIndentsSettings()
        {
            if (ApplicationSettings.ReadAsync(SettingsKey.EditorDefaultTabIndentsInt) is int tabIndents)
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
            if (ApplicationSettings.ReadAsync(SettingsKey.EditorFontFamilyStr) is string fontFamily)
            {
                _editorFontFamily = fontFamily;
            }
            else
            {
                _editorFontFamily = "Consolas";
            }

            if (ApplicationSettings.ReadAsync(SettingsKey.EditorFontSizeInt) is int fontSize)
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
