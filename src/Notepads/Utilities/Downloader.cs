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