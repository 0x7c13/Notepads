// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Services
{
    using System;
    using System.Collections.Generic;
    using Microsoft.Services.Store.Engagement;

    public static class AnalyticsService
    {
        private static StoreServicesCustomEventLogger Logger;

        static AnalyticsService()
        {
            try
            {
                Logger = StoreServicesCustomEventLogger.GetDefault();
            }
            catch
            {
                // best effort
            }
        }


        public static void TrackEvent(string eventName, IDictionary<string, string> properties = null)
        {
            try
            {
                // TODO: Simplify this to use a single line, reduce complexity of properties by all the callers
                //string eventMessage = properties != null
                //    ? $"{eventName} - {Newtonsoft.Json.JsonConvert.SerializeObject(properties)}"
                //    : eventName;
                Logger?.Log(eventName);
            }
            catch
            {
                // best effort
            }
        }

        public static void TrackError(Exception exception, IDictionary<string, string> properties = null)
        {
            try
            {
                Logger?.Log($"Error - {exception.Message} - {exception.StackTrace}");
            }
            catch
            {
                // best effort
            }
        }
    }
}