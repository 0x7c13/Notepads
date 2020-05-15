namespace Notepads.Extensions
{
    using System;
    using System.Threading.Tasks;
    using Windows.UI.Core;

    public static class DispatcherExtensions
    {
        public static async Task CallOnUIThreadAsync(this CoreDispatcher dispatcher, DispatchedHandler handler)
        {
            await dispatcher.RunAsync(CoreDispatcherPriority.Normal, handler);
        }
    }
}