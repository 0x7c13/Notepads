// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;

namespace Notepads.Presentation.Controls.Print;

public sealed partial class ContinuationPageFormat : Page
{
    public ContinuationPageFormat(RichTextBlockOverflow textLinkContainer, FontFamily textEditorFontFamily, double textEditorFontSize, string headerText, string footerText)
    {
        InitializeComponent();

        if (!string.IsNullOrEmpty(headerText))
        {
            Header.Visibility = Visibility.Visible;
            Header.Margin = new Thickness(0, 0, 0, textEditorFontSize + 6);
            HeaderTextBlock.Text = headerText;
            HeaderTextBlock.FontFamily = textEditorFontFamily;
            HeaderTextBlock.FontSize = textEditorFontSize + 4;
        }
        else
        {
            Header.Visibility = Visibility.Collapsed;
            HeaderTextBlock.FontFamily = textEditorFontFamily;
            HeaderTextBlock.FontSize = textEditorFontSize + 4;
        }

        if (!string.IsNullOrEmpty(footerText))
        {
            Footer.Visibility = Visibility.Visible;
            FooterTextBlock.Text = footerText;
            FooterTextBlock.FontFamily = textEditorFontFamily;
            FooterTextBlock.FontSize = textEditorFontSize;
        }
        else
        {
            Footer.Visibility = Visibility.Collapsed;
            FooterTextBlock.FontFamily = textEditorFontFamily;
            FooterTextBlock.FontSize = textEditorFontSize;
        }

        textLinkContainer.OverflowContentTarget = ContinuationPageLinkedContainer;
    }
}
