namespace Notepads.Services
{
    using System;
    using System.Threading.Tasks;
    using Notepads.Settings;
    using Windows.ApplicationModel;
    using Windows.UI.StartScreen;

    public static class JumpListService
    {
        public static bool IsJumpListOutOfDate
        {
            get
            {
                if (ApplicationSettingsStore.Read(SettingsKey.IsJumpListOutOfDateBool) is bool isJumpListOutOfDate)
                {
                    return isJumpListOutOfDate;
                }

                return true;
            }
            set => ApplicationSettingsStore.Write(SettingsKey.IsJumpListOutOfDateBool, value);
        }

        public static async Task<bool> UpdateJumpList()
        {
            if (!JumpList.IsSupported()) return false;

            try
            {
                JumpList jumpList = await JumpList.LoadCurrentAsync();
                jumpList.Items.Clear();
                jumpList.SystemGroupKind = JumpListSystemGroupKind.None;

                jumpList.Items.Add(GetNewWindowItem());

                await jumpList.SaveAsync();

                return true;
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"JumpListService_FailedToSetupJumpList: {ex.Message}");
            }

            return false;
        }

        private static JumpListItem GetNewWindowItem()
        {
            string packageId = Package.Current.Id.Name;
            var item = JumpListItem.CreateWithArguments("notepads://newinstance", $"ms-resource://{packageId}/Resources/JumpList_Tasks_NewWindow_Title");
            item.Description = $"ms-resource://{packageId}/Resources/JumpList_Tasks_NewWindow_Description";
            item.Logo = new Uri($"ms-appx:///Assets/Square44x44Logo.png");
            return item;
        }
    }
}