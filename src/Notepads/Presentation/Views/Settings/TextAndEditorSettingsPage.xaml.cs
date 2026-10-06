// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Text;
using Notepads.Features.Preferences;
using Notepads.Features.WebSearch;
using Notepads.Presentation.Theming;
using Windows.ApplicationModel.Resources;
using Windows.UI.Text;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using Windows.UI.Xaml.Navigation;

namespace Notepads.Presentation.Views.Settings;

[WinRT.GeneratedBindableCustomProperty]
public sealed partial class FontStyleItem
{
    public FontStyle FontStyle { get; set; }

    public string FontStyleLocalizedName { get; set; }
}

[WinRT.GeneratedBindableCustomProperty]
public sealed partial class FontWeightItem
{
    public FontWeight FontWeight { get; set; }

    public string FontWeightLocalizedName { get; set; }
}

public sealed partial class TextAndEditorSettingsPage : Page
{
    private readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

    public IReadOnlyCollection<string> AvailableFonts => FontOptions.GetSystemFontFamilies();

    public int[] AvailableFontSizes = FontOptions.PredefinedFontSizes;

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
            foreach (var (fontStyleName, fontStyle) in FontOptions.PredefinedFontStylesMap)
            {
                _availableFontStyles.Add(new FontStyleItem()
                {
                    FontStyle = fontStyle.ToFontStyle(),
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
            foreach (var (fontWeightName, fontWeight) in FontOptions.PredefinedFontWeightsMap)
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

    private SettingsContext _context;
    private bool _eventsBound;

    public TextAndEditorSettingsPage()
    {
        InitializeComponent();
    }

    internal void Initialize(SettingsContext context)
    {
        if (_context != null)
        {
            if (!ReferenceEquals(_context, context)) throw new InvalidOperationException("A settings page cannot change its window context.");
            return;
        }
        _context = context ?? throw new ArgumentNullException(nameof(context));

        TextWrappingToggle.IsOn = (ApplicationPreferences.EditorDefaultWordWrap.ToTextWrapping() == TextWrapping.Wrap);
        HighlightMisspelledWordsToggle.IsOn = ApplicationPreferences.IsHighlightMisspelledWordsEnabled;
        LineHighlighterToggle.IsOn = ApplicationPreferences.EditorDisplayLineHighlighter;
        LineNumbersToggle.IsOn = ApplicationPreferences.EditorDisplayLineNumbers;
        FontFamilyPicker.SelectedItem = ApplicationPreferences.EditorFontFamily;
        FontSizePicker.SelectedItem = ApplicationPreferences.EditorFontSize;
        FontStylePicker.SelectedItem = AvailableFontStyles.FirstOrDefault(style => style.FontStyle == ApplicationPreferences.EditorFontStyle.ToFontStyle());
        FontWeightPicker.SelectedItem = AvailableFontWeights.FirstOrDefault(weight => weight.FontWeight.Weight == ApplicationPreferences.EditorFontWeight);

        InitializeLineEndingSettings();

        InitializeEncodingSettings();

        InitializeDecodingSettings();

        InitializeTabIndentationSettings();

        InitializeSearchEngineSettings();

        Loaded += TextAndEditorSettings_Loaded;
        Unloaded += TextAndEditorSettings_Unloaded;
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        Initialize(e.Parameter as SettingsContext ?? throw new ArgumentException("Settings navigation requires a context."));
    }

    private void InitializeLineEndingSettings()
    {
        switch (ApplicationPreferences.EditorDefaultLineEnding)
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
        if (ApplicationPreferences.EditorDefaultEncoding.CodePage == Encoding.UTF8.CodePage)
        {
            if (Equals(ApplicationPreferences.EditorDefaultEncoding, new UTF8Encoding(false)))
            {
                Utf8EncodingRadioButton.IsChecked = true;
            }
            else
            {
                Utf8BomEncodingRadioButton.IsChecked = true;
            }
        }
        else if (ApplicationPreferences.EditorDefaultEncoding.CodePage == Encoding.Unicode.CodePage)
        {
            Utf16LeBomEncodingRadioButton.IsChecked = true;
        }
        else if (ApplicationPreferences.EditorDefaultEncoding.CodePage == Encoding.BigEndianUnicode.CodePage)
        {
            Utf16BeBomEncodingRadioButton.IsChecked = true;
        }
    }

    private void InitializeDecodingSettings()
    {
        if (ApplicationPreferences.EditorDefaultDecoding == null)
        {
            AutoGuessDecodingRadioButton.IsChecked = true;
        }
        else if (ApplicationPreferences.EditorDefaultDecoding.CodePage == Encoding.UTF8.CodePage)
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
        if (ApplicationPreferences.EditorDefaultTabIndents == -1)
        {
            TabDefaultRadioButton.IsChecked = true;
        }
        else if (ApplicationPreferences.EditorDefaultTabIndents == 2)
        {
            TabTwoSpacesRadioButton.IsChecked = true;
        }
        else if (ApplicationPreferences.EditorDefaultTabIndents == 4)
        {
            TabFourSpacesRadioButton.IsChecked = true;
        }
        else if (ApplicationPreferences.EditorDefaultTabIndents == 8)
        {
            TabEightSpacesRadioButton.IsChecked = true;
        }
    }

    private void InitializeSearchEngineSettings()
    {
        switch (ApplicationPreferences.EditorDefaultSearchEngine)
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

        if (!string.IsNullOrEmpty(ApplicationPreferences.EditorCustomMadeSearchUrl))
        {
            CustomSearchUrl.Text = ApplicationPreferences.EditorCustomMadeSearchUrl;
        }
    }

    private void TextAndEditorSettings_Loaded(object sender, RoutedEventArgs e)
    {
        if (_eventsBound) return;
        _eventsBound = true;
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

    private void TextAndEditorSettings_Unloaded(object sender, RoutedEventArgs e)
    {
        if (!_eventsBound) return;
        _eventsBound = false;
        TextWrappingToggle.Toggled -= TextWrappingToggle_OnToggled;
        HighlightMisspelledWordsToggle.Toggled -= HighlightMisspelledWordsToggle_OnToggled;
        LineHighlighterToggle.Toggled -= LineHighlighterToggle_OnToggled;
        LineNumbersToggle.Toggled -= LineNumbersToggle_Toggled;
        FontFamilyPicker.SelectionChanged -= FontFamilyPicker_OnSelectionChanged;
        FontSizePicker.SelectionChanged -= FontSizePicker_OnSelectionChanged;
        FontStylePicker.SelectionChanged -= FontStylePicker_OnSelectionChanged;
        FontWeightPicker.SelectionChanged -= FontWeightPicker_OnSelectionChanged;

        CrlfRadioButton.Checked -= LineEndingRadioButton_OnChecked;
        CrRadioButton.Checked -= LineEndingRadioButton_OnChecked;
        LfRadioButton.Checked -= LineEndingRadioButton_OnChecked;

        Utf8EncodingRadioButton.Checked -= EncodingRadioButton_Checked;
        Utf8BomEncodingRadioButton.Checked -= EncodingRadioButton_Checked;
        Utf16LeBomEncodingRadioButton.Checked -= EncodingRadioButton_Checked;
        Utf16BeBomEncodingRadioButton.Checked -= EncodingRadioButton_Checked;

        Utf8DecodingRadioButton.Checked -= DecodingRadioButton_Checked;
        AnsiDecodingRadioButton.Checked -= DecodingRadioButton_Checked;
        AutoGuessDecodingRadioButton.Checked -= DecodingRadioButton_Checked;

        TabDefaultRadioButton.Checked -= TabBehaviorRadioButton_Checked;
        TabTwoSpacesRadioButton.Checked -= TabBehaviorRadioButton_Checked;
        TabFourSpacesRadioButton.Checked -= TabBehaviorRadioButton_Checked;
        TabEightSpacesRadioButton.Checked -= TabBehaviorRadioButton_Checked;

        BingRadioButton.Checked -= SearchEngineRadioButton_Checked;
        GoogleRadioButton.Checked -= SearchEngineRadioButton_Checked;
        DuckDuckGoRadioButton.Checked -= SearchEngineRadioButton_Checked;
        CustomSearchUrlRadioButton.Checked -= SearchEngineRadioButton_Checked;
    }

    private void SearchEngineRadioButton_Checked(object sender, RoutedEventArgs e)
    {
        if (sender is not RadioButton radioButton) return;

        switch (radioButton.Name)
        {
            case "BingRadioButton":
                ApplicationPreferences.EditorDefaultSearchEngine = SearchEngine.Bing;
                OnCustomSearchEngineSelectionChanged(false);
                break;
            case "GoogleRadioButton":
                ApplicationPreferences.EditorDefaultSearchEngine = SearchEngine.Google;
                OnCustomSearchEngineSelectionChanged(false);
                break;
            case "DuckDuckGoRadioButton":
                ApplicationPreferences.EditorDefaultSearchEngine = SearchEngine.DuckDuckGo;
                OnCustomSearchEngineSelectionChanged(false);
                break;
            case "CustomSearchUrlRadioButton":
                OnCustomSearchEngineSelectionChanged(true);
                break;
        }
    }

    private void TabBehaviorRadioButton_Checked(object sender, RoutedEventArgs e)
    {
        if (sender is not RadioButton radioButton) return;

        switch (radioButton.Tag)
        {
            case "-1":
                ApplicationPreferences.EditorDefaultTabIndents = -1;
                break;
            case "2":
                ApplicationPreferences.EditorDefaultTabIndents = 2;
                break;
            case "4":
                ApplicationPreferences.EditorDefaultTabIndents = 4;
                break;
            case "8":
                ApplicationPreferences.EditorDefaultTabIndents = 8;
                break;
        }
    }

    private void EncodingRadioButton_Checked(object sender, RoutedEventArgs e)
    {
        if (sender is not RadioButton radioButton) return;

        switch (radioButton.Tag)
        {
            case "UTF-8":
                ApplicationPreferences.EditorDefaultEncoding = new UTF8Encoding(false);
                break;
            case "UTF-8-BOM":
                ApplicationPreferences.EditorDefaultEncoding = new UTF8Encoding(true);
                break;
            case "UTF-16 LE BOM":
                ApplicationPreferences.EditorDefaultEncoding = new UnicodeEncoding(false, true);
                break;
            case "UTF-16 BE BOM":
                ApplicationPreferences.EditorDefaultEncoding = new UnicodeEncoding(true, true);
                break;
        }
    }

    private void DecodingRadioButton_Checked(object sender, RoutedEventArgs e)
    {
        if (sender is not RadioButton radioButton) return;

        switch (radioButton.Tag)
        {
            case "Auto":
                ApplicationPreferences.EditorDefaultDecoding = null;
                break;
            case "UTF-8":
                ApplicationPreferences.EditorDefaultDecoding = new UTF8Encoding(false);
                break;
            case "ANSI":
                if (EncodingCatalog.TryGetSystemDefaultANSIEncoding(out var systemDefaultEncoding))
                {
                    ApplicationPreferences.EditorDefaultDecoding = systemDefaultEncoding;
                }
                else if (EncodingCatalog.TryGetCurrentCultureANSIEncoding(out var currentCultureEncoding))
                {
                    ApplicationPreferences.EditorDefaultDecoding = currentCultureEncoding;
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
        if (sender is not RadioButton radioButton) return;

        switch (radioButton.Tag)
        {
            case "Crlf":
                ApplicationPreferences.EditorDefaultLineEnding = LineEnding.Crlf;
                break;
            case "Cr":
                ApplicationPreferences.EditorDefaultLineEnding = LineEnding.Cr;
                break;
            case "Lf":
                ApplicationPreferences.EditorDefaultLineEnding = LineEnding.Lf;
                break;
        }
    }

    private void FontFamilyPicker_OnSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        var fontFamily = new FontFamily((string)e.AddedItems.First());
        ApplicationPreferences.EditorFontFamily = fontFamily.Source;
        FontStylePicker.FontFamily = fontFamily;
        FontWeightPicker.FontFamily = fontFamily;
    }

    private void FontSizePicker_OnSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        ApplicationPreferences.EditorFontSize = (int)e.AddedItems.First();
    }

    private void FontStylePicker_OnSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        ApplicationPreferences.EditorFontStyle = ((FontStyleItem)e.AddedItems.First()).FontStyle.ToFontSlant();
    }

    private void FontWeightPicker_OnSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        ApplicationPreferences.EditorFontWeight = ((FontWeightItem)e.AddedItems.First()).FontWeight.Weight;
    }

    private void TextWrappingToggle_OnToggled(object sender, RoutedEventArgs e)
    {
        ApplicationPreferences.EditorDefaultWordWrap = TextWrappingToggle.IsOn;
    }

    private void HighlightMisspelledWordsToggle_OnToggled(object sender, RoutedEventArgs e)
    {
        ApplicationPreferences.IsHighlightMisspelledWordsEnabled = HighlightMisspelledWordsToggle.IsOn;
    }

    private void LineHighlighterToggle_OnToggled(object sender, RoutedEventArgs e)
    {
        ApplicationPreferences.EditorDisplayLineHighlighter = LineHighlighterToggle.IsOn;
    }

    private void LineNumbersToggle_Toggled(object sender, RoutedEventArgs e)
    {
        ApplicationPreferences.EditorDisplayLineNumbers = LineNumbersToggle.IsOn;
    }

    private void CustomSearchUrl_TextChanged(object sender, TextChangedEventArgs e)
    {
        ApplicationPreferences.EditorCustomMadeSearchUrl = CustomSearchUrl.Text;
        CustomUrlErrorReport.Visibility = IsValidUrl(CustomSearchUrl.Text) ? Visibility.Collapsed : Visibility.Visible;
    }

    private void CustomSearchUrl_LostFocus(object sender, RoutedEventArgs e)
    {
        if (CustomSearchUrlRadioButton.IsChecked != null &&
            (IsValidUrl(CustomSearchUrl.Text) && (bool)CustomSearchUrlRadioButton.IsChecked))
        {
            ApplicationPreferences.EditorDefaultSearchEngine = SearchEngine.Custom;
        }
        else if (!IsValidUrl(CustomSearchUrl.Text) && ApplicationPreferences.EditorDefaultSearchEngine == SearchEngine.Custom)
        {
            ApplicationPreferences.EditorDefaultSearchEngine = SearchEngine.Bing;
        }

        CustomUrlErrorReport.Visibility = IsValidUrl(CustomSearchUrl.Text) ? Visibility.Collapsed : Visibility.Visible;
        ApplicationPreferences.EditorCustomMadeSearchUrl = CustomSearchUrl.Text;
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

    private void OnCustomSearchEngineSelectionChanged(bool selected)
    {
        if (selected)
        {
            CustomSearchUrl.IsEnabled = true;
            CustomSearchUrl.Focus(FocusState.Programmatic);
            CustomSearchUrl.Select(CustomSearchUrl.Text.Length, 0);
            if (IsValidUrl(CustomSearchUrl.Text))
            {
                ApplicationPreferences.EditorDefaultSearchEngine = SearchEngine.Custom;
                ApplicationPreferences.EditorCustomMadeSearchUrl = CustomSearchUrl.Text;
            }
            CustomSearchUrl_TextChanged(null, null);
        }
        else
        {
            CustomSearchUrl.IsEnabled = false;
            CustomSearchUrl.Text = ApplicationPreferences.EditorCustomMadeSearchUrl;
            CustomUrlErrorReport.Visibility = Visibility.Collapsed;
        }
    }
}
