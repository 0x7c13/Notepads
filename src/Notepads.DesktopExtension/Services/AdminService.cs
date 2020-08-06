namespace Notepads.DesktopExtension.Services
{
    using Notepads.Settings;
    using System;
    using System.IO;
    using System.IO.MemoryMappedFiles;
    using System.Threading.Tasks;
    using Windows.Storage;

    // NOTE: You can use the "Rename" command on the "Refactor" menu to change the class name "AdminService" in both code and config file together.
    public class AdminService : IAdminService
    {
        public async Task<bool> SaveFile(string memoryMapName, string filePath, int dataArrayLength)
        {
            try
            {
                var localSettings = ApplicationData.Current.LocalSettings;
                if (!localSettings.Values.ContainsKey(SettingsKey.AdminAuthenticationTokenStr) ||
                    !(localSettings.Values[SettingsKey.AdminAuthenticationTokenStr] is string token)) return false;
                localSettings.Values.Remove(SettingsKey.AdminAuthenticationTokenStr);

                // Open the memory-mapped file.
                using (var mmf = MemoryMappedFile.OpenExisting(memoryMapName))
                {
                    using (var stream = mmf.CreateViewStream())
                    {
                        var reader = new BinaryReader(stream);
                        var data = reader.ReadBytes(dataArrayLength);

                        await PathIO.WriteBytesAsync(filePath, data);
                    }
                }

                return true;
            }
            catch (Exception)
            {
                return false;
            }
        }
    }
}
