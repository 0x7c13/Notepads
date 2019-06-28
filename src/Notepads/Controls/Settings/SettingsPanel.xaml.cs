
namespace Notepads.Controls.Settings
{
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
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;
    using Windows.UI.Xaml.Media.Animation;
    using Windows.UI.Xaml.Navigation;

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
