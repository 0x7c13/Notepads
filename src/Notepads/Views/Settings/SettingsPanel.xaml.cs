namespace Notepads.Views.Settings
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
                    pageType = typeof(TextAndEditorSettingsPage);
                    break;
                case "Personalization":
                    pageType = typeof(PersonalizationSettingsPage);
                    break;
                case "Advanced":
                    pageType = typeof(AdvancedSettingsPage);
                    break;
                case "QuickIcons":
                    pageType = typeof(QuickIconsSettingsPage);
                    break;
                case "About":
                    pageType = typeof(AboutPage);
                    break;
                default:
                    pageType = typeof(TextAndEditorSettingsPage);
                    break;
            }

            LoggingService.LogInfo($"[{nameof(SettingsPanel)}] Navigating to: {tag} Page", consoleOnly: true);
            TitleTextBlock.Text = title;
            ContentFrame.Navigate(pageType, null, new SuppressNavigationTransitionInfo());
        }
    }
}