// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;

namespace NotepadsIntegrationTests;

internal static class MarkdownCoreImageTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        var window = Window.Current ?? throw new InvalidOperationException("Native image diagnostics require their own test window.");
        var previous = window.Content;
        var root = new Grid();
        window.Content = root;
        window.Activate();
        try { await RunDiagnosticsAsync(log); }
        finally
        {
            if (ReferenceEquals(window.Content, root)) window.Content = previous;
        }
    }

    private static async Task RunDiagnosticsAsync(StringBuilder log)
    {
        string[] expected =
        [
            "image_decode_bmp_independent_pixels", "image_decode_bad_and_unsupported",
            "image_decode_dimension_guard", "image_decode_cancel_and_stale",
            "image_decode_exif_transpose_antitranspose", "image_decode_gif_canvas_contract",
            "image_decode_xml_dispatch", "image_font_watch_stop_reload",
            "image_font_watch_idle_destroy", "image_font_watch_notification_owners",
            "image_raw_encoded_failed_free_process_budget", "image_metadata_forget_readmit_pending_failed_wic",
            "image_svg_native_worker_and_finish", "image_svg_retained_owners_and_release",
            "image_decode_last_native_owner", "image_font_live_view_refresh_recovery",
            "image_viewport_canvas_padding_priority", "image_payload_independent_last_owners"
        ];
        var operation = CoreValidationProbe.RunImagesAsync();
        Task<CoreValidationReport> pending = operation.AsTask();
        CoreValidationReport result;
        try { result = await pending.WaitAsync(TimeSpan.FromSeconds(180)); }
        catch (TimeoutException)
        {
            operation.Cancel();
            _ = ObserveLateFailureAsync(pending);
            throw;
        }
        CoreValidationCheck[] checks = result.Checks;
        foreach (var check in checks)
            log.AppendLine($"Native image {check.Name}: passed={check.Passed}; status={check.Status}; {check.Detail}");
        if (checks.Length != expected.Length || !result.Passed ||
            expected.Any(name => checks.Count(check => check.Name == name) != 1) ||
            checks.Any(check => !check.Passed))
            throw new InvalidOperationException("All native image diagnostics, including the real pre-frame reading publication interleaving, must pass once.");
        log.AppendLine($"PASS: exact component image diagnostic set ({checks.Length} cases, Core ABI {result.CoreAbiVersion}).");
    }

    private static async Task ObserveLateFailureAsync(Task pending)
    {
        try { await pending; }
        catch { }
    }
}
