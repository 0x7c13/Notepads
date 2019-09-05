namespace Notepads.Controls.Settings
{
    using System;
    using Notepads.Services;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media.Animation;

    public sealed partial class SettingsPanel : Page
    {
        public SettingsPanel()
        {
            InitializeComponent();
        }

        public void Show(string title, string tag)
        {
            Type pageType;

            switch (tag)
            {
                case "TextAndEditor":
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

            LoggingService.LogInfo($"Navigating to: {tag} Page", consoleOnly: true);
            TitleTextBlock.Text = title;
            ContentFrame.Navigate(pageType, null, new SuppressNavigationTransitionInfo());
        }
    }
}