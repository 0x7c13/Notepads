// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Documents;
using Windows.UI.Xaml.Media;

namespace Notepads.Presentation.Controls.Print;

public sealed partial class PrintPageFormat : Page
{
    public RichTextBlock TextContentBlock { get; set; }

    public PrintPageFormat(string textEditorText, FontFamily textEditorFontFamily, double textEditorFontSize,
        string headerText, string footerText, bool rightToLeftReadingOrder = false)
    {
        InitializeComponent();

        TextContent.FontFamily = textEditorFontFamily;
        TextContent.FontSize = textEditorFontSize;
        // Document direction is independent of the app's print-preview chrome.
        TextContent.FlowDirection = rightToLeftReadingOrder ? FlowDirection.RightToLeft : FlowDirection.LeftToRight;
        TextContent.TextReadingOrder = TextReadingOrder.UseFlowDirection;
        TextContent.TextAlignment = rightToLeftReadingOrder ? TextAlignment.Right : TextAlignment.Left;
        TextContent.HorizontalAlignment = HorizontalAlignment.Stretch;
        ContinuationPageLinkedContainer.FlowDirection = TextContent.FlowDirection;

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

        var run = new Run { Text = textEditorText };
        TextEditorContent.Inlines.Add(run);
    }
}
