
namespace Notepads.Settings
{
    using System;
    using Windows.Storage;

    public class ApplicationSettings
    {
        public static object Read(string key)
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

        public static void Write(string key, object obj, bool saveToRoaming = false)
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
