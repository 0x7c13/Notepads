namespace Notepads.DesktopExtension
{
    using System;
    using System.Threading.Tasks;
    using Windows.Storage;

    // NOTE: You can use the "Rename" command on the "Refactor" menu to change the class name "AdminService" in both code and config file together.
    public class AdminService : IAdminService
    {
        internal static string AdminAuthenticationTokenStr = "AdminAuthenticationTokenStr";

        public async Task<bool> SaveFile(string filePath, byte[] data)
        {
            try
            {
                var localSettings = ApplicationData.Current.LocalSettings;
                if (!localSettings.Values.ContainsKey(AdminAuthenticationTokenStr) ||
                    !(localSettings.Values[AdminAuthenticationTokenStr] is string token)) return false;
                localSettings.Values.Remove(AdminAuthenticationTokenStr);
                await PathIO.WriteBytesAsync(filePath, data);
                return true;
            }
            catch (Exception)
            {
                return false;
            }
        }
    }
}
