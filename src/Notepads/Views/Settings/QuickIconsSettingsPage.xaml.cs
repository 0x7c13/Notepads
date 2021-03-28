namespace Notepads.Views.Settings
{
    using Notepads.Services;
    using Notepads.Settings;
    using Notepads.Utilities;
    using System.Collections;
    using System.Collections.Generic;
    using System.Collections.ObjectModel;
    using System.Data;
    using System.Linq;
    using Windows.Globalization;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;

    public sealed partial class QuickIconsSettingsPage : Page
    {
        private readonly IReadOnlyCollection<LanguageItem> SupportedLanguages = LanguageUtility.GetSupportedLanguageItems();

        public QuickIconsSettingsPage()
        {
            InitializeComponent();
             

            Loaded += AdvancedSettings_Loaded;
            Unloaded += AdvancedSettings_Unloaded;
        }

        private readonly IDictionary<string,ToggleSwitch> toggles = new Dictionary<string, ToggleSwitch>(); 
         

        private void AdvancedSettings_Loaded(object sender, RoutedEventArgs e)
        {
            toggles.Add("PinTop",  ShowQuickPinTopToggleSwitch);
            toggles.Add("Print",   ShowQuickPrintToggleSwitch);
            toggles.Add("Save",    ShowQuickSaveToggleSwitch);
            toggles.Add("History", ShowQuickHistoryToggleSwitch);

             
            AppSettingsService.TopQuickButtons.ForEach(f => toggles[f].IsOn = true);

            toggles.Values.ToList().ForEach(f=> f.Toggled += ShowQuickButtonToggleSwitch_Toggled); 
        }

        private void AdvancedSettings_Unloaded(object sender, RoutedEventArgs e)
        {

            toggles.Values.ToList().ForEach(f => f.Toggled -= ShowQuickButtonToggleSwitch_Toggled); 
        }

        private void ShowQuickButtonToggleSwitch_Toggled(object sender, RoutedEventArgs e)
        { 
            
            if(sender is ToggleSwitch toggle)
            {
                bool isOn = toggle.IsOn;
                string nameBtn = toggle.Tag.ToString();

                AppSettingsService.TopQuickButtonsAdd(nameBtn, isOn);
            }
        } 
    }
}