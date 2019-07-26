
namespace Notepads.Services
{
    using System;
    using System.Collections.Concurrent;
    using System.Collections.Generic;
    using System.Globalization;
    using System.Threading;
    using System.Threading.Tasks;
    using Notepads.Utilities;
    using Windows.ApplicationModel.Core;
    using Windows.Storage;

    public static class LoggingService
    {
        private const string _messageFormat = "{0} [{1}] {2}"; // {timestamp} [{level}] {message}

        private static readonly ConcurrentQueue<string> _messageQueue = new ConcurrentQueue<string>();
        private static readonly SemaphoreSlim _semaphoreSlim = new SemaphoreSlim(1, 1);
        private static readonly TimeSpan _loggingInterval = TimeSpan.FromSeconds(10);
        private static readonly List<string> _messages = new List<string>();

        private static StorageFile _logFile;
        private static bool _initialized;

        public static async Task InitializeAsync()
        {
            CoreApplication.Suspending += async (sender, args) => { await TryFlushMessageQueueAsync(); };

            await InitializeBackgroundThreadAsync();
        }

        public static void LogInfo(string message)
        {
            LogMessage("Info", message);
        }

        public static void LogWarning(string message)
        {
            LogMessage("Warning", message);
        }

        public static void LogError(string message)
        {
            LogMessage("Error", message);
        }

        public static void LogException(Exception ex)
        {
            if (ex == null)
            {
                return;
            }

            LogError(ex.ToString());
        }

        private static void LogMessage(string level, string message)
        {
            if (!_initialized)
            {
                return;
            }

            _messageQueue.Enqueue(String.Format(_messageFormat, DateTime.UtcNow.ToString(CultureInfo.InvariantCulture), level, message));
        }

        private static async Task InitializeBackgroundThreadAsync()
        {
            StorageFolder logsFolder = await FileSystemUtility.GetOrCreateAppFolder("Logs");
            _logFile = await FileSystemUtility.CreateFile(logsFolder, DateTime.UtcNow.ToString("yyyyMMddTHHmmss", CultureInfo.InvariantCulture) + ".log");

            try
            {
                _ = Task.Run(
                    async () =>
                    {
                        while (true)
                        {
                            Thread.Sleep(_loggingInterval);

                            // We will try to write all pending messages in our next attempt, if the current attempt failed
                            // However, if the size of _messages has become abnormally big, we know something is wrong and should abort at this point
                            if (!await TryFlushMessageQueueAsync() && _messages.Count > 1000)
                            {
                                break;
                            }
                        }
                    });

                _initialized = true;
            }
            catch
            {
                // Ignore
            }
        }

        private static async Task<bool> TryFlushMessageQueueAsync()
        {
            await _semaphoreSlim.WaitAsync();

            while (_messageQueue.TryDequeue(out string message))
            {
                _messages.Add(message);
            }

            bool success = true;

            try
            {
                await FileIO.AppendLinesAsync(_logFile, _messages);
                _messages.Clear();
            }
            catch
            {
                success = false;
            }

            _semaphoreSlim.Release();
            return success;
        }
    }
}
