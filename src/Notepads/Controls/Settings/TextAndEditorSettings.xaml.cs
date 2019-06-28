
namespace Notepads.Controls.Settings
{
    using Notepads.Services;
    using Notepads.Utilities;
    using System;
    using System.Collections.Generic;
    using System.Collections.ObjectModel;
    using System.ComponentModel.Design;
    using System.IO;
    using System.Linq;
    using System.Runtime.InteropServices.WindowsRuntime;
    using System.Text;
    using Windows.Foundation;
    using Windows.Foundation.Collections;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Controls.Primitives;
    using Windows.UI.Xaml.Data;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;
    using Windows.UI.Xaml.Navigation;

    public sealed partial class TextAndEditorSettings : Page
    {
        public string[] SystemFonts = Microsoft.Graphics.Canvas.Text.CanvasTextFormat.GetSystemFontFamilies();

        public int[] FontSizes = new int[]
        {
            8, 9, 10, 11, 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72
        };

        public TextAndEditorSettings()
        {
            InitializeComponent();

            TextWrappingToggle.IsOn = (EditorSettingsService.EditorDefaultTextWrapping == TextWrapping.Wrap);
            FontFamilyPicker.SelectedItem = EditorSettingsService.EditorFontFamily;
            FontSizePicker.SelectedItem = EditorSettingsService.EditorFontSize;

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

            if (EditorSettingsService.EditorDefaultEncoding.CodePage == Encoding.UTF8.CodePage)
            {
                Utf8RadioButton.IsChecked = true;
            }
            else if (EditorSettingsService.EditorDefaultEncoding.CodePage == Encoding.Unicode.CodePage)
            {
                Utf16LeRadioButton.IsChecked = true;
            }
            else if (EditorSettingsService.EditorDefaultEncoding.CodePage == Encoding.BigEndianUnicode.CodePage)
            {
                Utf16BeRadioButton.IsChecked = true;
            }

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

            Loaded += TextAndEditorSettings_Loaded;
        }

        private void TextAndEditorSettings_Loaded(object sender, RoutedEventArgs e)
        {
            TextWrappingToggle.Toggled += TextWrappingToggle_OnToggled;
            FontFamilyPicker.SelectionChanged += FontFamilyPicker_OnSelectionChanged;
            FontSizePicker.SelectionChanged += FontSizePicker_OnSelectionChanged;

            CrlfRadioButton.Checked += LineEndingRadioButton_OnChecked;
            CrRadioButton.Checked += LineEndingRadioButton_OnChecked;
            LfRadioButton.Checked += LineEndingRadioButton_OnChecked;

            Utf8RadioButton.Checked += EncodingRadioButton_Checked;
            Utf16LeRadioButton.Checked += EncodingRadioButton_Checked;
            Utf16BeRadioButton.Checked += EncodingRadioButton_Checked;

            TabDefaultRadioButton.Checked += TabBehaviorRadioButton_Checked;
            TabTwoSpacesRadioButton.Checked += TabBehaviorRadioButton_Checked;
            TabFourSpacesRadioButton.Checked += TabBehaviorRadioButton_Checked;
            TabEightSpacesRadioButton.Checked += TabBehaviorRadioButton_Checked;
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
                    EditorSettingsService.EditorDefaultEncoding = Encoding.UTF8;
                    break;
                case "UTF-16 LE":
                    EditorSettingsService.EditorDefaultEncoding = Encoding.Unicode;
                    break;
                case "UTF-16 BE":
                    EditorSettingsService.EditorDefaultEncoding = Encoding.BigEndianUnicode;
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
    }
}
