
namespace Notepads.Controls.Settings
{
    using System;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media.Animation;

    public sealed partial class SettingsPanel : Page
    {
        public SettingsPanel()
        {
            InitializeComponent();
        }

        public void Show(string title)
        {
            Type pageType;

            switch (title)
            {
                case "Text & Editor":
                    pageType = typeof(TextAndEditorSettings);
                    break;
                case "Personalization":
                    pageType = typeof(PersonalizationSettings);
                    break;
                case "Advanced":
                    pageType = typeof(AdvancedSettings);
                    break;
                case "About":
                    pageType = typeof(AboutPage);
                    break;
                default:
                    pageType = typeof(TextAndEditorSettings);
                    break;
            }

            TitleTextBlock.Text = title;
            ContentFrame.Navigate(pageType, null, new SuppressNavigationTransitionInfo());
        }
    }
}
