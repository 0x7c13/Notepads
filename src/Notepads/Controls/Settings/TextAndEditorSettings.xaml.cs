namespace Notepads.Controls.Settings
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using System.Text;
    using Services;
    using Utilities;
    using Windows.Globalization;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Microsoft.AppCenter.Analytics;

    public sealed partial class TextAndEditorSettings : Page
    {
        /// <summary>
        /// The collection of symbol fonts that need to be skipped from the available fonts, as they don't produce readable text
        /// </summary>
        private static readonly IReadOnlyCollection<string> SymbolFonts = new HashSet<string>(new[]
        {
            "Segoe MDL2 Assets",
            "Webdings",
            "Wingdings",
            "HoloLens MDL2 Assets",
            "Bookshelf Symbol 7",
            "MT Extra",
            "MS Outlook",
            "MS Reference Specialty",
            "Wingdings 2",
            "Wingdings 3",
            "Marlett"
        });

        private IReadOnlyCollection<string> _availableFonts;

        /// <summary>
        /// Gets the collection of fonts that the user can choose from System Fonts
        /// </summary>
        public IReadOnlyCollection<string> AvailableFonts
        {
            get
            {
                if (_availableFonts == null)
                {
                    try
                    {
                        var systemFonts = Microsoft.Graphics.Canvas.Text.CanvasTextFormat.GetSystemFontFamilies(ApplicationLanguages.Languages);
                        _availableFonts = systemFonts.Where(font => !SymbolFonts.Contains(font)).OrderBy(font => font).ToArray();
                    }
                    catch (Exception ex)
                    {
                        Analytics.TrackEvent("FailedToGetSystemFontFamilies", new Dictionary<string, string>()
                        {
                            { "Exception", ex.ToString() }
                        });
                        _availableFonts = new List<string>();
                    }
                }
                return _availableFonts;
            }
        }

        public int[] FontSizes = new int[]
        {
            8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 20, 22, 24, 26, 28, 36, 48, 72
        };

        public TextAndEditorSettings()
        {
            InitializeComponent();

            TextWrappingToggle.IsOn = (EditorSettingsService.EditorDefaultTextWrapping == TextWrapping.Wrap);
            HighlightMisspelledWordsToggle.IsOn = EditorSettingsService.IsHighlightMisspelledWordsEnabled;
            LineHighlighterToggle.IsOn = EditorSettingsService.IsLineHighlighterEnabled;
            FontFamilyPicker.SelectedItem = EditorSettingsService.EditorFontFamily;
            FontSizePicker.SelectedItem = EditorSettingsService.EditorFontSize;

            // Line Ending
            switch (EditorSettingsService.EditorDefaultLineEnding)
            {
                case LineEnding.Crlf:
                    CrlfRadioButton.IsChecked = true;
                    break;
                case LineEnding.Cr:
                    CrRadioButton.IsChecked = true;
                    break;
                case LineEnding.Lf:
                    LfRadioButton.IsChecked = true;
                    break;
            }

            // Encoding
            if (EditorSettingsService.EditorDefaultEncoding.CodePage == Encoding.UTF8.CodePage)
            {
                if (Equals(EditorSettingsService.EditorDefaultEncoding, new UTF8Encoding(false)))
                {
                    Utf8EncodingRadioButton.IsChecked = true;
                }
                else
                {
                    Utf8BomEncodingRadioButton.IsChecked = true;
                }
            }
            else if (EditorSettingsService.EditorDefaultEncoding.CodePage == Encoding.Unicode.CodePage)
            {
                Utf16LeBomEncodingRadioButton.IsChecked = true;
            }
            else if (EditorSettingsService.EditorDefaultEncoding.CodePage == Encoding.BigEndianUnicode.CodePage)
            {
                Utf16BeBomEncodingRadioButton.IsChecked = true;
            }

            // Decoding
            if (EditorSettingsService.EditorDefaultDecoding == null)
            {
                AutoGuessDecodingRadioButton.IsChecked = true;
            }
            else if (EditorSettingsService.EditorDefaultDecoding.CodePage == Encoding.UTF8.CodePage)
            {
                Utf8DecodingRadioButton.IsChecked = true;
            }
            else
            {
                AnsiDecodingRadioButton.IsChecked = true;
            }

            // Tab indentation
            if (EditorSettingsService.EditorDefaultTabIndents == -1)
            {
                TabDefaultRadioButton.IsChecked = true;
            }
            else if (EditorSettingsService.EditorDefaultTabIndents == 2)
            {
                TabTwoSpacesRadioButton.IsChecked = true;
            }
            else if (EditorSettingsService.EditorDefaultTabIndents == 4)
            {
                TabFourSpacesRadioButton.IsChecked = true;
            }
            else if (EditorSettingsService.EditorDefaultTabIndents == 8)
            {
                TabEightSpacesRadioButton.IsChecked = true;
            }

            // Search Engine
            switch (EditorSettingsService.EditorDefaultSearchEngine)
            {
                case SearchEngine.Bing:
                    BingRadioButton.IsChecked = true;
                    CustomSearchUrl.IsEnabled = false;
                    break;
                case SearchEngine.Google:
                    GoogleRadioButton.IsChecked = true;
                    CustomSearchUrl.IsEnabled = false;
                    break;
                case SearchEngine.DuckDuckGo:
                    DuckDuckGoRadioButton.IsChecked = true;
                    CustomSearchUrl.IsEnabled = false;
                    break;
                case SearchEngine.Custom:
                    CustomSearchUrlRadioButton.IsChecked = true;
                    CustomSearchUrl.IsEnabled = true;
                    break;
            }
            if (!string.IsNullOrEmpty(EditorSettingsService.EditorCustomMadeSearchUrl))
            {
                CustomSearchUrl.Text = EditorSettingsService.EditorCustomMadeSearchUrl;
            }

            Loaded += TextAndEditorSettings_Loaded;
        }

        private void TextAndEditorSettings_Loaded(object sender, RoutedEventArgs e)
        {
            TextWrappingToggle.Toggled += TextWrappingToggle_OnToggled;
            HighlightMisspelledWordsToggle.Toggled += HighlightMisspelledWordsToggle_OnToggled;
            LineHighlighterToggle.Toggled += LineHighlighterToggle_OnToggled;
            FontFamilyPicker.SelectionChanged += FontFamilyPicker_OnSelectionChanged;
            FontSizePicker.SelectionChanged += FontSizePicker_OnSelectionChanged;

            CrlfRadioButton.Checked += LineEndingRadioButton_OnChecked;
            CrRadioButton.Checked += LineEndingRadioButton_OnChecked;
            LfRadioButton.Checked += LineEndingRadioButton_OnChecked;

            Utf8EncodingRadioButton.Checked += EncodingRadioButton_Checked;
            Utf8BomEncodingRadioButton.Checked += EncodingRadioButton_Checked;
            Utf16LeBomEncodingRadioButton.Checked += EncodingRadioButton_Checked;
            Utf16BeBomEncodingRadioButton.Checked += EncodingRadioButton_Checked;

            Utf8DecodingRadioButton.Checked += DecodingRadioButton_Checked;
            AnsiDecodingRadioButton.Checked += DecodingRadioButton_Checked;
            AutoGuessDecodingRadioButton.Checked += DecodingRadioButton_Checked;

            TabDefaultRadioButton.Checked += TabBehaviorRadioButton_Checked;
            TabTwoSpacesRadioButton.Checked += TabBehaviorRadioButton_Checked;
            TabFourSpacesRadioButton.Checked += TabBehaviorRadioButton_Checked;
            TabEightSpacesRadioButton.Checked += TabBehaviorRadioButton_Checked;

            BingRadioButton.Checked += SearchEngineRadioButton_Checked;
            GoogleRadioButton.Checked += SearchEngineRadioButton_Checked;
            DuckDuckGoRadioButton.Checked += SearchEngineRadioButton_Checked;
            CustomSearchUrlRadioButton.Checked += SearchEngineRadioButton_Checked;
        }

        private void SearchEngineRadioButton_Checked(object sender, RoutedEventArgs e)
        {
            if (!(sender is RadioButton radioButton)) return;

            switch (radioButton.Name)
            {
                case "BingRadioButton":
                    EditorSettingsService.EditorDefaultSearchEngine = SearchEngine.Bing;
                    CustomSearchUrl.IsEnabled = false;
                    CustomUrlErrorReport.Visibility = Visibility.Collapsed;
                    break;
                case "GoogleRadioButton":
                    EditorSettingsService.EditorDefaultSearchEngine = SearchEngine.Google;
                    CustomSearchUrl.IsEnabled = false;
                    CustomUrlErrorReport.Visibility = Visibility.Collapsed;
                    break;
                case "DuckDuckGoRadioButton":
                    EditorSettingsService.EditorDefaultSearchEngine = SearchEngine.DuckDuckGo;
                    CustomSearchUrl.IsEnabled = false;
                    CustomUrlErrorReport.Visibility = Visibility.Collapsed;
                    break;
                case "CustomSearchUrlRadioButton":
                    CustomSearchUrl.IsEnabled = true;
                    CustomSearchUrl.Focus(FocusState.Programmatic);
                    CustomSearchUrl.Select(CustomSearchUrl.Text.Length, 0);
                    CustomUrlErrorReport.Visibility = IsValidUrl(CustomSearchUrl.Text) ? Visibility.Collapsed : Visibility.Visible;
                    EditorSettingsService.EditorCustomMadeSearchUrl = CustomSearchUrl.Text;
                    break;
            }
        }

        private void TabBehaviorRadioButton_Checked(object sender, RoutedEventArgs e)
        {
            if (!(sender is RadioButton radioButton)) return;

            switch (radioButton.Tag)
            {
                case "-1":
                    EditorSettingsService.EditorDefaultTabIndents = -1;
                    break;
                case "2":
                    EditorSettingsService.EditorDefaultTabIndents = 2;
                    break;
                case "4":
                    EditorSettingsService.EditorDefaultTabIndents = 4;
                    break;
                case "8":
                    EditorSettingsService.EditorDefaultTabIndents = 8;
                    break;
            }
        }

        private void EncodingRadioButton_Checked(object sender, RoutedEventArgs e)
        {
            if (!(sender is RadioButton radioButton)) return;

            switch (radioButton.Tag)
            {
                case "UTF-8":
                    EditorSettingsService.EditorDefaultEncoding = new UTF8Encoding(false);
                    break;
                case "UTF-8-BOM":
                    EditorSettingsService.EditorDefaultEncoding = new UTF8Encoding(true);
                    break;
                case "UTF-16 LE BOM":
                    EditorSettingsService.EditorDefaultEncoding = new UnicodeEncoding(false, true);
                    break;
                case "UTF-16 BE BOM":
                    EditorSettingsService.EditorDefaultEncoding = new UnicodeEncoding(true, true);
                    break;
            }
        }

        private void DecodingRadioButton_Checked(object sender, RoutedEventArgs e)
        {
            if (!(sender is RadioButton radioButton)) return;

            switch (radioButton.Tag)
            {
                case "Auto":
                    EditorSettingsService.EditorDefaultDecoding = null;
                    break;
                case "UTF-8":
                    EditorSettingsService.EditorDefaultDecoding = new UTF8Encoding(false);
                    break;
                case "ANSI":
                    if (EncodingUtility.TryGetSystemDefaultANSIEncoding(out var systemDefaultEncoding))
                    {
                        EditorSettingsService.EditorDefaultDecoding = systemDefaultEncoding;
                    }
                    else if (EncodingUtility.TryGetCurrentCultureANSIEncoding(out var currentCultureEncoding))
                    {
                        EditorSettingsService.EditorDefaultDecoding = currentCultureEncoding;
                    }
                    else
                    {
                        AutoGuessDecodingRadioButton.IsChecked = true;
                    }
                    break;
            }
        }

        private void LineEndingRadioButton_OnChecked(object sender, RoutedEventArgs e)
        {
            if (!(sender is RadioButton radioButton)) return;

            switch (radioButton.Tag)
            {
                case "Crlf":
                    EditorSettingsService.EditorDefaultLineEnding = LineEnding.Crlf;
                    break;
                case "Cr":
                    EditorSettingsService.EditorDefaultLineEnding = LineEnding.Cr;
                    break;
                case "Lf":
                    EditorSettingsService.EditorDefaultLineEnding = LineEnding.Lf;
                    break;
            }
        }

        private void FontFamilyPicker_OnSelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            EditorSettingsService.EditorFontFamily = (string)e.AddedItems.First();
        }

        private void FontSizePicker_OnSelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            EditorSettingsService.EditorFontSize = (int)e.AddedItems.First();
        }

        private void TextWrappingToggle_OnToggled(object sender, RoutedEventArgs e)
        {
            EditorSettingsService.EditorDefaultTextWrapping = TextWrappingToggle.IsOn ? TextWrapping.Wrap : TextWrapping.NoWrap;
        }

        private void HighlightMisspelledWordsToggle_OnToggled(object sender, RoutedEventArgs e)
        {
            EditorSettingsService.IsHighlightMisspelledWordsEnabled = HighlightMisspelledWordsToggle.IsOn;
        }

        private void LineHighlighterToggle_OnToggled(object sender, RoutedEventArgs e)
        {
            EditorSettingsService.IsLineHighlighterEnabled = LineHighlighterToggle.IsOn;
        }

        private void CustomSearchUrl_TextChanged(object sender, TextChangedEventArgs e)
        {
            EditorSettingsService.EditorCustomMadeSearchUrl = CustomSearchUrl.Text;
            CustomUrlErrorReport.Visibility = IsValidUrl(CustomSearchUrl.Text) ? Visibility.Collapsed : Visibility.Visible;
        }

        private void CustomSearchUrl_LostFocus(object sender, RoutedEventArgs e)
        {
            if (CustomSearchUrlRadioButton.IsChecked != null &&
                (IsValidUrl(CustomSearchUrl.Text) && (bool) CustomSearchUrlRadioButton.IsChecked))
            {
                EditorSettingsService.EditorDefaultSearchEngine = SearchEngine.Custom;
            }
            else if (!IsValidUrl(CustomSearchUrl.Text) && EditorSettingsService.EditorDefaultSearchEngine == SearchEngine.Custom)
            {
                EditorSettingsService.EditorDefaultSearchEngine = SearchEngine.Bing;
            }

            CustomUrlErrorReport.Visibility = IsValidUrl(CustomSearchUrl.Text) ? Visibility.Collapsed : Visibility.Visible;
        }

        private bool IsValidUrl(string url)
        {
            try
            {
                if (Uri.TryCreate(url, UriKind.Absolute, out var uriResult) && (uriResult.Scheme == Uri.UriSchemeHttp || uriResult.Scheme == Uri.UriSchemeHttps))
                {
                    if (string.Format(url, "s") == url)
                        return false;
                }
                else
                {
                    return false;
                }
            }
            catch (Exception)
            {
                return false;
            }
            return true;
        }
    }
}
