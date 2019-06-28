
namespace Notepads.Settings
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using System.Text;
    using System.Threading.Tasks;
    using Windows.Storage;

    public class ApplicationSettings
    {
        public static object ReadAsync(string key)
        {
            object obj = null;

            ApplicationDataContainer localSettings = Windows.Storage.ApplicationData.Current.LocalSettings;
            if (localSettings.Values.ContainsKey(key))
            {
                obj = localSettings.Values[key];
            }
            else
            {
                //ApplicationDataContainer roamingSettings = Windows.Storage.ApplicationData.Current.RoamingSettings;
                //if (roamingSettings.Values.ContainsKey(key))
                //{
                //    obj = roamingSettings.Values[key];
                //    WriteAsync(key, obj, false);
                //}
            }

            return obj;
        }

        public static void WriteAsync(string key, object obj, bool saveToRoaming)
        {
            ApplicationDataContainer localSettings = Windows.Storage.ApplicationData.Current.LocalSettings;
            localSettings.Values[key] = obj;

            //if (saveToRoaming)
            //{
            //    ApplicationDataContainer roamingSettings = Windows.Storage.ApplicationData.Current.RoamingSettings;
            //    roamingSettings.Values[key] = obj;
            //}
        }
    }
}
