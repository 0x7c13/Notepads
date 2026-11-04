// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;

namespace NotepadsDragDropPeer;

internal sealed class PeerChannel : IDisposable
{
    internal static readonly UTF8Encoding Utf8 = new(false, true);
    private readonly string directory;
    private readonly FileStream ownership;
    private readonly object writeLock = new();

    internal PeerChannel(string path)
    {
        if (!Path.IsPathFullyQualified(path) || !Directory.Exists(path))
        {
            throw new ArgumentException("Channel must be an existing absolute directory.");
        }

        directory = Path.GetFullPath(path);
        bool insideArtifacts = false;
        for (DirectoryInfo? current = new(directory); current is not null; current = current.Parent)
        {
            if ((current.Attributes & FileAttributes.ReparsePoint) != 0)
            {
                throw new ArgumentException("Channel ancestors must not be reparse points.");
            }

            insideArtifacts |= current.Parent is not null &&
                string.Equals(current.Parent.Name, "artifacts", StringComparison.OrdinalIgnoreCase);
        }

        if (!insideArtifacts || Directory.EnumerateFileSystemEntries(directory).Any())
        {
            throw new ArgumentException("Channel must be a new empty directory beneath artifacts.");
        }

        ownership = new FileStream(PathFor("channel.lock"), FileMode.CreateNew, FileAccess.Write, FileShare.None);
    }

    internal string PathFor(string name)
    {
        if (name != Path.GetFileName(name) || name.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0)
        {
            throw new ArgumentException("Invalid channel file name.");
        }

        return Path.Combine(directory, name);
    }

    internal string? Read(string name)
    {
        string path = PathFor(name);
        if (!File.Exists(path)) return null;
        using FileStream stream = new(path, FileMode.Open, FileAccess.Read, FileShare.Read | FileShare.Delete);
        using StreamReader reader = new(stream, Utf8);
        return reader.ReadToEnd();
    }

    internal void WriteAtomic(string name, string value)
    {
        lock (writeLock)
        {
            string path = PathFor(name);
            string temporary = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
            File.WriteAllText(temporary, value, Utf8);
            File.Move(temporary, path, true);
        }
    }

    internal void Event(string caseId, string eventName, int requestId = 0, string detail = "")
    {
        lock (writeLock)
        {
            string path = PathFor(caseId + ".events.tsv");
            if (!File.Exists(path))
            {
                File.WriteAllText(path, "ticks\tpid\tthreadId\tevent\trequestId\tdetail64\r\n", Utf8);
            }

            string line = string.Join('\t',
                DateTime.UtcNow.Ticks.ToString(CultureInfo.InvariantCulture),
                Environment.ProcessId.ToString(CultureInfo.InvariantCulture),
                Environment.CurrentManagedThreadId.ToString(CultureInfo.InvariantCulture),
                eventName,
                requestId.ToString(CultureInfo.InvariantCulture),
                Encode(detail));
            File.AppendAllText(path, line + "\r\n", Utf8);
        }
    }

    // These gates deliberately do not depend on the UI timer or message pump: GetData and
    // DragDrop are synchronous OLE callbacks. No DoEvents permits a reset to reenter them.
    internal bool WaitForRelease(string caseId, string suffix, int requestId)
    {
        Stopwatch timeout = Stopwatch.StartNew();
        while (timeout.Elapsed < TimeSpan.FromSeconds(12))
        {
            string? command = Read(caseId + "." + suffix);
            if (command is not null)
            {
                command = command.Trim();
                if (command is not "ok" and not "fail")
                {
                    throw new InvalidDataException("Gate release must contain ok or fail.");
                }

                Event(caseId, suffix, requestId, command);
                return command == "ok";
            }

            Thread.Sleep(15);
        }

        Event(caseId, suffix + "-timeout", requestId);
        return false;
    }

    internal static string Encode(string value) => Convert.ToBase64String(Utf8.GetBytes(value));

    internal static string Decode(string value)
    {
        string decoded = Utf8.GetString(Convert.FromBase64String(value));
        if (decoded.IndexOf('\0') >= 0)
        {
            throw new InvalidDataException("Text fixtures must not contain embedded NUL characters.");
        }

        return decoded;
    }

    public void Dispose() => ownership.Dispose();
}
