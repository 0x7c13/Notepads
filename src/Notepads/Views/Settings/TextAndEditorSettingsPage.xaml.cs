namespace Notepads.Views.Settings
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using System.Text;
    using Services;
    using Utilities;
    using Windows.ApplicationModel.Resources;
    using Windows.UI.Text;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;

    public class FontStyleItem
    {
        public FontStyle FontStyle { get; set; }

        public string FontStyleLocalizedName { get; set; }
    }

    public class FontWeightItem
    {
        public FontWeight FontWeight { get; set; }

        public string FontWeightLocalizedName { get; set; }
    }

    public sealed partial class TextAndEditorSettingsPage : Page
    {
        private readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

        public IReadOnlyCollection<string> AvailableFonts => FontUtility.GetSystemFontFamilies();

        public int[] AvailableFontSizes = FontUtility.PredefinedFontSizes;

        private IList<FontStyleItem> _availableFontStyles;

        public IList<FontStyleItem> AvailableFontStyles
        {
            get
            {
                if (_availableFontStyles != null)
                {
                    return _availableFontStyles;
                }

                _availableFontStyles = new List<FontStyleItem>();
                foreach (var (fontStyleName, fontStyle) in FontUtility.PredefinedFontStylesMap)
                {
                    _availableFontStyles.Add(new FontStyleItem()
                    {
                        FontStyle = fontStyle,
                        FontStyleLocalizedName = _resourceLoader.GetString($"FontStyle_{fontStyleName}")
                    });
                }

                return _availableFontStyles;
            }
        }

        private IList<FontWeightItem> _availableFontWeights;

        public IList<FontWeightItem> AvailableFontWeights
        {
            get
            {
                if (_availableFontWeights != null)
                {
                    return _availableFontWeights;
                }

                _availableFontWeights = new List<FontWeightItem>();
                foreach (var (fontWeightName, fontWeight) in FontUtility.PredefinedFontWeightsMap)
                {
                    _availableFontWeights.Add(new FontWeightItem()
                    {
                        FontWeight = new FontWeight() { Weight = fontWeight },
                        FontWeightLocalizedName = _resourceLoader.GetString($"FontWeight_{fontWeightName}")
                    });
                }

                return _availableFontWeights;
            }
        }

        public TextAndEditorSettingsPage()
        {
            InitializeComponent();

            TextWrappingToggle.IsOn = (AppSettingsService.EditorDefaultTextWrapping == TextWrapping.Wrap);
            HighlightMisspelledWordsToggle.IsOn = AppSettingsService.IsHighlightMisspelledWordsEnabled;
            LineHighlighterToggle.IsOn = AppSettingsService.EditorDisplayLineHighlighter;
            LineNumbersToggle.IsOn = AppSettingsService.EditorDisplayLineNumbers;
            FontFamilyPicker.SelectedItem = AppSettingsService.EditorFontFamily;
            FontSizePicker.SelectedItem = AppSettingsService.EditorFontSize;
            FontStylePicker.SelectedItem = AvailableFontStyles.FirstOrDefault(style => style.FontStyle == AppSettingsService.EditorFontStyle);
            FontWeightPicker.SelectedItem = AvailableFontWeights.FirstOrDefault(weight => weight.FontWeight.Weight == AppSettingsService.EditorFontWeight.Weight);

            InitializeLineEndingSettings();

            InitializeEncodingSettings();

            InitializeDecodingSettings();

            InitializeTabIndentationSettings();

            InitializeSearchEngineSettings();

            Loaded += TextAndEditorSettings_Loaded;
        }

        private void InitializeLineEndingSettings()
        {
            switch (AppSettingsService.EditorDefaultLineEnding)
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
        }

        private void InitializeEncodingSettings()
        {
            if (AppSettingsService.EditorDefaultEncoding.CodePage == Encoding.UTF8.CodePage)
            {
                if (Equals(AppSettingsService.EditorDefaultEncoding, new UTF8Encoding(false)))
                {
                    Utf8EncodingRadioButton.IsChecked = true;
                }
                else
                {
                    Utf8BomEncodingRadioButton.IsChecked = true;
                }
            }
            else if (AppSettingsService.EditorDefaultEncoding.CodePage == Encoding.Unicode.CodePage)
            {
                Utf16LeBomEncodingRadioButton.IsChecked = true;
            }
            else if (AppSettingsService.EditorDefaultEncoding.CodePage == Encoding.BigEndianUnicode.CodePage)
            {
                Utf16BeBomEncodingRadioButton.IsChecked = true;
            }
        }

        private void InitializeDecodingSettings()
        {
            if (AppSettingsService.EditorDefaultDecoding == null)
            {
                AutoGuessDecodingRadioButton.IsChecked = true;
            }
            else if (AppSettingsService.EditorDefaultDecoding.CodePage == Encoding.UTF8.CodePage)
            {
                Utf8DecodingRadioButton.IsChecked = true;
            }
            else
            {
                AnsiDecodingRadioButton.IsChecked = true;
            }
        }

        private void InitializeTabIndentationSettings()
        {
            if (AppSettingsService.EditorDefaultTabIndents == -1)
            {
                TabDefaultRadioButton.IsChecked = true;
            }
            else if (AppSettingsService.EditorDefaultTabIndents == 2)
            {
                TabTwoSpacesRadioButton.IsChecked = true;
            }
            else if (AppSettingsService.EditorDefaultTabIndents == 4)
            {
                TabFourSpacesRadioButton.IsChecked = true;
            }
            else if (AppSettingsService.EditorDefaultTabIndents == 8)
            {
                TabEightSpacesRadioButton.IsChecked = true;
            }
        }

        private void InitializeSearchEngineSettings()
        {
            switch (AppSettingsService.EditorDefaultSearchEngine)
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

            if (!string.IsNullOrEmpty(AppSettingsService.EditorCustomMadeSearchUrl))
            {
                CustomSearchUrl.Text = AppSettingsService.EditorCustomMadeSearchUrl;
            }
        }

        private void TextAndEditorSettings_Loaded(object sender, RoutedEventArgs e)
        {
            TextWrappingToggle.Toggled += TextWrappingToggle_OnToggled;
            HighlightMisspelledWordsToggle.Toggled += HighlightMisspelledWordsToggle_OnToggled;
            LineHighlighterToggle.Toggled += LineHighlighterToggle_OnToggled;
            LineNumbersToggle.Toggled += LineNumbersToggle_Toggled;
            FontFamilyPicker.SelectionChanged += FontFamilyPicker_OnSelectionChanged;
            FontSizePicker.SelectionChanged += FontSizePicker_OnSelectionChanged;
            FontStylePicker.SelectionChanged += FontStylePicker_OnSelectionChanged;
            FontWeightPicker.SelectionChanged += FontWeightPicker_OnSelectionChanged;

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
                    AppSettingsService.EditorDefaultSearchEngine = SearchEngine.Bing;
                    CustomSearchUrl.IsEnabled = false;
                    CustomUrlErrorReport.Visibility = Visibility.Collapsed;
                    break;
                case "GoogleRadioButton":
                    AppSettingsService.EditorDefaultSearchEngine = SearchEngine.Google;
                    CustomSearchUrl.IsEnabled = false;
                    CustomUrlErrorReport.Visibility = Visibility.Collapsed;
                    break;
                case "DuckDuckGoRadioButton":
                    AppSettingsService.EditorDefaultSearchEngine = SearchEngine.DuckDuckGo;
                    CustomSearchUrl.IsEnabled = false;
                    CustomUrlErrorReport.Visibility = Visibility.Collapsed;
                    break;
                case "CustomSearchUrlRadioButton":
                    CustomSearchUrl.IsEnabled = true;
                    CustomSearchUrl.Focus(FocusState.Programmatic);
                    CustomSearchUrl.Select(CustomSearchUrl.Text.Length, 0);
                    CustomUrlErrorReport.Visibility = IsValidUrl(CustomSearchUrl.Text) ? Visibility.Collapsed : Visibility.Visible;
                    AppSettingsService.EditorCustomMadeSearchUrl = CustomSearchUrl.Text;
                    break;
            }
        }

        private void TabBehaviorRadioButton_Checked(object sender, RoutedEventArgs e)
        {
            if (!(sender is RadioButton radioButton)) return;

            switch (radioButton.Tag)
            {
                case "-1":
                    AppSettingsService.EditorDefaultTabIndents = -1;
                    break;
                case "2":
                    AppSettingsService.EditorDefaultTabIndents = 2;
                    break;
                case "4":
                    AppSettingsService.EditorDefaultTabIndents = 4;
                    break;
                case "8":
                    AppSettingsService.EditorDefaultTabIndents = 8;
                    break;
            }
        }

        private void EncodingRadioButton_Checked(object sender, RoutedEventArgs e)
        {
            if (!(sender is RadioButton radioButton)) return;

            switch (radioButton.Tag)
            {
                case "UTF-8":
                    AppSettingsService.EditorDefaultEncoding = new UTF8Encoding(false);
                    break;
                case "UTF-8-BOM":
                    AppSettingsService.EditorDefaultEncoding = new UTF8Encoding(true);
                    break;
                case "UTF-16 LE BOM":
                    AppSettingsService.EditorDefaultEncoding = new UnicodeEncoding(false, true);
                    break;
                case "UTF-16 BE BOM":
                    AppSettingsService.EditorDefaultEncoding = new UnicodeEncoding(true, true);
                    break;
            }
        }

        private void DecodingRadioButton_Checked(object sender, RoutedEventArgs e)
        {
            if (!(sender is RadioButton radioButton)) return;

            switch (radioButton.Tag)
            {
                case "Auto":
                    AppSettingsService.EditorDefaultDecoding = null;
                    break;
                case "UTF-8":
                    AppSettingsService.EditorDefaultDecoding = new UTF8Encoding(false);
                    break;
                case "ANSI":
                    if (EncodingUtility.TryGetSystemDefaultANSIEncoding(out var systemDefaultEncoding))
                    {
                        AppSettingsService.EditorDefaultDecoding = systemDefaultEncoding;
                    }
                    else if (EncodingUtility.TryGetCurrentCultureANSIEncoding(out var currentCultureEncoding))
                    {
                        AppSettingsService.EditorDefaultDecoding = currentCultureEncoding;
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
                    AppSettingsService.EditorDefaultLineEnding = LineEnding.Crlf;
                    break;
                case "Cr":
                    AppSettingsService.EditorDefaultLineEnding = LineEnding.Cr;
                    break;
                case "Lf":
                    AppSettingsService.EditorDefaultLineEnding = LineEnding.Lf;
                    break;
            }
        }

        private void FontFamilyPicker_OnSelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            AppSettingsService.EditorFontFamily = (string)e.AddedItems.First();
        }

        private void FontSizePicker_OnSelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            AppSettingsService.EditorFontSize = (int)e.AddedItems.First();
        }

        private void FontStylePicker_OnSelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            AppSettingsService.EditorFontStyle = ((FontStyleItem)e.AddedItems.First()).FontStyle;
        }

        private void FontWeightPicker_OnSelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            AppSettingsService.EditorFontWeight = ((FontWeightItem)e.AddedItems.First()).FontWeight;
        }

        private void TextWrappingToggle_OnToggled(object sender, RoutedEventArgs e)
        {
            AppSettingsService.EditorDefaultTextWrapping = TextWrappingToggle.IsOn ? TextWrapping.Wrap : TextWrapping.NoWrap;
        }

        private void HighlightMisspelledWordsToggle_OnToggled(object sender, RoutedEventArgs e)
        {
            AppSettingsService.IsHighlightMisspelledWordsEnabled = HighlightMisspelledWordsToggle.IsOn;
        }

        private void LineHighlighterToggle_OnToggled(object sender, RoutedEventArgs e)
        {
            AppSettingsService.EditorDisplayLineHighlighter = LineHighlighterToggle.IsOn;
        }

        private void LineNumbersToggle_Toggled(object sender, RoutedEventArgs e)
        {
            AppSettingsService.EditorDisplayLineNumbers = LineNumbersToggle.IsOn;
        }

        private void CustomSearchUrl_TextChanged(object sender, TextChangedEventArgs e)
        {
            AppSettingsService.EditorCustomMadeSearchUrl = CustomSearchUrl.Text;
            CustomUrlErrorReport.Visibility = IsValidUrl(CustomSearchUrl.Text) ? Visibility.Collapsed : Visibility.Visible;
        }

        private void CustomSearchUrl_LostFocus(object sender, RoutedEventArgs e)
        {
            if (CustomSearchUrlRadioButton.IsChecked != null &&
                (IsValidUrl(CustomSearchUrl.Text) && (bool)CustomSearchUrlRadioButton.IsChecked))
            {
                AppSettingsService.EditorDefaultSearchEngine = SearchEngine.Custom;
            }
            else if (!IsValidUrl(CustomSearchUrl.Text) && AppSettingsService.EditorDefaultSearchEngine == SearchEngine.Custom)
            {
                AppSettingsService.EditorDefaultSearchEngine = SearchEngine.Bing;
            }

            CustomUrlErrorReport.Visibility = IsValidUrl(CustomSearchUrl.Text) ? Visibility.Collapsed : Visibility.Visible;
        }

        private static bool IsValidUrl(string url)
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