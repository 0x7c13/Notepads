using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using Windows.Foundation;
using Windows.Foundation.Collections;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Data;
using Windows.UI.Xaml.Documents;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Media;
using Windows.UI.Xaml.Navigation;

// The Blank Page item template is documented at https://go.microsoft.com/fwlink/?LinkId=234238

namespace Notepads.Controls.Print
{
    /// <summary>
    /// An empty page that can be used on its own or navigated to within a Frame.
    /// </summary>
    public sealed partial class PrintPageFormat : Page
    {
        public RichTextBlock TextContentBlock { get; set; }

        public PrintPageFormat(string textEditorText, FontFamily textEditorFontFamily, double textEditorFontSize, string headerText, string footerText)
        {
            this.InitializeComponent();

            TextContent.FontFamily = textEditorFontFamily;
            TextContent.FontSize = textEditorFontSize;

            if (!string.IsNullOrEmpty(headerText))
            {
                Header.Visibility = Visibility.Visible;
                HeaderTextBlock.Text = headerText;
                HeaderTextBlock.FontFamily = textEditorFontFamily;
                HeaderTextBlock.FontSize = textEditorFontSize + 2;
            }
            else
            {
                Header.Visibility = Visibility.Collapsed;
                HeaderTextBlock.FontFamily = textEditorFontFamily;
                HeaderTextBlock.FontSize = textEditorFontSize + 2;
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

            var run = new Run();
            run.Text = textEditorText;
            TextEditorContent.Inlines.Add(run);
        }
    }
}
