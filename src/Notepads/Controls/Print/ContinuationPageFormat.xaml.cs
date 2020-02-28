namespace Notepads.Controls.Print
{
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media;

    /// <summary>
    /// An empty page that can be used on its own or navigated to within a Frame.
    /// </summary>
    public sealed partial class ContinuationPageFormat : Page
    {
        public ContinuationPageFormat(RichTextBlockOverflow textLinkContainer, FontFamily textEditorFontFamily, double textEditorFontSize, string headerText, string footerText)
        {
            InitializeComponent();

            if (!string.IsNullOrEmpty(headerText))
            {
                Header.Visibility = Visibility.Visible;
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
}
