namespace Notepads.Services
{
    public class NotificationCenter
    {
        private static NotificationCenter _instance;

        private INotificationDelegate _notificationDelegate;

        private NotificationCenter()
        {
        }

        public static NotificationCenter Instance => _instance ?? (_instance = new NotificationCenter());

        public void SetNotificationDelegate(INotificationDelegate notificationDelegate)
        {
            _notificationDelegate = notificationDelegate;
        }

        public void PostNotification(string notification, int duration)
        {
            _notificationDelegate?.PostNotification(notification, duration);
        }
    }

    public interface INotificationDelegate
    {
        void PostNotification(string notification, int duration);
    }
}