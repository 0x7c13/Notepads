// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

global using Clipboard = NotepadsIntegrationTests.TestClipboard;
using System;
using Windows.ApplicationModel.DataTransfer;

namespace NotepadsIntegrationTests;

// Preserve the production DataPackage and copy/cut/paste code. The outbound OS
// boundary is explicit: ordinary tests retain clipboard/history/roaming isolation.
internal static class TestClipboard
{
    private static DataPackage _content = new();
    // Only the explicitly invoked external Markdown keyboard fixture opts in.
    // Ordinary integration groups retain their isolated outbound boundary.
    internal static bool UseSystemClipboard { get; set; }
    internal static bool AcceptWrites { get; set; } = true;
    internal static Exception WriteFailure { get; set; }
    internal static Exception FlushFailure { get; set; }
    internal static int FlushCount { get; private set; }
    internal static ClipboardContentOptions LastOptions { get; private set; }
    public static DataPackageView GetContent() => UseSystemClipboard
        ? Windows.ApplicationModel.DataTransfer.Clipboard.GetContent() : _content.GetView();
    public static void SetContent(DataPackage content) => _content = content;
    public static bool SetContentWithOptions(DataPackage content, ClipboardContentOptions options)
    {
        LastOptions = options;
        if (WriteFailure != null) throw WriteFailure;
        if (!AcceptWrites) return false;
        _content = content;
        return true;
    }
    public static void Flush()
    {
        FlushCount++;
        if (FlushFailure != null) throw FlushFailure;
    }
    internal static void Reset()
    {
        _content = new DataPackage();
        UseSystemClipboard = false;
        AcceptWrites = true;
        FlushCount = 0;
        LastOptions = null;
        WriteFailure = FlushFailure = null;
    }
}
