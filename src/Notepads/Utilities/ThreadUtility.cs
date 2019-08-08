
namespace Notepads.Utilities
{
    using Windows.UI.Core;

    internal static class ThreadUtility
    {
        public static bool IsOnUIThread()
        {
            CoreWindow coreWindow = CoreWindow.GetForCurrentThread();
            return coreWindow != null && coreWindow.Dispatcher.HasThreadAccess;
        }
    }
}
