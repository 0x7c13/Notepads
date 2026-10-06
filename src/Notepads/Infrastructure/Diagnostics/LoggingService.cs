// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.Threading;
using System.Threading.Tasks;
using Windows.Storage;

namespace Notepads.Infrastructure.Diagnostics;

public static class LoggingService
{
    private const string MessageFormatString = "{0} [{1}] {2}"; // {timestamp} [{level}] {message}

    private static readonly ConcurrentQueue<string> MessageQueue = new();
    private static readonly SemaphoreSlim SemaphoreSlim = new(1, 1);
    private static readonly TimeSpan LoggingInterval = TimeSpan.FromSeconds(10);
    private static readonly List<string> Messages = new();

    private static ILogSink _sink;
    private static Task _backgroundTask;
    private static bool _initialized;

    internal static async Task InitializeAsync(ILogSink sink)
    {
        if (sink == null) throw new ArgumentNullException(nameof(sink));
        await InitializeLogFileWriterBackgroundTaskAsync(sink);
    }

    public static StorageFile GetLogFile()
    {
        return _sink?.File;
    }

    internal static Task<bool> FlushAsync() => TryFlushMessageQueueAsync();

    internal static async Task FlushUntilAsync(DateTimeOffset deadline)
    {
        var remaining = deadline - DateTimeOffset.Now;
        if (remaining <= TimeSpan.Zero) return;
        // Flush owns its sink and semaphore until it finishes, including after
        // the suspension budget expires. Its exceptions are handled internally.
        var flush = TryFlushMessageQueueAsync();
        await Task.WhenAny(flush, Task.Delay(remaining));
    }

    internal static Task<bool> ResumeAsync() => InitializeLogFileWriterBackgroundTaskAsync();

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
        string timeStamp = DateTime.UtcNow.ToString(CultureInfo.InvariantCulture);
        string formattedMessage = string.Format(MessageFormatString, timeStamp, level, message);

        // Print to console
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

    private static async Task<bool> InitializeLogFileWriterBackgroundTaskAsync(ILogSink sink = null)
    {
        await SemaphoreSlim.WaitAsync();

        if (_backgroundTask != null && !_backgroundTask.IsCompleted)
        {
            SemaphoreSlim.Release();
            return false;
        }

        try
        {
            if (_sink == null) _sink = sink;
            if (_sink == null) return false;
            await _sink.InitializeAsync();

            _backgroundTask = Task.Run(WriteLogMessagesAsync);

            _initialized = true;
            LogInfo($"Log file location: {_sink.File.Path}", true);
            return true;
        }
        catch (Exception ex)
        {
            Debug.WriteLine(ex);
        }
        finally
        {
            SemaphoreSlim.Release();
        }

        return false;
    }

    private static async Task WriteLogMessagesAsync()
    {
        while (true)
        {
            await Task.Delay(LoggingInterval).ConfigureAwait(false);

            // We will try to write all pending messages in our next attempt, if the current attempt failed
            // However, if the size of messages has become abnormally big, we know something is wrong and should abort at this point
            if (!await TryFlushMessageQueueAsync() && Messages.Count > 1000)
            {
                break;
            }
        }
    }

    private static async Task<bool> TryFlushMessageQueueAsync()
    {
        if (!_initialized)
        {
            return false;
        }

        await SemaphoreSlim.WaitAsync();

        try
        {
            if (MessageQueue.Count == 0)
            {
                return true;
            }

            while (MessageQueue.TryDequeue(out string message))
            {
                Messages.Add(message);
            }

            await _sink.AppendAsync(Messages);
            Messages.Clear();
            return true;
        }
        catch (Exception ex)
        {
            Debug.WriteLine(ex);
        }
        finally
        {
            SemaphoreSlim.Release();
        }

        return false;
    }
}
