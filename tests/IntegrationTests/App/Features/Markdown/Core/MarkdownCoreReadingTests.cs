// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;

namespace NotepadsIntegrationTests;

// Run the original component reading diagnostics plus inline-code and script regressions
// through the actual source projection and native renderer.
internal static class MarkdownCoreReadingTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        string[] expected =
        [
            "reading_snapshot_inline_main", "reading_snapshot_cr_lf_crlf", "reading_snapshot_table_task_image",
            "reading_snapshot_empty_main", "reading_snapshot_scalar_batch_boundary",
            "reading_snapshot_faults_and_owners", "reading_snapshot_math_source_export",
            "reading_snapshot_diagram_native_artifact", "reading_measurement_cache_geometry_and_lifetime",
            "reading_search_literal_budget_and_permits",
            "reading_outline_metadata_fragments_budget_and_lifetime",
            "reading_outline_empty_headings_current_block_geometry",
            "reading_task_action_transport_cluster_focus_and_lifetime",
            "reading_inline_code_badges_geometry_pixels_and_selection",
            "reading_script_shaping_scopes_objects_cache_and_lifetime",
            "reading_ruby_annotation_rendering_and_child_geometry",
            "reading_static_html_checkbox_geometry_mapping_and_lifetime",
            "reading_html_table_spans_caption_geometry_and_lifetime",
            "reading_text_bitmap_cache_pixels_and_lifetime",
            "reading_static_text_bitmap_cache_pixels_and_lifetime",
            "reading_list_marker_gutter_batch_cache_and_narrow_restore",
            "reading_table_page_navigation_document_edges",
            "reading_table_page_navigation_wrapped_final_row",
            "reading_table_colspan_navigation_across_cell_padding",
            "reading_table_page_fallback_ranks_cells_on_same_row",
            "reading_pointer_hit_preserves_shared_wrap_side",
            "reading_wide_table_lines_address_their_materialized_cell",
            "reading_math_fallback_normalizes_mandatory_line_breaks",
            "reading_visible_line_index_includes_nested_script_geometry",
            "reading_edit_image_width_proof_matches_measured_tables",
            "reading_cached_preview_after_stale_native_unload", "reading_math_help_text_on_relayout",
            "reading_ui_begun_worker_resumed_surface_pixels_and_cleanup"
        ];
        var operation = CoreValidationProbe.RunReadingAsync();
        Task<CoreValidationReport> pending = operation.AsTask();
        CoreValidationReport result;
        try { result = await pending.WaitAsync(TimeSpan.FromSeconds(45)); }
        catch (TimeoutException)
        {
            operation.Cancel();
            _ = ObserveLateFailureAsync(pending);
            throw;
        }
        CoreValidationCheck[] checks = result.Checks;
        if (checks.Length != expected.Length || !result.Passed ||
            expected.Any(name => checks.Count(check => check.Name == name) != 1) ||
            checks.Any(check => !check.Passed))
        {
            foreach (var check in checks)
                log.AppendLine($"Native reading {check.Name}: passed={check.Passed}; status={check.Status}; {check.Detail}");
            throw new InvalidOperationException("The component reading diagnostics and rendering regressions must all pass once.");
        }
        foreach (var check in checks)
            log.AppendLine($"PASS: native reading {check.Name}: status={check.Status}; {check.Detail}");
        log.AppendLine($"PASS: exact component reading diagnostic set ({checks.Length} cases, Core ABI {result.CoreAbiVersion}).");
    }

    private static async Task ObserveLateFailureAsync(Task pending)
    {
        try { await pending; }
        catch { }
    }
}
