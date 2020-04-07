namespace Notepads.Controls.Print
{
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Documents;
    using Windows.UI.Xaml.Media;

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
}
