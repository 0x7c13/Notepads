namespace Notepads.Services
{
    using System;
    using System.Collections.Concurrent;
    using System.Collections.Generic;
    using System.Diagnostics;
    using System.Globalization;
    using System.Threading;
    using System.Threading.Tasks;
    using Notepads.Utilities;
    using Windows.ApplicationModel.Core;
    using Windows.Storage;

    public static class LoggingService
    {
        private const string MessageFormat = "{0} [{1}] {2}"; // {timestamp} [{level}] {message}

        private static readonly ConcurrentQueue<string> MessageQueue = new ConcurrentQueue<string>();
        private static readonly SemaphoreSlim SemaphoreSlim = new SemaphoreSlim(1, 1);
        private static readonly TimeSpan LoggingInterval = TimeSpan.FromSeconds(10);
        private static readonly List<string> Messages = new List<string>();

        private static StorageFile _logFile;
        private static Task _backgroundTask;
        private static bool _initialized;

        public static async Task InitializeAsync()
        {
            if (_initialized)
            {
                return;
            }

            CoreApplication.Suspending += async (sender, args) => { await TryFlushMessageQueueAsync(); };
            CoreApplication.Resuming += async (sender, args) => { await InitializeBackgroundTaskAsync(); };

            await InitializeBackgroundTaskAsync();
        }

        public static void LogInfo(string message, bool consoleOnly = false)
        {
            LogMessage("Info", message, consoleOnly);
        }

        public static void LogWarning(string message, bool consoleOnly = false)
        {
            LogMessage("Warning", message, consoleOnly);
        }

        public static void LogError(string message, bool consoleOnly = false)
        {
            LogMessage("Error", message, consoleOnly);
        }

        public static void LogException(Exception ex, bool consoleOnly = false)
        {
            if (ex == null)
            {
                return;
            }

            LogError(ex.ToString(), consoleOnly);
        }

        private static void LogMessage(string level, string message, bool consoleOnly)
        {
            string formattedMessage = string.Format(MessageFormat, DateTime.UtcNow.ToString(CultureInfo.InvariantCulture), level, message);

            // Print out to debug
            Debug.WriteLine(formattedMessage);

            if (!_initialized)
            {
                return;
            }

            if (!consoleOnly)
            {
                // Add to message queue
                MessageQueue.Enqueue(formattedMessage);
            }
        }

        private static async Task InitializeBackgroundTaskAsync()
        {
            await SemaphoreSlim.WaitAsync();

            if (_backgroundTask != null && !_backgroundTask.IsCompleted)
            {
                SemaphoreSlim.Release();
                return;
            }

            if (_logFile == null)
            {
                StorageFolder logsFolder = await FileSystemUtility.GetOrCreateAppFolder("Logs");
                _logFile = await FileSystemUtility.CreateFile(logsFolder, DateTime.UtcNow.ToString("yyyyMMddTHHmmss", CultureInfo.InvariantCulture) + ".log");
            }

            try
            {
                _backgroundTask = Task.Run(
                    async () =>
                    {
                        while (true)
                        {
                            Thread.Sleep(LoggingInterval);

                            // We will try to write all pending messages in our next attempt, if the current attempt failed
                            // However, if the size of _messages has become abnormally big, we know something is wrong and should abort at this point
                            if (!await TryFlushMessageQueueAsync() && Messages.Count > 1000)
                            {
                                break;
                            }
                        }
                    });

                _initialized = true;
                LogInfo($"Log file location: {_logFile.Path}", true);
            }
            catch
            {
                // Ignore
            }

            SemaphoreSlim.Release();
        }

        private static async Task<bool> TryFlushMessageQueueAsync()
        {
            if (!_initialized)
            {
                return false;
            }

            await SemaphoreSlim.WaitAsync();

            while (MessageQueue.TryDequeue(out string message))
            {
                Messages.Add(message);
            }

            bool success = true;

            try
            {
                await FileIO.AppendLinesAsync(_logFile, Messages);
                Messages.Clear();
            }
            catch
            {
                success = false;
            }

            SemaphoreSlim.Release();
            return success;
        }
    }
}