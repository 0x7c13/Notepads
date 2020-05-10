using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.Serialization;
using System.ServiceModel;
using System.Text;
using System.Threading.Tasks;
using System.Windows.Forms;
using Windows.Storage;

namespace Notepads.DesktopExtension
{
    // NOTE: You can use the "Rename" command on the "Refactor" menu to change the class name "AdminService" in both code and config file together.
    public class AdminService : IAdminService
    {
        internal static string AdminAuthenticationTokenStr = "AdminAuthenticationTokenStr";

        public bool SaveFile(string filePath, byte[] data)
        {
            try
            {
                var localSettings = ApplicationData.Current.LocalSettings;
                if (!localSettings.Values.ContainsKey(AdminAuthenticationTokenStr) ||
                    !(localSettings.Values[AdminAuthenticationTokenStr] is string token)) return false;
                localSettings.Values.Remove(AdminAuthenticationTokenStr);
                WriteToFile(filePath, data);
                return true;
            }
            catch (Exception)
            {
                return false;
            }
        }

        private async void WriteToFile(string filePath, byte[] data)
        {
            await PathIO.WriteBytesAsync(filePath, data);
        }
    }
}
