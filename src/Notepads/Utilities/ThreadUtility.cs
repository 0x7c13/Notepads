namespace Notepads.Utilities
{
    using System;
    using System.Threading.Tasks;
    using Windows.UI.Core;

    internal static class ThreadUtility
    {
        public static bool IsOnUIThread()
        {
            CoreWindow coreWindow = CoreWindow.GetForCurrentThread();
            return coreWindow != null && coreWindow.Dispatcher.HasThreadAccess;
        }

        public static async Task CallOnUIThreadAsync(CoreDispatcher dispatcher, DispatchedHandler handler)
        {
            await dispatcher.RunAsync(CoreDispatcherPriority.Normal, handler);
        }

        //public static async Task CallOnMainViewUIThreadAsync(DispatchedHandler handler) => 
        //    await CallOnUIThreadAsync(CoreApplication.MainView.CoreWindow.Dispatcher, handler);
    }
}