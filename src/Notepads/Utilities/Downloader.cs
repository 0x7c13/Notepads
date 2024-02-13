// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Utilities
{
    using System.IO;
    using System.Net;
    using System.Threading.Tasks;

    public static class Downloader
    {
        public static async Task<MemoryStream> GetDataFeedAsync(string feedUrl)
        {
            using (var ms = new MemoryStream())
            {
                var request = (HttpWebRequest)WebRequest.Create(feedUrl);
                request.Method = "GET";
                using (var response = (HttpWebResponse)await request.GetResponseAsync())
                {
                    response.GetResponseStream()?.CopyTo(ms);
                    return ms;
                }
            }
        }
    }
}