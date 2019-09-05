namespace Notepads.Utilities
{
    using System;
    using System.IO;
    using System.Net;
    using System.Threading.Tasks;

    public class Downloader : IDisposable
    {
        public async Task<MemoryStream> GetDataFeed(string feedUrl)
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

        public void Dispose()
        {
        }
    }
}