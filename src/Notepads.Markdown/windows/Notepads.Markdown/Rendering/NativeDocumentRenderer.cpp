// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
#include "Diagnostics/NativeStartupTrace.h"
#include "Diagnostics/NativeDrawTrace.h"
#endif
#include "Rendering/NativeDocumentRenderer.h"
#include "Rendering/NativeTextBitmapCache.h"
#include "Text/NativeScriptLayout.h"
#include "Text/NativeScriptFonts.h"
#include "Reading/NativeReadingPreparation.h"
#include "Text/NativeScriptTextRenderer.h"
#include "Resources/Images/NativeImageResources.h"
#include "Reading/NativeReadingOutline.h"
#include "notepads/markdown/derived.h"
#include <array>
#include <bit>
#include <cstddef>
#include <dwrite_1.h>
#include <limits>
#include <new>
#include <utility>

namespace Notepads::Markdown::Native
{
    void ValidatePreviewDecoration(notepads_markdown_preview_decoration const &decoration)
    {
        if (decoration.kind < NOTEPADS_MARKDOWN_PREVIEW_DECORATION_BULLET ||
            decoration.kind > NOTEPADS_MARKDOWN_PREVIEW_DECORATION_STATIC_HTML_CHECKBOX || decoration.reserved != 0)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        if (IsPreviewTaskDecoration(decoration.kind) && (decoration.checked > 1 || decoration.has_action > 1))
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        if (decoration.kind == NOTEPADS_MARKDOWN_PREVIEW_DECORATION_STATIC_HTML_CHECKBOX &&
            (decoration.has_action != 0 || decoration.action_start != 0 ||
             decoration.action_end != 0 || decoration.expected != 0))
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
    }

    void ValidatePreviewHtmlCell(notepads_markdown_preview_cell const &cell, notepads_markdown_preview_html_cell const &metadata)
    {
        auto invalid = cell.node != metadata.node || metadata.reserved != 0 || metadata.reserved_geometry != 0 ||
                       (metadata.flags & ~NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_MASK) != 0 ||
                       cell.header > 1 || cell.missing > 1 || cell.alignment > 3;
        if (metadata.flags == 0)
            invalid |= metadata.colspan != 1 || metadata.rowspan != 1;
        else
        {
            invalid |= (metadata.flags & NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_HTML) == 0 || cell.missing != 0;
            if (metadata.flags & NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_COVERED)
                invalid |= metadata.colspan != 0 || metadata.rowspan != 0 || cell.header != 0 || cell.alignment != 0;
            else
            {
                invalid |= metadata.colspan == 0 || metadata.colspan > 64 || metadata.rowspan == 0 || metadata.rowspan > 256;
                if (metadata.flags & NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_CAPTION)
                    invalid |= cell.header != 0 || cell.alignment != 2 || metadata.rowspan != 1;
            }
        }
        if (invalid)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
    }

    void ValidatePreviewHtmlTable(std::span<notepads_markdown_preview_cell const> cells,
                                  std::span<notepads_markdown_preview_html_cell const> metadata,
                                  uint32_t rows, uint32_t columns, std::span<uint32_t> owners)
    {
        if (!columns || columns > 64 || !rows || rows > 256 || columns * rows > 4096 ||
            cells.size() != columns * rows || metadata.size() != cells.size() || owners.size() != cells.size())
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        std::fill(owners.begin(), owners.end(), NOTEPADS_MARKDOWN_PREVIEW_NONE);
        for (uint32_t row = 0; row < rows; ++row)
        {
            auto first = row * columns;
            auto rowNode = cells[first].row_node;
            auto caption = (metadata[first].flags & NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_CAPTION) != 0;
            if (caption && (row != 0 || rows < 2))
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            for (uint32_t column = 0; column < columns; ++column)
            {
                auto slot = first + column;
                auto const &cell = cells[slot];
                auto const &span = metadata[slot];
                ValidatePreviewHtmlCell(cell, span);
                bool covered = (span.flags & NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_COVERED) != 0;
                if (!(span.flags & NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_HTML) ||
                    ((span.flags & NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_CAPTION) != 0) != caption ||
                    cell.row != row || cell.column != column || cell.row_node != rowNode ||
                    covered != (owners[slot] != NOTEPADS_MARKDOWN_PREVIEW_NONE))
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                if (covered)
                    continue;
                if (span.colspan > columns - column || span.rowspan > rows - row ||
                    (caption && (column != 0 || span.colspan != columns)))
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                for (uint32_t dy = 0; dy < span.rowspan; ++dy)
                    for (uint32_t dx = 0; dx < span.colspan; ++dx)
                    {
                        auto &owner = owners[(row + dy) * columns + column + dx];
                        if (owner != NOTEPADS_MARKDOWN_PREVIEW_NONE)
                            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                        owner = slot;
                    }
            }
        }
    }

    namespace
    {
        struct TableWidthPlan
        {
            float width{}, padding{}, columnWidth{}, keyWidth{};
            uint32_t columns{};
            bool html{}, rowHeaders{};

            float Edge(uint32_t column) const noexcept
            {
                if (html)
                    return column == columns ? width : width * (static_cast<float>(column) / columns);
                return rowHeaders && column == 1 ? keyWidth : column * columnWidth;
            }
            float TextWidth(uint32_t column, uint32_t span = 1) const noexcept
            {
                return std::max(1.0f, Edge(column + span) - Edge(column) - 2 * padding);
            }
            static TableWidthPlan Html(double width, uint32_t columns) noexcept
            {
                auto value = static_cast<float>(width);
                return {value, std::min(6.0f, value / (4.0f * columns)), 0, 0, columns, true, false};
            }
            static TableWidthPlan Markdown(double width, uint32_t columns, float padding = 6.0f,
                                             bool rowHeaders = false, float keyWidth = 0) noexcept
            {
                return {static_cast<float>(width), padding, static_cast<float>(width / columns),
                    keyWidth, columns, false, rowHeaders};
            }
        };

        bool TableHasRowHeaders(std::span<notepads_markdown_preview_cell const> cells,
                                uint32_t columns, uint32_t rows)
        {
            bool rowHeaders = columns == 2 && cells.size() == uint64_t{rows} * 2;
            for (auto const &cell : cells)
            {
                if (cell.row >= rows || cell.column >= columns)
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
                rowHeaders = rowHeaders && cell.header == (cell.column == 0 ? 1u : 0u);
            }
            return rowHeaders;
        }

        float OrdinaryTextWidth(float width, bool htmlFallback, bool disclosureSummary) noexcept
        {
            const auto inset = htmlFallback ? std::min(8.0f, width * 0.25f) : 0.0f;
            const auto left = inset + (disclosureSummary && width >= 24.0f ? 12.0f : 0.0f);
            return width - left - inset;
        }

        IDWriteFactory *SystemTextFactory()
        {
            // This retains system font-cache state only. No layout, document or
            // private loader is retained or registered on this shared factory.
            static const auto factory = []
            {
                winrt::com_ptr<IDWriteFactory> value;
                winrt::check_hresult(DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED, __uuidof(IDWriteFactory),
                                                       reinterpret_cast<IUnknown **>(value.put())));
                return value;
            }();
            return factory.get();
        }
        // The caller supplies a complete paint clip in pre-transform Draw coordinates.
        // OccupiedEm contains actual fill/stroke geometry, not only inline metrics.
        bool MathReplayOutsidePaintClip(MathInkBounds const &occupied, MathPlacement const &placement,
                                       MathTextAnchor anchor, D2D1_RECT_F const &clip, double margin) noexcept
        {
            auto const scale = static_cast<double>(placement.emToDips);
            if (!std::isfinite(clip.left) || !std::isfinite(clip.top) ||
                !std::isfinite(clip.right) || !std::isfinite(clip.bottom) ||
                clip.left >= clip.right || clip.top >= clip.bottom ||
                !std::isfinite(occupied.left) || !std::isfinite(occupied.top) ||
                !std::isfinite(occupied.right) || !std::isfinite(occupied.bottom) ||
                occupied.left > occupied.right || occupied.top > occupied.bottom ||
                !std::isfinite(scale) || scale <= 0 || !std::isfinite(anchor.left) ||
                !std::isfinite(anchor.baseline) || !std::isfinite(placement.paintOriginX) ||
                !std::isfinite(placement.paintOriginY) || !std::isfinite(margin) || margin < 0)
                return false;
            // Match Replay's rounded float translation after the text/scroll anchor.
            auto const translation = static_cast<double>(anchor.baseline) + placement.paintOriginY;
            if (!std::isfinite(translation) || std::abs(translation) > std::numeric_limits<float>::max())
                return false;
            auto const origin = static_cast<double>(static_cast<float>(translation));
            auto const top = origin + occupied.top * scale;
            auto const bottom = origin + occupied.bottom * scale;
            if (!std::isfinite(top) || !std::isfinite(bottom) || top > bottom)
                return false;
            auto const magnitude = (std::max)({1.0, std::abs(origin), std::abs(top), std::abs(bottom)});
            auto const guard = margin + magnitude * (8.0 * std::numeric_limits<float>::epsilon());
            if (!std::isfinite(guard))
                return false;
            return bottom < static_cast<double>(clip.top) - guard || top > static_cast<double>(clip.bottom) + guard;
        }

        template <typename T> T PreviewPod()
        {
            T result{};
            result.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(T)};
            return result;
        }
        template <typename T> T CorePod()
        {
            T result{};
            result.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(T)};
            return result;
        }
        void Check(int32_t status, wchar_t const *stage = L"preview")
        {
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
                throw PreviewFailure{status, stage};
        }
        float MarkerOrigin(PreviewBlockVisual const &visual, float originX = 0.0f) noexcept
        {
            return originX + static_cast<float>(visual.markerX);
        }
        struct MetricWriter
        {
            std::span<uint8_t> bytes;
            size_t size{};
            bool valid{true};
            void Word(uint32_t value) noexcept
            {
                if (!valid || size > bytes.size() || bytes.size() - size < 4)
                {
                    valid = false;
                    return;
                }
                for (uint32_t i = 0; i < 4; ++i)
                    bytes[size++] = static_cast<uint8_t>(value >> (i * 8));
            }
            void Float(float value) noexcept
            {
                Word(std::bit_cast<uint32_t>(value));
            }
            void Double(double value) noexcept
            {
                auto bits = std::bit_cast<uint64_t>(value);
                Word(static_cast<uint32_t>(bits));
                Word(static_cast<uint32_t>(bits >> 32));
            }
            void Text(std::wstring_view value) noexcept
            {
                if (!valid || size > bytes.size() || value.size() > (bytes.size() - size) / 2)
                {
                    valid = false;
                    return;
                }
                for (wchar_t unit : value)
                {
                    bytes[size++] = static_cast<uint8_t>(unit);
                    bytes[size++] = static_cast<uint8_t>(unit >> 8);
                }
            }
        };
        struct MarkerWidths
        {
            float advance{}, left{}, right{};
            void Include(MarkerWidths const &other) noexcept
            {
                advance = std::max(advance, other.advance);
                left = std::max(left, other.left);
                right = std::max(right, other.right);
            }
        };
        float RoundMarkerWidth(double value)
        {
            if (!std::isfinite(value) || value < 0 || value > std::numeric_limits<float>::max())
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
            auto rounded = std::ceil(value);
            auto result = static_cast<float>(rounded);
            if (static_cast<double>(result) < rounded)
                result = std::nextafter(result, std::numeric_limits<float>::infinity());
            return result;
        }
        size_t MarkerText(notepads_markdown_preview_decoration const &decoration, std::span<wchar_t> output)
        {
            ValidatePreviewDecoration(decoration);
            if (output.size() < 21)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
            if (decoration.kind != NOTEPADS_MARKDOWN_PREVIEW_DECORATION_ORDERED)
            {
                output[0] = L'\x2022';
                return 1;
            }
            std::array<wchar_t, 20> digits{};
            auto at = digits.size();
            auto ordinal = decoration.ordinal;
            do
            {
                digits[--at] = static_cast<wchar_t>(L'0' + ordinal % 10);
                ordinal /= 10;
            } while (ordinal);
            auto length = digits.size() - at;
            std::copy(digits.begin() + at, digits.end(), output.begin());
            output[length++] = L'.';
            return length;
        }
        winrt::com_ptr<IDWriteTextFormat> MarkerFormat(IDWriteFactory *factory, PreviewTheme const &theme)
        {
            winrt::com_ptr<IDWriteTextFormat> format;
            winrt::check_hresult(factory->CreateTextFormat(theme.fontFamily.c_str(), nullptr,
                DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL,
                theme.fontSize, L"", format.put()));
            winrt::check_hresult(format->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP));
            winrt::check_hresult(format->SetReadingDirection(DWRITE_READING_DIRECTION_LEFT_TO_RIGHT));
            winrt::check_hresult(format->SetFlowDirection(DWRITE_FLOW_DIRECTION_TOP_TO_BOTTOM));
            winrt::check_hresult(format->SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM,
                                                       theme.lineHeight, theme.fontSize));
            return format;
        }
        struct MarkerGroup
        {
            static constexpr uint32_t Maximum = 256;
            std::array<wchar_t, Maximum * 22> text{};
            uint32_t length{}, count{};
            void Append(notepads_markdown_preview_decoration const &decoration)
            {
                if (count >= Maximum)
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                if (count)
                    text[length++] = L'\n';
                length += static_cast<uint32_t>(MarkerText(decoration, std::span{text}.subspan(length)));
                ++count;
            }
        };
        MarkerWidths MeasureMarkerGroup(IDWriteFactory *factory, IDWriteTextFormat *format,
                                         PreviewTheme const &theme, MarkerGroup const &group,
                                         std::shared_ptr<NativeMeasurementCache> const &cache,
                                         ListMarkerProbeStatistics &statistics)
        {
            ++statistics.batches;
            constexpr uint32_t keyMagic = 0x314B4D4Cu, valueMagic = 0x31564D4Cu;
            NativeMeasurementCache::Workspace workspace;
            uint32_t keyBytes{};
            uint64_t epoch{};
            if (cache)
            {
                workspace = cache->TryWorkspace();
                if (workspace)
                {
                    MetricWriter writer{workspace.Key()};
                    writer.Word(keyMagic);
                    writer.Word(1); // Complete marker shaping/measurement policy.
                    writer.Float(theme.fontSize);
                    writer.Float(theme.lineHeight);
                    writer.Word(static_cast<uint32_t>(theme.fontFamily.size()));
                    writer.Word(group.length);
                    writer.Word(group.count);
                    writer.Word(DWRITE_FONT_WEIGHT_NORMAL);
                    writer.Word(DWRITE_FONT_STYLE_NORMAL);
                    writer.Word(DWRITE_FONT_STRETCH_NORMAL);
                    writer.Word(DWRITE_WORD_WRAPPING_NO_WRAP);
                    writer.Word(DWRITE_READING_DIRECTION_LEFT_TO_RIGHT);
                    writer.Word(DWRITE_FLOW_DIRECTION_TOP_TO_BOTTOM);
                    writer.Word(DWRITE_TEXT_ALIGNMENT_TRAILING);
                    writer.Word(DWRITE_LINE_SPACING_METHOD_UNIFORM);
                    writer.Word(0); // Actual locale is empty; no trimming on the format.
                    writer.Text(theme.fontFamily);
                    writer.Text({group.text.data(), group.length});
                    if (writer.valid)
                    {
                        keyBytes = static_cast<uint32_t>(writer.size);
                        auto expectedEpoch = cache->Stats().epoch;
                        auto found = workspace.Lookup(keyBytes, expectedEpoch);
                        epoch = found.epoch == expectedEpoch ? expectedEpoch : 0;
                        if (found.disposition == NOTEPADS_MARKDOWN_METRIC_CACHE_HIT && found.copied_bytes == 32)
                        {
                            auto bytes = workspace.Payload();
                            auto word = [&](size_t offset)
                            {
                                uint32_t value{};
                                for (uint32_t i = 0; i < 4; ++i)
                                    value |= uint32_t(bytes[offset + i]) << (8 * i);
                                return value;
                            };
                            MarkerWidths result{std::bit_cast<float>(word(16)), std::bit_cast<float>(word(20)),
                                                std::bit_cast<float>(word(24))};
                            if (word(0) == valueMagic && word(4) == 1 && word(8) == 32 && word(12) == group.count &&
                                word(28) == 0 && std::isfinite(result.advance) && result.advance > 0 &&
                                std::isfinite(result.left) && result.left >= 0 &&
                                std::isfinite(result.right) && result.right >= 0)
                            {
                                ++statistics.cacheHits;
                                return result;
                            }
                        }
                    }
                }
            }
            ++statistics.layouts;
            winrt::com_ptr<IDWriteTextLayout> layout;
            winrt::check_hresult(format->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING));
            winrt::check_hresult(factory->CreateTextLayout(group.text.data(), group.length, format,
                std::numeric_limits<float>::max() / 4, theme.lineHeight * (group.count + 1), layout.put()));
            DWRITE_TEXT_METRICS metrics{};
            winrt::check_hresult(layout->GetMetrics(&metrics));
            if (metrics.lineCount != group.count || !std::isfinite(metrics.widthIncludingTrailingWhitespace) ||
                metrics.widthIncludingTrailingWhitespace <= 0)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            auto advance = RoundMarkerWidth(metrics.widthIncludingTrailingWhitespace);
            winrt::check_hresult(layout->SetMaxWidth(advance));
            winrt::check_hresult(layout->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING));
            DWRITE_OVERHANG_METRICS overhang{};
            winrt::check_hresult(layout->GetOverhangMetrics(&overhang));
            if (!std::isfinite(overhang.left) || !std::isfinite(overhang.right))
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            MarkerWidths result{advance, RoundMarkerWidth(std::max(0.0f, overhang.left)),
                                RoundMarkerWidth(std::max(0.0f, overhang.right))};
            if (workspace && keyBytes && epoch)
            {
                MetricWriter writer{workspace.Payload()};
                writer.Word(valueMagic);
                writer.Word(1);
                writer.Word(32);
                writer.Word(group.count);
                writer.Float(result.advance);
                writer.Float(result.left);
                writer.Float(result.right);
                writer.Word(0);
                if (writer.valid)
                    workspace.Put(keyBytes, static_cast<uint32_t>(writer.size), epoch);
            }
            return result;
        }
        MarkerWidths ProbeListMarkers(notepads_markdown_core_model *model, PreviewCancellation const &cancel,
                                       notepads_markdown_preview_key const &key, PreviewTheme const &theme,
                                       ReadingSourceState &reading, std::shared_ptr<NativeMeasurementCache> const &cache,
                                       ListMarkerProbeStatistics &statistics)
        {
            if (reading.listMarkerMetricsValid &&
                std::wstring_view{reading.listMarkerFont.data(), reading.listMarkerFont.size()} == theme.fontFamily &&
                reading.listMarkerFontSize == theme.fontSize && reading.listMarkerLineHeight == theme.lineHeight)
                return {reading.listMarkerAdvance, reading.listMarkerLeft, reading.listMarkerRight};
            struct Probe
            {
                notepads_markdown_preview_layout *value{};
                ~Probe()
                {
                    (void)notepads_markdown_preview_layout_destroy(value);
                }
            } probe;
            auto error = CorePod<notepads_markdown_core_error>();
            Check(notepads_markdown_preview_layout_create(model, cancel.Get(), &key, nullptr, 0,
                      NOTEPADS_MARKDOWN_PREVIEW_LAYOUT_MARKERS, 0, &probe.value, &error), L"marker probe create");
            auto factory = SystemTextFactory();
            auto format = MarkerFormat(factory, theme);
            MarkerWidths widths;
            MarkerGroup group;
            auto flush = [&]
            {
                if (group.count)
                {
                    widths.Include(MeasureMarkerGroup(factory, format.get(), theme, group, cache, statistics));
                    group.length = group.count = 0;
                }
            };
            for (;;)
            {
                auto summary = PreviewPod<notepads_markdown_preview_summary>();
                Check(notepads_markdown_preview_measure_begin(probe.value, 32, &summary, &error), L"marker probe begin");
                if (std::memcmp(&summary.key, &key, sizeof(key)) != 0 || summary.blocks > 32)
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                if (!summary.blocks)
                    break;
                std::array<notepads_markdown_preview_block, 32> blocks{};
                auto read = CorePod<notepads_markdown_core_read_result>();
                Check(notepads_markdown_preview_read(probe.value, NOTEPADS_MARKDOWN_PREVIEW_BLOCKS, 0,
                    static_cast<uint32_t>(summary.blocks), sizeof(blocks[0]), blocks.data(), &read, &error), L"marker probe blocks");
                if (read.written != summary.blocks || read.next != summary.blocks || !read.done)
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                std::array<notepads_markdown_preview_decoration, 256> rows{};
                for (uint64_t start = 0; start < summary.decorations;)
                {
                    auto count = static_cast<uint32_t>(std::min<uint64_t>(rows.size(), summary.decorations - start));
                    read = CorePod<notepads_markdown_core_read_result>();
                    Check(notepads_markdown_preview_read(probe.value, NOTEPADS_MARKDOWN_PREVIEW_DECORATIONS, start,
                        count, sizeof(rows[0]), rows.data(), &read, &error), L"marker probe decorations");
                    if (read.written != count || read.next != start + count || read.done != (start + count == summary.decorations))
                        throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                    for (uint32_t i = 0; i < count; ++i)
                    {
                        ValidatePreviewDecoration(rows[i]);
                        if (!IsPreviewTaskDecoration(rows[i].kind))
                        {
                            group.Append(rows[i]);
                            if (group.count == MarkerGroup::Maximum)
                                flush();
                        }
                    }
                    start += count;
                }
                std::array<notepads_markdown_preview_metric, 32> metrics{};
                for (size_t i = 0; i < summary.blocks; ++i)
                    metrics[i].node = blocks[i].node;
                Check(notepads_markdown_preview_submit(probe.value, &key, summary.start, metrics.data(),
                    static_cast<uint32_t>(summary.blocks), &error), L"marker probe submit");
            }
            flush();
            // Reborrow the completed draft to checkpoint cancellation after the last DWrite group.
            auto summary = PreviewPod<notepads_markdown_preview_summary>();
            Check(notepads_markdown_preview_measure_begin(probe.value, 32, &summary, &error), L"marker probe complete");
            reading.listMarkerFont.assign(theme.fontFamily.data(), theme.fontFamily.size());
            reading.listMarkerFontSize = theme.fontSize;
            reading.listMarkerLineHeight = theme.lineHeight;
            reading.listMarkerAdvance = widths.advance;
            reading.listMarkerLeft = widths.left;
            reading.listMarkerRight = widths.right;
            reading.listMarkerMetricsValid = true;
            return widths;
        }
        uint32_t CaptureTextMetrics(IDWriteTextLayout *layout, std::span<uint8_t> scratch, uint32_t length,
                                    float height, float width, std::span<DWRITE_LINE_METRICS const> adjustedLines = {},
                                    float overflowAbove = 0, float overflowBelow = 0)
        {
            UINT32 clusters{}, nativeLines{};
            auto status = layout->GetClusterMetrics(nullptr, 0, &clusters);
            if (FAILED(status) && status != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                winrt::check_hresult(status);
            status = layout->GetLineMetrics(nullptr, 0, &nativeLines);
            if (FAILED(status) && status != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                winrt::check_hresult(status);
            if (adjustedLines.size() > nativeLines)
                return 0;
            auto lines = adjustedLines.empty() ? nativeLines : static_cast<uint32_t>(adjustedLines.size());
            uint64_t bytes = uint64_t(TextMetricView::HeaderBytes) + uint64_t(clusters) * 4 + uint64_t(lines) * 12;
            uint64_t temporary = (bytes + 15) & ~uint64_t(15);
            auto nativeBytes = std::max(uint64_t(clusters) * sizeof(DWRITE_CLUSTER_METRICS),
                                        uint64_t(nativeLines) * sizeof(DWRITE_LINE_METRICS));
            if (temporary + nativeBytes > scratch.size())
                return 0;
            MetricWriter writer{scratch.first(static_cast<size_t>(bytes))};
            writer.Word(TextMetricView::Magic);
            writer.Word(2);
            writer.Word(static_cast<uint32_t>(bytes));
            writer.Word(length);
            writer.Word(clusters);
            writer.Word(lines);
            writer.Float(height);
            writer.Float(width);
            writer.Float(overflowAbove);
            writer.Float(overflowBelow);
            UINT32 actual{};
            auto clusterRows = reinterpret_cast<DWRITE_CLUSTER_METRICS *>(scratch.data() + temporary);
            if (clusters)
                winrt::check_hresult(layout->GetClusterMetrics(clusterRows, clusters, &actual));
            if (actual != clusters)
                return 0;
            for (uint32_t i = 0; i < clusters; ++i)
                writer.Word(clusterRows[i].length);
            auto lineRows = reinterpret_cast<DWRITE_LINE_METRICS *>(scratch.data() + temporary);
            actual = 0;
            if (nativeLines)
                winrt::check_hresult(layout->GetLineMetrics(lineRows, nativeLines, &actual));
            if (actual != nativeLines)
                return 0;
            if (lines != nativeLines &&
                !(lines + 1 == nativeLines && !lineRows[nativeLines - 1].length && lineRows[lines - 1].newlineLength))
                return 0;
            for (uint32_t i = 0; i < lines; ++i)
            {
                if (!adjustedLines.empty() &&
                    (adjustedLines[i].length != lineRows[i].length ||
                     adjustedLines[i].newlineLength != lineRows[i].newlineLength ||
                     !std::isfinite(adjustedLines[i].height) || adjustedLines[i].height <= 0))
                    return 0;
                writer.Word(lineRows[i].length);
                writer.Word(lineRows[i].newlineLength);
                writer.Float(adjustedLines.empty() ? lineRows[i].height : adjustedLines[i].height);
            }
            return writer.valid && TextMetricView{scratch.first(static_cast<size_t>(bytes))}.Valid(length)
                       ? static_cast<uint32_t>(bytes)
                       : 0;
        }
        struct alignas(MEMORY_ALLOCATION_ALIGNMENT) PreviewPrefix
        {
            ModuleLifetime module;
            notepads_markdown_preview_allocation *lease;
        };
        class ImageInline final : public IDWriteInlineObject
        {
            std::atomic<ULONG> _references{1};
            DWRITE_INLINE_OBJECT_METRICS _metrics;
            bool _display{};
            bool _terminal{};

          public:
            ImageInline(float width, float height, float baseline, bool display = false, bool terminal = false) noexcept
                : _metrics{width, height, baseline, FALSE}, _display(display), _terminal(terminal)
            {
            }
            static void *operator new(size_t bytes, notepads_markdown_preview_layout *owner)
            {
                return AllocatePreview(owner, bytes);
            }
            static void operator delete(void *memory) noexcept
            {
                FreePreview(memory);
            }
            static void operator delete(void *memory, notepads_markdown_preview_layout *) noexcept
            {
                FreePreview(memory);
            }
            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = nullptr;
                if (iid == __uuidof(IUnknown) || iid == __uuidof(IDWriteInlineObject))
                {
                    *output = static_cast<IDWriteInlineObject *>(this);
                    AddRef();
                    return S_OK;
                }
                return E_NOINTERFACE;
            }
            ULONG STDMETHODCALLTYPE AddRef() noexcept override
            {
                return ++_references;
            }
            ULONG STDMETHODCALLTYPE Release() noexcept override
            {
                auto count = --_references;
                if (!count)
                    delete this;
                return count;
            }
            HRESULT STDMETHODCALLTYPE Draw(void *, IDWriteTextRenderer *, FLOAT, FLOAT, BOOL, BOOL,
                                           IUnknown *) noexcept override
            {
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetMetrics(DWRITE_INLINE_OBJECT_METRICS *output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = _metrics;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetOverhangMetrics(DWRITE_OVERHANG_METRICS *output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = {};
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetBreakConditions(DWRITE_BREAK_CONDITION *before,
                                                         DWRITE_BREAK_CONDITION *after) noexcept override
            {
                if (!before || !after)
                    return E_POINTER;
                *before = _display ? DWRITE_BREAK_CONDITION_MUST_BREAK : DWRITE_BREAK_CONDITION_NEUTRAL;
                *after = _display && !_terminal ? DWRITE_BREAK_CONDITION_MUST_BREAK : DWRITE_BREAK_CONDITION_NEUTRAL;
                return S_OK;
            }
        };
        D2D1_COLOR_F Color(winrt::Windows::UI::Color value)
        {
            return D2D1::ColorF(value.R / 255.0f, value.G / 255.0f, value.B / 255.0f, value.A / 255.0f);
        }

        D2D1_COLOR_F CalloutBorder(uint32_t kind, PreviewTheme const &theme) noexcept
        {
            if (theme.highContrast)
                return Color(theme.foreground);
            const bool dark = 0.2126f * theme.background.R + 0.7152f * theme.background.G +
                                  0.0722f * theme.background.B < 128.0f;
            switch (kind)
            {
            case NOTEPADS_MARKDOWN_CALLOUT_NOTE:
            case NOTEPADS_MARKDOWN_CALLOUT_ABSTRACT:
            case NOTEPADS_MARKDOWN_CALLOUT_INFO:
            case NOTEPADS_MARKDOWN_CALLOUT_TODO: return D2D1::ColorF(dark ? 0x66B2FF : 0x2368BF);
            case NOTEPADS_MARKDOWN_CALLOUT_TIP:
            case NOTEPADS_MARKDOWN_CALLOUT_SUCCESS: return D2D1::ColorF(dark ? 0x79C98A : 0x2E7D43);
            case NOTEPADS_MARKDOWN_CALLOUT_IMPORTANT:
            case NOTEPADS_MARKDOWN_CALLOUT_EXAMPLE: return D2D1::ColorF(dark ? 0xBB9AEF : 0x7551A3);
            case NOTEPADS_MARKDOWN_CALLOUT_QUESTION: return D2D1::ColorF(dark ? 0x76CBD0 : 0x197D83);
            case NOTEPADS_MARKDOWN_CALLOUT_WARNING: return D2D1::ColorF(dark ? 0xF2C774 : 0x9B610F);
            case NOTEPADS_MARKDOWN_CALLOUT_CAUTION:
            case NOTEPADS_MARKDOWN_CALLOUT_FAILURE:
            case NOTEPADS_MARKDOWN_CALLOUT_DANGER:
            case NOTEPADS_MARKDOWN_CALLOUT_BUG: return D2D1::ColorF(dark ? 0xF19899 : 0xAD4146);
            default: return Color(theme.borderColor);
            }
        }

        D2D1_COLOR_F ResourceNoticeColor(PreviewTheme const &theme, bool failed) noexcept
        {
            const auto panel = Color(theme.panelBackground);
            const auto background = Color(theme.background);
            const auto lightness = [](D2D1_COLOR_F color)
            {
                return 0.2126f * color.r + 0.7152f * color.g + 0.0722f * color.b;
            };
            const auto darkPanel = std::lerp(lightness(background), lightness(panel), panel.a) < 0.5f;
            return D2D1::ColorF(failed ? (darkPanel ? 0xFF9A94 : 0xA42621)
                                      : (darkPanel ? 0xF6C878 : 0x805400));
        }

        D2D1_COLOR_F HighlightBackground(PreviewTheme const &theme) noexcept
        {
            if (theme.highContrast)
            {
                const auto background = Color(theme.background);
                const auto foreground = Color(theme.foreground);
                return D2D1::ColorF(std::lerp(background.r, foreground.r, 0.2f),
                                    std::lerp(background.g, foreground.g, 0.2f),
                                    std::lerp(background.b, foreground.b, 0.2f));
            }
            const auto foreground = Color(theme.foreground);
            const auto lightText = 0.2126f * foreground.r + 0.7152f * foreground.g + 0.0722f * foreground.b > 0.5f;
            return D2D1::ColorF(lightText ? 0x513F16 : 0xFFF0AC, 0.8f);
        }

        bool DisplayMath(uint32_t syntax) noexcept
        {
            return syntax == NOTEPADS_MARKDOWN_MATH_DOLLAR_DISPLAY || syntax == NOTEPADS_MARKDOWN_MATH_LATEX_DISPLAY ||
                   syntax == NOTEPADS_MARKDOWN_MATH_FENCED_DISPLAY;
        }

        NativeMathState MathAttempt(PreviewMathSnapshot const &math, uint32_t ordinal) noexcept
        {
            if (!math.enabled)
                return NativeMathState::Unavailable;
            if (ordinal >= math.attempts.size())
                return NativeMathState::Limited;
            auto state = math.attempts[ordinal];
            // A Ready attempt without its immutable artifact cannot be drawn.
            if (state == NativeMathState::Ready)
                return NativeMathState::Unavailable;
            return state == NativeMathState::None ? NativeMathState::Pending : state;
        }

        std::wstring_view MathLabel(NativeMathState state) noexcept
        {
            switch (state)
            {
            case NativeMathState::Pending:
            case NativeMathState::None:
                return L"Math pending";
            case NativeMathState::Limited:
                return L"Math limited";
            case NativeMathState::Unsupported:
                return L"Math unsupported";
            case NativeMathState::Failed:
                return L"Math failed";
            case NativeMathState::Cancelled:
                return L"Math cancelled";
            default:
                return L"Math unavailable";
            }
        }

        NativeMathState MathFailureState(int32_t status) noexcept
        {
            if (status == E_OUTOFMEMORY || status == HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA))
                return NativeMathState::Limited;
            return NativeMathPresentation::Classify(status);
        }

        NativeDiagramState DiagramAttempt(PreviewDiagramSnapshot const &snapshot, uint32_t ordinal) noexcept
        {
            if (!snapshot.enabled)
                return NativeDiagramState::Unavailable;
            if (ordinal >= snapshot.attempts.size())
                return NativeDiagramState::Limited;
            auto state = snapshot.attempts[ordinal];
            if (state == NativeDiagramState::Ready)
                return NativeDiagramState::Unavailable;
            return state == NativeDiagramState::None ? NativeDiagramState::Pending : state;
        }

        std::wstring_view DiagramLabel(NativeDiagramState state) noexcept
        {
            switch (state)
            {
            case NativeDiagramState::None:
            case NativeDiagramState::Pending:
                return L"Diagram pending";
            case NativeDiagramState::Limited:
                return L"Diagram limited";
            case NativeDiagramState::Unsupported:
                return L"Diagram unsupported";
            case NativeDiagramState::Failed:
                return L"Diagram failed";
            case NativeDiagramState::Cancelled:
                return L"Diagram cancelled";
            case NativeDiagramState::FontUnavailable:
                return L"Diagram font unavailable";
            default:
                return L"Diagram unavailable";
            }
        }

        NativeDiagramState DiagramFailureState(int32_t status) noexcept
        {
            if (status == E_OUTOFMEMORY || status == HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA))
                return NativeDiagramState::Limited;
            if (status == E_NOINTERFACE)
                return NativeDiagramState::Unavailable;
            return NativeDiagramPresentation::Classify(status);
        }

        struct CodeHighlight
        {
            notepads_markdown_highlight_block const *row{};
            std::span<notepads_markdown_highlight_span const> spans;
        };

        bool MatchesCodeText(MainTextSnapshot const &main, notepads_markdown_preview_block const &block,
                             std::span<PreviewTextVisual const> texts, notepads_markdown_highlight_block const &row) noexcept
        {
            if (row.logical_start != block.logical_start || row.logical_end != block.logical_end ||
                row.logical_start > row.logical_end || row.logical_end > main.Length() ||
                !main.IsBoundary(static_cast<uint32_t>(row.logical_start)) ||
                !main.IsBoundary(static_cast<uint32_t>(row.logical_end)))
                return false;
            auto position = row.logical_start;
            for (auto const &text : texts)
            {
                if (!text.matchesMainText || text.logicalStart != position || text.logicalEnd < position ||
                    text.logicalEnd > row.logical_end || text.logicalEnd - position != text.utf16Length ||
                    !main.IsBoundary(static_cast<uint32_t>(text.logicalEnd)))
                    return false;
                position = text.logicalEnd;
            }
            return position == row.logical_end;
        }

        CodeHighlight CodeHighlights(std::shared_ptr<NativeHighlightSnapshot const> const &snapshot, int32_t status,
                                     std::shared_ptr<MainTextSnapshot const> const &main, notepads_markdown_preview_block const &block,
                                     std::span<PreviewTextVisual const> texts)
        {
            if (block.kind != 3 || status != NOTEPADS_MARKDOWN_CORE_OK || !snapshot || !main)
                return {};
            auto const row = snapshot->Find(block.node);
            if (!row || row->state != NOTEPADS_MARKDOWN_HIGHLIGHT_COMPLETE ||
                !MatchesCodeText(*main, block, texts, *row))
                return {};
            auto const spans = snapshot->Spans(*row);
            uint64_t position{};
            auto const length = row->logical_end - row->logical_start;
            // Validate the whole partition before applying any color. The
            // layout text was checked against Main during measurement; these
            // local UTF16 boundaries must also be actual scalar boundaries.
            for (auto const &span : spans)
            {
                if (span.start_utf16 != position || !span.length_utf16 || span.length_utf16 > length - position ||
                    span.role >= NOTEPADS_MARKDOWN_HIGHLIGHT_ROLE_COUNT || span.reserved ||
                    !main->IsBoundary(static_cast<uint32_t>(row->logical_start + position)) ||
                    !main->IsBoundary(static_cast<uint32_t>(row->logical_start + position + span.length_utf16)))
                    return {};
                position += span.length_utf16;
            }
            return position == length ? CodeHighlight{row, spans} : CodeHighlight{};
        }

    }

    void *AllocatePreview(notepads_markdown_preview_layout *layout, size_t bytes)
    {
        if (!layout || bytes > SIZE_MAX - sizeof(PreviewPrefix))
            throw std::bad_alloc();
        auto error = CorePod<notepads_markdown_core_error>();
        notepads_markdown_preview_allocation *lease{};
        auto total = bytes + sizeof(PreviewPrefix);
        Check(notepads_markdown_preview_reserve(layout, total, &lease, &error), L"native allocation reserve");
        auto memory = ::HeapAlloc(::GetProcessHeap(), 0, total);
        if (!memory)
        {
            (void)notepads_markdown_preview_allocation_destroy(lease);
            throw std::bad_alloc();
        }
        auto prefix = ::new (memory) PreviewPrefix{{}, lease};
        auto actual = ::HeapSize(::GetProcessHeap(), 0, memory);
        if (actual == static_cast<SIZE_T>(-1))
        {
            FreePreview(prefix + 1);
            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
        }
        auto status = notepads_markdown_preview_admit(layout, lease, actual, &error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
        {
            FreePreview(prefix + 1);
            Check(status);
        }
        return prefix + 1;
    }
    void FreePreview(void *memory) noexcept
    {
        if (!memory)
            return;
        ModuleLifetime pin;
        auto prefix = static_cast<PreviewPrefix *>(memory) - 1;
        auto lease = prefix->lease;
        prefix->~PreviewPrefix();
        if (::HeapFree(::GetProcessHeap(), 0, prefix))
        {
            (void)notepads_markdown_preview_allocation_destroy(lease);
        }
        else
        {
            // Keep the reservation alive if Windows did not free the bytes.
            ::new (prefix) PreviewPrefix{{}, lease};
        }
    }

    PreviewCancellation::PreviewCancellation(notepads_markdown_core_model *model)
    {
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_core_model_cancel_create(model, &_cancel, &error));
    }
    PreviewCancellation::~PreviewCancellation()
    {
        if (_cancel)
            (void)notepads_markdown_core_cancel_destroy(_cancel);
    }
    void PreviewCancellation::Cancel() noexcept
    {
        if (_cancel)
            (void)notepads_markdown_core_cancel_request(_cancel);
    }

    std::shared_ptr<ReadingSourceState> ReadingSourceState::Create(CoreModelPin model, uint64_t sourceGeneration,
                                                                    bool deferHighlights)
    {
        auto summary = CorePod<notepads_markdown_core_model_summary_result>();
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_core_model_summary(model.Get(), &summary, &error));
        auto cancel = std::make_shared<PreviewCancellation>(model.Get());
        auto memory = ReadingMemory::Create(std::move(model), cancel->Get(), summary.identity);
        return std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory}, memory,
                                                        std::move(cancel), sourceGeneration, NextReadingTextToken(), deferHighlights);
    }
    void ReadingSourceState::EnsureMain()
    {
        if (fault != NOTEPADS_MARKDOWN_CORE_OK)
            throw ReadingFailure{fault};
        if (Main())
            return;
        try
        {
            PublishMain(MainTextSnapshot::Build(memory, cancel->Get(), sourceGeneration, textGeneration));
        }
        catch (ReadingFailure const &error)
        {
            fault = error.status;
            throw;
        }
        catch (std::bad_alloc const &)
        {
            fault = NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
            throw;
        }
        catch (winrt::hresult_error const &error)
        {
            fault = error.code().value;
            throw;
        }
        catch (...)
        {
            fault = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
            throw;
        }
    }

    struct NativeDocumentRenderer::Presentation
    {
        notepads_markdown_preview_layout *owner;
        PreviewVector<notepads_markdown_preview_block> blocks;
        PreviewVector<notepads_markdown_preview_block_geometry> geometry;
        PreviewVector<notepads_markdown_preview_text> texts;
        PreviewVector<notepads_markdown_preview_run> runs;
        PreviewVector<notepads_markdown_preview_script_scope> scriptScopes;
        PreviewVector<notepads_markdown_preview_ruby_unit> rubyUnits;
        PreviewVector<notepads_markdown_preview_cell> cells;
        PreviewVector<notepads_markdown_preview_html_cell> htmlCells;
        PreviewVector<notepads_markdown_preview_decoration> decorations;
        PreviewVector<notepads_markdown_preview_image> images;
        PreviewVector<notepads_markdown_preview_link> links;
        PreviewVector<notepads_markdown_derived_row> derived;
        PreviewVector<uint8_t> blob;
        explicit Presentation(notepads_markdown_preview_layout *layout)
            : owner(layout), blocks(PreviewAllocator<notepads_markdown_preview_block>{layout}),
              geometry(PreviewAllocator<notepads_markdown_preview_block_geometry>{layout}),
              texts(PreviewAllocator<notepads_markdown_preview_text>{layout}), runs(PreviewAllocator<notepads_markdown_preview_run>{layout}),
              scriptScopes(PreviewAllocator<notepads_markdown_preview_script_scope>{layout}),
              rubyUnits(PreviewAllocator<notepads_markdown_preview_ruby_unit>{layout}),
              cells(PreviewAllocator<notepads_markdown_preview_cell>{layout}),
              htmlCells(PreviewAllocator<notepads_markdown_preview_html_cell>{layout}),
              decorations(PreviewAllocator<notepads_markdown_preview_decoration>{layout}),
              images(PreviewAllocator<notepads_markdown_preview_image>{layout}), links(PreviewAllocator<notepads_markdown_preview_link>{layout}),
              derived(PreviewAllocator<notepads_markdown_derived_row>{layout}), blob(PreviewAllocator<uint8_t>{layout})
        {
        }

        PreviewString Text(notepads_markdown_preview_blob slice) const
        {
            if (slice.start > blob.size() || slice.length > blob.size() - slice.start || slice.length > INT_MAX)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            PreviewString result{PreviewAllocator<wchar_t>{owner}};
            if (!slice.length)
                return result;
            auto input = reinterpret_cast<char const *>(blob.data() + slice.start);
            auto count =
                ::MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, input, static_cast<int>(slice.length), nullptr, 0);
            if (!count)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_UTF8};
            result.resize(count);
            if (!::MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, input, static_cast<int>(slice.length),
                                       result.data(), count))
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_UTF8};
            return result;
        }

        PreviewString ImageCaption(notepads_markdown_preview_blob slice) const
        {
            if (slice.start > blob.size() || slice.length > blob.size() - slice.start)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            constexpr size_t maximum = 192;
            auto bytes = std::min<uint64_t>(slice.length, maximum * 4);
            // Slice only on a UTF-8 scalar boundary before the bounded decode.
            while (bytes < slice.length && (blob[static_cast<size_t>(slice.start + bytes)] & 0xc0) == 0x80)
                --bytes;
            auto caption = Text({slice.start, bytes});
            if (bytes < slice.length || caption.size() > maximum)
            {
                auto count = std::min(caption.size(), maximum - 1);
                if (count && caption[count - 1] >= 0xd800 && caption[count - 1] <= 0xdbff)
                    --count;
                caption.resize(count);
                caption.push_back(L'\x2026');
            }
            return caption;
        }
    };

    NativeDocumentRenderer::Presentation NativeDocumentRenderer::Read(notepads_markdown_preview_summary const &summary,
                                                                       notepads_markdown_preview_layout *layout)
    {
        layout = layout ? layout : _layout;
        Presentation result{layout};
        if (std::memcmp(&summary.key, &_key, sizeof(_key)) != 0)
            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        auto read = [&](auto &rows, uint64_t count, uint32_t table)
        {
            using T = typename std::remove_reference_t<decltype(rows)>::value_type;
            if (count > rows.max_size())
                throw std::bad_alloc();
            rows.resize(static_cast<size_t>(count));
            uint64_t offset{};
            while (offset < count)
            {
                auto chunk = std::min<uint64_t>(count - offset, table == NOTEPADS_MARKDOWN_PREVIEW_BLOB ? 65536 : 256);
                auto output = CorePod<notepads_markdown_core_read_result>();
                auto error = CorePod<notepads_markdown_core_error>();
                Check(notepads_markdown_preview_read(layout, table, offset, chunk, sizeof(T), rows.data() + offset, &output, &error));
                if (output.written != chunk || output.next != offset + chunk)
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                offset += chunk;
            }
        };
        read(result.blocks, summary.blocks, NOTEPADS_MARKDOWN_PREVIEW_BLOCKS);
        read(result.geometry, summary.blocks, NOTEPADS_MARKDOWN_PREVIEW_BLOCK_GEOMETRY);
        for (size_t index = 0; index < result.blocks.size(); ++index)
        {
            auto const &block = result.blocks[index];
            auto const &geometry = result.geometry[index];
            if (geometry.node != block.node || geometry.reserved != 0 || !std::isfinite(geometry.marker_x) ||
                geometry.marker_x < 0 || !std::isfinite(block.x) || geometry.marker_x > block.x)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        }
        read(result.texts, summary.texts, NOTEPADS_MARKDOWN_PREVIEW_TEXTS);
        read(result.runs, summary.runs, NOTEPADS_MARKDOWN_PREVIEW_RUNS);
        read(result.cells, summary.cells, NOTEPADS_MARKDOWN_PREVIEW_CELLS);
        read(result.htmlCells, summary.cells, NOTEPADS_MARKDOWN_PREVIEW_HTML_CELLS);
        for (size_t index = 0; index < result.cells.size(); ++index)
            ValidatePreviewHtmlCell(result.cells[index], result.htmlCells[index]);
        read(result.decorations, summary.decorations, NOTEPADS_MARKDOWN_PREVIEW_DECORATIONS);
        read(result.images, summary.images, NOTEPADS_MARKDOWN_PREVIEW_IMAGES);
        read(result.links, summary.links, NOTEPADS_MARKDOWN_PREVIEW_LINKS);
        notepads_markdown_preview_script_summary_result scripts{};
        scripts.header = {NOTEPADS_MARKDOWN_PREVIEW_SCRIPT_ABI_VERSION, sizeof(scripts)};
        auto scriptError = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_preview_script_summary(layout, &scripts, &scriptError));
        if (scripts.start != summary.start || std::memcmp(&scripts.key, &summary.key, sizeof(summary.key)) != 0)
            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        read(result.scriptScopes, scripts.rows, NOTEPADS_MARKDOWN_PREVIEW_SCRIPT_SCOPES);
        if (summary.ruby_units > 1024)
            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        read(result.rubyUnits, summary.ruby_units, NOTEPADS_MARKDOWN_PREVIEW_RUBY_UNITS);
        for (size_t i = 0; i < result.rubyUnits.size(); ++i)
        {
            auto const &unit = result.rubyUnits[i];
            if (unit.text >= result.texts.size() || unit.start >= unit.base_end || unit.base_end >= unit.end ||
                unit.end > result.texts[unit.text].utf16_length)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            if (i)
            {
                auto const &prior = result.rubyUnits[i - 1];
                if (unit.text < prior.text || (unit.text == prior.text && unit.start < prior.end))
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            }
        }
        auto validScript = [](auto const &value)
        {
            return std::isfinite(value.size_scale) && value.size_scale > 0 && value.size_scale <= 1 &&
                   std::isfinite(value.relative_em) && std::isfinite(value.absolute_dip);
        };
        for (auto const &run : result.runs)
            if (!validScript(run))
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        for (size_t index = 0; index < result.scriptScopes.size(); ++index)
        {
            auto const &scope = result.scriptScopes[index];
            if (!validScript(scope) || (scope.font_flags & ~NOTEPADS_MARKDOWN_PREVIEW_SCRIPT_SCOPE_MASK) || scope.text >= result.texts.size() ||
                scope.start > scope.end || scope.end > result.texts[scope.text].utf16_length)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            if (index)
            {
                auto const &prior = result.scriptScopes[index - 1];
                if (scope.text < prior.text ||
                    (scope.text == prior.text && (scope.start < prior.start ||
                     (scope.start == prior.start && scope.end < prior.end))))
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            }
        }
        notepads_markdown_derived_summary_result derived{};
        derived.header = {NOTEPADS_MARKDOWN_DERIVED_ABI_VERSION, sizeof(derived)};
        auto derivedError = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_derived_summary(layout, &derived, &derivedError));
        if (derived.start != summary.start || derived.blob_bytes != summary.blob_bytes ||
            std::memcmp(&derived.key, &summary.key, sizeof(summary.key)) != 0)
            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        if (derived.rows > result.derived.max_size())
            throw std::bad_alloc();
        result.derived.resize(static_cast<size_t>(derived.rows));
        for (uint64_t offset = 0; offset < derived.rows;)
        {
            auto count = std::min<uint64_t>(256, derived.rows - offset);
            auto output = CorePod<notepads_markdown_core_read_result>();
            Check(notepads_markdown_derived_read(layout, offset, count, sizeof(notepads_markdown_derived_row), result.derived.data() + offset,
                                  &output, &derivedError));
            if (output.written != count || output.next != offset + count)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
            offset += count;
        }
        for (size_t index = 0; index < result.derived.size(); ++index)
        {
            auto const &row = result.derived[index];
            auto validMetadata = [&](notepads_markdown_preview_blob slice)
            {
                if (!slice.length)
                    return slice.start == 0;
                return row.kind == NOTEPADS_MARKDOWN_DERIVED_DIAGRAM && slice.start >= row.input.start &&
                       slice.start - row.input.start <= row.input.length &&
                       slice.length <= row.input.length - (slice.start - row.input.start);
            };
            if (!validMetadata(row.accessible_title) || !validMetadata(row.accessible_description) || row.text >= result.texts.size() || row.offset_utf16 >= result.texts[row.text].utf16_length ||
                (row.kind == NOTEPADS_MARKDOWN_DERIVED_MATH ? row.syntax < NOTEPADS_MARKDOWN_MATH_DOLLAR_INLINE || row.syntax > NOTEPADS_MARKDOWN_MATH_FENCED_DISPLAY
                                             : (row.kind != NOTEPADS_MARKDOWN_DERIVED_DIAGRAM &&
                                                row.kind != NOTEPADS_MARKDOWN_DERIVED_NOTE_EMBED) || row.syntax != 0))
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            if (index)
            {
                auto const &prior = result.derived[index - 1];
                if (row.text < prior.text || (row.text == prior.text && row.offset_utf16 <= prior.offset_utf16))
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            }
        }
        read(result.blob, summary.blob_bytes, NOTEPADS_MARKDOWN_PREVIEW_BLOB);
        return result;
    }

    std::shared_ptr<NativeDocumentRenderer> NativeDocumentRenderer::Create(
        CoreModelPin model, std::shared_ptr<PreviewCancellation> cancel, notepads_markdown_preview_key const &key, PreviewTheme theme,
        std::shared_ptr<ReadingSourceState> reading, std::shared_ptr<ImagePresentationSnapshot const> images,
        PreviewMathSnapshot math, PreviewDiagramSnapshot diagram, std::shared_ptr<NativeMeasurementCache> cache,
        InitialViewport const *initialViewport,
        std::span<notepads_markdown_preview_disclosure_override const> overrides,
        std::wstring_view sectionTitle, std::shared_ptr<PreviewNoteEmbedExtents const> noteEmbeds,
        PreviewEditResources *editResources)
    {
        if (editResources)
            *editResources = {};
        if (!reading)
            reading = ReadingSourceState::Create(std::move(model), key.model.generation);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        StartupTrace::Mark(StartupTrace::Phase::ReadingMain, key.layout_generation);
#endif
        reading->EnsureMain();
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        StartupTrace::Mark(StartupTrace::Phase::ReadingMainReady, key.layout_generation);
#endif
        if (std::memcmp(&reading->Main()->Identity(), &key.model, sizeof(notepads_markdown_core_identity)) != 0)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_STALE_RESULT};
        std::optional<ReadingSection> section;
        if (!sectionTitle.empty())
        {
            auto main = reading->Main();
            auto outline = ReadingOutline::Build(*main, cancel->Get());
            section = outline->Section(sectionTitle, main->Length());
            if (!section)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_UNKNOWN_ID};
        }
        if (!editResources && !initialViewport && !reading->deferHighlights && !reading->highlightingAttempted)
        {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            StartupTrace::Mark(StartupTrace::Phase::Highlights, key.layout_generation);
#endif
            reading->EnsureHighlights();
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            StartupTrace::Mark(StartupTrace::Phase::HighlightsReady, key.layout_generation);
#endif
        }
        if (math.enabled && (std::memcmp(&math.key.identity, &key.model, sizeof(key.model)) != 0 ||
                             math.key.sourceGeneration != reading->sourceGeneration ||
                             math.key.textGeneration != reading->textGeneration))
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_STALE_RESULT};
        if (diagram.enabled && (std::memcmp(&diagram.key.identity, &key.model, sizeof(key.model)) != 0 ||
                                diagram.key.sourceGeneration != reading->sourceGeneration ||
                                diagram.key.textGeneration != reading->textGeneration))
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_STALE_RESULT};
        if (images)
        {
            auto const &snapshot = images->policy.header;
            if (images->textGeneration != reading->textGeneration || !snapshot.has_epoch ||
                snapshot.epoch.source.revision != key.model.revision ||
                std::memcmp(snapshot.epoch.source.document, key.model.document, sizeof(key.model.document)) != 0)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_STALE_RESULT};
            auto fail = [] { throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT}; };
            if (snapshot.count > NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES || images->entries.size() < snapshot.count)
                fail();
            // Windowed policy rows are bounded independently of same-source
            // intrinsic geometry. Extra rows grant no request or pixel authority.
            std::array<std::pair<uint32_t, uint32_t>, NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES> policyRows{};
            for (uint32_t index = 0; index < snapshot.count; ++index)
                policyRows[index] = {images->policy.entries[index].object, index};
            auto policyEnd = policyRows.begin() + snapshot.count;
            std::sort(policyRows.begin(), policyEnd);
            for (uint32_t index = 1; index < snapshot.count; ++index)
                if (policyRows[index - 1].first == policyRows[index].first)
                    fail();
            uint32_t matchedPolicy{};
            for (size_t index = 0; index < images->entries.size(); ++index)
            {
                auto const &entry = images->entries[index];
                auto const &row = entry.state;
                if (index && images->entries[index - 1].state.object >= row.object)
                    fail();
                auto policyRow = std::lower_bound(policyRows.begin(), policyEnd, row.object,
                    [](auto const &item, uint32_t object) { return item.first < object; });
                notepads_markdown_image_entry expected{};
                if (policyRow != policyEnd && policyRow->first == row.object)
                {
                    expected = images->policy.entries[policyRow->second];
                    ++matchedPolicy;
                    if (!(expected.status_flags & NOTEPADS_MARKDOWN_IMAGE_HAS_METADATA) &&
                        expected.state != NOTEPADS_MARKDOWN_IMAGE_READY &&
                        (row.status_flags & NOTEPADS_MARKDOWN_IMAGE_HAS_METADATA))
                    {
                        expected.metadata = row.metadata;
                        expected.status_flags |= NOTEPADS_MARKDOWN_IMAGE_HAS_METADATA;
                    }
                }
                else
                {
                    expected.object = row.object;
                    expected.state = NOTEPADS_MARKDOWN_IMAGE_UNREQUESTED;
                    expected.metadata = row.metadata;
                    expected.status_flags = NOTEPADS_MARKDOWN_IMAGE_HAS_METADATA;
                    if (entry.pixels)
                        fail();
                }
                if (std::memcmp(&row, &expected, sizeof(row)) != 0)
                    fail();
                if (row.status_flags & NOTEPADS_MARKDOWN_IMAGE_HAS_METADATA)
                {
                    auto capacity = uint64_t(row.metadata.stride) * row.metadata.decoded_height;
                    if (!ValidImageMetadata(row.metadata, capacity))
                        fail();
                }
            }
            if (matchedPolicy != snapshot.count)
                fail();
            if (!images->entries.empty())
            {
                auto descriptors = reading->Descriptors();
                if (!descriptors || descriptors->textGeneration != reading->textGeneration ||
                    images->entries.size() > descriptors->objects.size())
                    fail();
                size_t matchedImages{};
                for (auto const &object : descriptors->objects)
                    if (auto entry = images->Find(object.object))
                    {
                        if (object.kind != ReadingObjectKind::Image)
                            fail();
                        ++matchedImages;
                    }
                if (matchedImages != images->entries.size())
                    fail();
            }
        }
        auto error = CorePod<notepads_markdown_core_error>();
        auto modelHandle = model ? model.Get() : reading->memory->ModelOwner().Get();
        ListMarkerProbeStatistics markerStatistics;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        StartupTrace::Mark(StartupTrace::Phase::MarkerProbe, key.layout_generation);
#endif
        auto widths = ProbeListMarkers(modelHandle, *cancel, key, theme, *reading, cache, markerStatistics);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        StartupTrace::Mark(StartupTrace::Phase::MarkerProbeReady, key.layout_generation);
#endif
        auto resolvedKey = key;
        if (widths.advance > 0)
            resolvedKey.indent = std::max(key.indent, static_cast<double>(RoundMarkerWidth(
                static_cast<double>(widths.advance) + widths.left + widths.right + 6.0)));
        auto createRenderer = [&]
        {
            notepads_markdown_preview_layout *layout{};
            if (overrides.size() > UINT32_MAX)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE};
            Check(notepads_markdown_preview_layout_create(modelHandle, cancel->Get(), &resolvedKey,
                      overrides.data(), static_cast<uint32_t>(overrides.size()), NOTEPADS_MARKDOWN_PREVIEW_LAYOUT_VISIBLE,
                      section ? section->heading : 0, &layout, &error), L"layout create");
            try
            {
                auto renderer = std::allocate_shared<NativeDocumentRenderer>(
                    PreviewAllocator<NativeDocumentRenderer>{layout}, std::move(model), cancel, layout, resolvedKey,
                    theme, reading, images, math, diagram, cache);
                renderer->_markerLeftGuard = widths.left;
                renderer->_section = section;
                renderer->_noteEmbeds = noteEmbeds;
                renderer->_markerRightGuard = widths.right;
                renderer->_listMarkerStats = markerStatistics;
                return renderer;
            }
            catch (...)
            {
                (void)notepads_markdown_preview_layout_destroy(layout);
                throw;
            }
        };
        auto result = createRenderer();
        if (editResources)
        {
            // This extra edit-only projection is optional. Its allocation
            // failure must not latch a fault before the ordinary layout runs.
            result->PrepareEditResources(*editResources);
            return result;
        }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        StartupTrace::Mark(StartupTrace::Phase::Measure, key.layout_generation);
#endif
        PrepareReadingSource(reading, [&]
        {
            try
            {
                result->Measure(initialViewport);
                return;
            }
            catch (winrt::hresult_error const &failure)
            {
                auto code = failure.code();
                if (!result->_initialViewportAttempted ||
                    (code != NativeStatusHresult(NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT) && code != E_OUTOFMEMORY))
                    throw;
            }
            catch (std::bad_alloc const &)
            {
                if (!result->_initialViewportAttempted)
                    throw;
            }
            // Discard the failed draft and its queued visual payload before
            // retrying once with the ordinary full-layout path.
            model = std::move(result->_model);
            result.reset();
            result = createRenderer();
            result->Measure(nullptr);
        });
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        StartupTrace::Mark(StartupTrace::Phase::MeasureReady, key.layout_generation);
#endif
        // Standalone renderers and incremental edits retain synchronous
        // highlights. First publication can schedule its colors separately.
        if (initialViewport && !reading->deferHighlights && !reading->highlightingAttempted)
        {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            StartupTrace::Mark(StartupTrace::Phase::Highlights, key.layout_generation);
#endif
            reading->EnsureHighlights();
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            StartupTrace::Mark(StartupTrace::Phase::HighlightsReady, key.layout_generation);
#endif
        }
        return result;
    }
    NativeDocumentRenderer::NativeDocumentRenderer(CoreModelPin model, std::shared_ptr<PreviewCancellation> cancel,
                                                   notepads_markdown_preview_layout *layout, notepads_markdown_preview_key const &key,
                                                   PreviewTheme theme, std::shared_ptr<ReadingSourceState> reading,
                                                   std::shared_ptr<ImagePresentationSnapshot const> images,
                                                   PreviewMathSnapshot math, PreviewDiagramSnapshot diagram,
                                                   std::shared_ptr<NativeMeasurementCache> cache)
        : _model(std::move(model)), _cancel(std::move(cancel)), _layout(layout), _key(key), _theme(std::move(theme)),
          _images(std::move(images)), _math(std::move(math)), _diagram(std::move(diagram)),
          _reading(std::move(reading)), _main(_reading->Main()), _descriptors(_reading->Descriptors()),
          _measurementCache(std::move(cache))
    {
        if (_measurementCache)
            _measurementEpoch = _measurementCache->Stats().epoch;
        winrt::check_hresult(::DWriteCreateFactory(DWRITE_FACTORY_TYPE_ISOLATED, __uuidof(IDWriteFactory),
                                                   reinterpret_cast<IUnknown **>(_factory.put())));
    }
    NativeDocumentRenderer::~NativeDocumentRenderer()
    {
        if (_layout)
            (void)notepads_markdown_preview_layout_destroy(_layout);
    }

    uint32_t NativeDocumentRenderer::MathOrdinal(uint32_t object)
    {
        for (uint32_t index = 0; index < _mathObjectCount; ++index)
        {
            if (_mathObjects[index] == object)
                return index;
        }
        if (!_measuring || _mathObjectCount == _mathObjects.size())
            return static_cast<uint32_t>(_mathObjects.size());
        auto ordinal = _mathObjectCount++;
        _mathObjects[ordinal] = object;
        return ordinal;
    }

    uint32_t NativeDocumentRenderer::DiagramOrdinal(uint32_t object)
    {
        for (uint32_t index = 0; index < _diagramObjectCount; ++index)
        {
            if (_diagramObjects[index] == object)
                return index;
        }
        if (!_measuring || _diagramObjectCount == _diagramObjects.size())
            return static_cast<uint32_t>(_diagramObjects.size());
        auto ordinal = _diagramObjectCount++;
        _diagramObjects[ordinal] = object;
        return ordinal;
    }

    winrt::com_ptr<IDWriteTextLayout> NativeDocumentRenderer::DiagramCaption(
        NativeDiagramState state, IDWriteTextLayout *layout, IDWriteTextFormat *format, uint32_t offset,
        float width, float height, float fontSize)
    {
        auto caption = DiagramLabel(state);
        winrt::com_ptr<IDWriteTextLayout> label;
        winrt::check_hresult(_factory->CreateTextLayout(caption.data(), static_cast<UINT32>(caption.size()), format,
                                                        (std::max)(1.0f, width - 12.0f), (std::max)(1.0f, height),
                                                        label.put()));
        // Capture the actual object's theme/code/script typography. Body
        // alignment may later become CENTER for an atomic display object;
        // caption alignment and spacing belong to this independent layout.
        UINT32 familyLength{};
        winrt::check_hresult(layout->GetFontFamilyNameLength(offset, &familyLength));
        PreviewString family{PreviewAllocator<wchar_t>{_layout}};
        family.resize(static_cast<size_t>(familyLength) + 1);
        winrt::check_hresult(layout->GetFontFamilyName(offset, family.data(), familyLength + 1));
        DWRITE_FONT_WEIGHT weight{};
        DWRITE_FONT_STYLE style{};
        DWRITE_FONT_STRETCH stretch{};
        winrt::check_hresult(layout->GetFontWeight(offset, &weight));
        winrt::check_hresult(layout->GetFontStyle(offset, &style));
        winrt::check_hresult(layout->GetFontStretch(offset, &stretch));
        DWRITE_TEXT_RANGE all{0, static_cast<UINT32>(caption.size())};
        winrt::check_hresult(label->SetFontFamilyName(family.c_str(), all));
        winrt::check_hresult(label->SetFontWeight(weight, all));
        winrt::check_hresult(label->SetFontStyle(style, all));
        winrt::check_hresult(label->SetFontStretch(stretch, all));
        winrt::check_hresult(label->SetFontSize(fontSize, all));
        winrt::check_hresult(label->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP));
        winrt::check_hresult(label->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING));
        winrt::check_hresult(label->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR));
        winrt::check_hresult(label->SetLineSpacing(DWRITE_LINE_SPACING_METHOD_DEFAULT, 0, 0));
        return label;
    }

    void NativeDocumentRenderer::SetDiagramFallback(PreviewDiagramVisual &visual, IDWriteTextLayout *layout,
                                                    IDWriteTextFormat *format, std::wstring_view content, float width,
                                                    float fontSize)
    {
        winrt::check_hresult(layout->SetInlineObject(nullptr, {visual.offset, 1}));
        visual.hasInline = false;
        visual.label = nullptr;
        visual.width = (std::max)(1.0f, width);
        auto label = DiagramCaption(visual.state, layout, format, visual.offset, visual.width, fontSize * 2, fontSize);
        DWRITE_LINE_METRICS line{};
        UINT32 count{};
        winrt::check_hresult(label->GetLineMetrics(&line, 1, &count));
        if (count != 1 || !std::isfinite(line.height) || !std::isfinite(line.baseline) || line.height <= 0)
            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
        visual.height = (std::max)(fontSize, line.height) + 6;
        visual.baseline = line.baseline + 3;
        winrt::check_hresult(label->SetMaxHeight((std::max)(1.0f, visual.height - 6)));
        auto ordinal = DiagramOrdinal(visual.object);
        auto object = ordinal < _diagramInlines.size() ? _diagramInlines[ordinal] : nullptr;
        if (object)
        {
            DWRITE_INLINE_OBJECT_METRICS metrics{};
            if (FAILED(object->GetMetrics(&metrics)) || metrics.width != visual.width ||
                metrics.height != visual.height || metrics.baseline != visual.baseline)
                object = nullptr;
        }
        if (!object)
        {
            if (_measuring && ordinal < _diagramInlines.size())
                _diagramInlines[ordinal] = nullptr;
            object.attach(new (_layout) ImageInline(visual.width, visual.height, visual.baseline, true,
                                                    visual.offset + 1 == content.size()));
        }
        // A missing Node still needs an ink-free replacement for U+FFFC.
        // This mandatory Preview-funded wrapper follows Math fallback's
        // allocation contract: inability to install it fails the Frame.
        winrt::check_hresult(layout->SetInlineObject(object.get(), {visual.offset, 1}));
        visual.hasInline = true;
        // The caption is published only after the mandatory replacement is
        // installed. Failures above propagate rather than reclassifying the
        // visual while leaving a stale caption or ordinary placeholder ink.
        visual.label = std::move(label);
        if (_measuring && ordinal < _diagramInlines.size())
            _diagramInlines[ordinal] = object;
    }

    winrt::com_ptr<IDWriteTextFormat> NativeDocumentRenderer::TextFormat(TextFormatKey const &key)
    {
        for (auto const &entry : _textFormats)
            if (entry.format && entry.key == key)
            {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                ++_textFormatReused;
#endif
                return entry.format;
            }

        auto const &family = key.code ? _theme.codeFontFamily : _theme.fontFamily;
        winrt::com_ptr<IDWriteTextFormat> format;
        winrt::check_hresult(key.factory->CreateTextFormat(family.c_str(), nullptr, key.weight, DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL, key.fontSize, L"", format.put()));
        winrt::check_hresult(format->SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, key.spacing, key.baseline));
        winrt::check_hresult(format->SetWordWrapping(key.noWrap ? DWRITE_WORD_WRAPPING_NO_WRAP : DWRITE_WORD_WRAPPING_WRAP));
        winrt::check_hresult(format->SetTextAlignment(key.alignment));
        _textFormats[_nextTextFormat] = {key, format};
        _nextTextFormat = (_nextTextFormat + 1) % _textFormats.size();
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        ++_textFormatCreated;
#endif
        return format;
    }

    void NativeDocumentRenderer::SetMathFallback(PreviewMathVisual &visual, IDWriteTextLayout *layout,
                                                 IDWriteTextFormat *format, std::wstring_view content,
                                                 std::wstring_view input, float width, float fontSize)
    {
        // Replacing a previously installed native wrapper must release its
        // arena slot before measuring a fallback. All anchors are recomputed
        // after the final replacement pass below.
        winrt::check_hresult(layout->SetInlineObject(nullptr, {visual.offset, 1}));
        auto ordinal = MathOrdinal(visual.object);
        if (_measuring && ordinal < _mathInlines.size())
            _mathInlines[ordinal] = nullptr;
        visual.presentation = nullptr;
        visual.label = nullptr;
        PreviewString caption{PreviewAllocator<wchar_t>{_layout}};
        caption.assign(MathLabel(visual.state));
        if (!input.empty())
        {
            caption.append(L": ");
            constexpr size_t maximum = 80;
            auto count = std::min(input.size(), maximum);
            if (count < input.size() && count && input[count - 1] >= 0xd800 && input[count - 1] <= 0xdbff)
                --count;
            for (size_t index = 0; index < count; ++index)
            {
                auto character = input[index];
                auto lineBreak = character < L' ' || character == L'\x0085' ||
                                 character == L'\x2028' || character == L'\x2029';
                caption.push_back(lineBreak ? L' ' : character);
            }
            if (count < input.size())
                caption.push_back(L'\x2026');
        }
        auto available = std::max(1.0f, width);
        winrt::check_hresult(_factory->CreateTextLayout(caption.data(), static_cast<uint32_t>(caption.size()), format,
                                                        std::max(1.0f, available - 12.0f), std::max(1.0f, fontSize * 2),
                                                        visual.label.put()));
        winrt::check_hresult(visual.label->SetFontSize(fontSize, {0, static_cast<UINT32>(caption.size())}));
        winrt::check_hresult(visual.label->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP));
        winrt::check_hresult(visual.label->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING));
        winrt::check_hresult(visual.label->SetLineSpacing(DWRITE_LINE_SPACING_METHOD_DEFAULT, 0, 0));
        DWRITE_TEXT_METRICS metrics{};
        DWRITE_LINE_METRICS line{};
        UINT32 count{};
        winrt::check_hresult(visual.label->GetMetrics(&metrics));
        winrt::check_hresult(visual.label->GetLineMetrics(&line, 1, &count));
        if (count != 1 || !std::isfinite(metrics.widthIncludingTrailingWhitespace) || !std::isfinite(line.height) ||
            !std::isfinite(line.baseline))
            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
        visual.width = DisplayMath(visual.syntax)
                           ? available
                           : std::min(available, std::max(24.0f, metrics.widthIncludingTrailingWhitespace + 12.0f));
        visual.height = std::max(fontSize, line.height) + 6;
        visual.baseline = line.baseline + 3;
        winrt::check_hresult(visual.label->SetMaxWidth(std::max(1.0f, visual.width - 12.0f)));
        winrt::check_hresult(visual.label->SetMaxHeight(std::max(1.0f, visual.height - 6.0f)));
        winrt::com_ptr<IDWriteInlineObject> fallback;
        fallback.attach(new (_layout) ImageInline(visual.width, visual.height, visual.baseline,
                                                  DisplayMath(visual.syntax), visual.offset + 1 == content.size()));
        winrt::check_hresult(layout->SetInlineObject(fallback.get(), {visual.offset, 1}));
    }

    PreviewTextVisual NativeDocumentRenderer::MeasureText(Presentation const &batch, notepads_markdown_preview_text const &text,
                                                          float width, float fontSize, bool code, bool header,
                                                          uint32_t alignment, bool codeBlock, bool headingBlock,
                                                          bool noWrap, bool centerLineInk)
    {
        PreviewTextVisual result{_layout};
        result.logicalStart = text.logical_start;
        result.logicalEnd = text.logical_end;
        auto content = batch.Text(text.utf8);
        if (content.size() != text.utf16_length || content.size() > UINT32_MAX)
            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
        result.utf16Length = static_cast<uint32_t>(content.size());
        if (text.logical_start <= text.logical_end && text.logical_end <= _main->Length() &&
            text.logical_end - text.logical_start == content.size())
        {
            auto mainText = _main->Text().substr(static_cast<size_t>(text.logical_start), content.size());
            result.matchesMainText = mainText == std::wstring_view(content.data(), content.size());
        }
        if (codeBlock && !content.empty() && (content.back() == L'\n' || content.back() == L'\r'))
            result.terminalBreak = content.ends_with(L"\r\n") ? 2 : 1;
        auto slice = [&](notepads_markdown_core_rows rows, size_t count)
        {
            if (rows.start > count || rows.count > count - rows.start)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        };
        slice(text.runs, batch.runs.size());
        slice(text.images, batch.images.size());
        bool hasInlineCode{}, hasHighlight{};
        for (uint64_t i = text.runs.start; i < text.runs.start + text.runs.count; ++i)
        {
            auto const &run = batch.runs[static_cast<size_t>(i)];
            if (run.start > run.end || run.end > content.size())
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            hasInlineCode |= !code && (run.flags & 8) && run.start < run.end;
            hasHighlight |= (run.flags & NOTEPADS_MARKDOWN_PREVIEW_RUN_HIGHLIGHT) && run.start < run.end;
            if (run.link != NOTEPADS_MARKDOWN_PREVIEW_NONE)
            {
                if (run.link >= batch.links.size())
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
                auto linkNode = batch.links[run.link].node;
                if (!result.links.empty() && result.links.back().node == linkNode &&
                    uint64_t{result.links.back().start} + result.links.back().length == run.start)
                    result.links.back().length += run.end - run.start;
                else
                {
                    PreviewLinkVisual link{_layout};
                    link.node = linkNode;
                    link.start = run.start;
                    link.length = run.end - run.start;
                    link.destination = batch.Text(batch.links[run.link].destination);
                    result.links.push_back(std::move(link));
                }
            }
        }
        auto textIndex = static_cast<size_t>(&text - batch.texts.data());
        auto firstScope = std::lower_bound(batch.scriptScopes.begin(), batch.scriptScopes.end(), textIndex,
                                           [](notepads_markdown_preview_script_scope const &row, size_t index) { return row.text < index; });
        auto lastScope = std::upper_bound(firstScope, batch.scriptScopes.end(), textIndex,
                                          [](size_t index, notepads_markdown_preview_script_scope const &row) { return index < row.text; });
        auto scopeCount = static_cast<uint64_t>(lastScope - firstScope);
        auto hasScriptScopes = std::any_of(firstScope, lastScope, [](notepads_markdown_preview_script_scope const &scope)
                                          { return !(scope.font_flags & NOTEPADS_MARKDOWN_PREVIEW_SCRIPT_SHAPE_ONLY); });
        auto firstRuby = std::lower_bound(batch.rubyUnits.begin(), batch.rubyUnits.end(), textIndex,
                                         [](auto const &unit, size_t index) { return unit.text < index; });
        auto lastRuby = std::upper_bound(firstRuby, batch.rubyUnits.end(), textIndex,
                                        [](size_t index, auto const &unit) { return index < unit.text; });
        const bool hasRuby = firstRuby != lastRuby;
        result.bitmapCandidate = !code && !hasScriptScopes && !hasRuby && result.matchesMainText &&
            content.size() <= 4096;
        result.prewarmCandidate = result.bitmapCandidate && !hasInlineCode;
        PreviewVector<uint32_t> shapeBarriers{PreviewAllocator<uint32_t>{_layout}};
        if (hasScriptScopes)
        {
            shapeBarriers.reserve(static_cast<size_t>(scopeCount) * 2);
            for (auto scope = firstScope; scope != lastScope; ++scope)
            {
                if (scope->start == scope->end)
                    continue;
                for (auto position : {scope->start, scope->end})
                    if (position && position < result.utf16Length)
                        shapeBarriers.push_back(position);
            }
            std::sort(shapeBarriers.begin(), shapeBarriers.end());
            shapeBarriers.erase(std::unique(shapeBarriers.begin(), shapeBarriers.end()), shapeBarriers.end());
        }
        auto codeSpans = [&](auto const &visit)
        {
            auto emit = [&](uint32_t start, uint32_t finish)
            {
                auto boundary = std::upper_bound(shapeBarriers.begin(), shapeBarriers.end(), start);
                for (; boundary != shapeBarriers.end() && *boundary < finish; ++boundary)
                {
                    visit(start, *boundary);
                    start = *boundary;
                }
                if (start < finish)
                    visit(start, finish);
            };
            uint32_t start{}, finish{};
            for (uint64_t i = text.runs.start; i < text.runs.start + text.runs.count; ++i)
            {
                auto const &run = batch.runs[static_cast<size_t>(i)];
                if ((run.flags & 8) && run.start < run.end)
                {
                    auto const &prior = batch.runs[static_cast<size_t>(i > text.runs.start ? i - 1 : i)];
                    if (start != finish && (run.start != finish || run.size_scale != prior.size_scale ||
                                           run.relative_em != prior.relative_em || run.absolute_dip != prior.absolute_dip))
                    {
                        emit(start, finish);
                        start = finish;
                    }
                    if (start == finish)
                        start = run.start;
                    finish = run.end;
                }
                else
                {
                    if (start < finish)
                        emit(start, finish);
                    start = finish;
                }
            }
            if (start < finish)
                emit(start, finish);
        };
        auto firstDerived = std::lower_bound(batch.derived.begin(), batch.derived.end(), textIndex,
                                             [](notepads_markdown_derived_row const &row, size_t index) { return row.text < index; });
        auto lastDerived = std::upper_bound(firstDerived, batch.derived.end(), textIndex,
                                            [](size_t index, notepads_markdown_derived_row const &row) { return index < row.text; });
        auto const &family = code ? _theme.codeFontFamily : _theme.fontFamily;
        auto weight = header ? DWRITE_FONT_WEIGHT_SEMI_BOLD : DWRITE_FONT_WEIGHT_NORMAL;
        auto spacing = fontSize * (headingBlock ? 1.25f : _theme.lineHeight / _theme.fontSize);
        // Distribute a quote's line leading around the typical capital height.
        const auto baseline = codeBlock ? CodeBlockStyle::TextBaseline(fontSize, spacing)
                                        : centerLineInk ? (spacing + 0.72f * fontSize) * 0.5f : fontSize;
        auto maxWidth = std::max(1.0f, width);
        constexpr float maxHeight = 1000000000.0f;
        auto textAlignment = alignment == 2   ? DWRITE_TEXT_ALIGNMENT_CENTER
                             : alignment == 3 ? DWRITE_TEXT_ALIGNMENT_TRAILING
                                              : DWRITE_TEXT_ALIGNMENT_LEADING;
        NativeMeasurementCache::Workspace workspace;
        uint32_t keyBytes{};
        uint64_t cacheEpoch{};
        auto const requiredKey = 92ull + (uint64_t(family.size()) + _theme.codeFontFamily.size() + content.size()) * 2 +
                                 text.runs.count * 40 + scopeCount * 36;
        if (_measuring && _measurementCache && !hasRuby && text.images.count == 0 && firstDerived == lastDerived &&
            requiredKey <= NOTEPADS_MARKDOWN_METRIC_CACHE_KEY_LIMIT)
        {
            workspace = _measurementCache->TryWorkspace();
            if (workspace)
            {
                auto required = requiredKey;
                if (required <= workspace.Key().size())
                {
                    MetricWriter writer{workspace.Key()};
                    writer.Word(0x334B534Du); // Stable script shaping and paint measurement key.
                    writer.Word(codeBlock ? 2 : hasInlineCode ? 3 : 1); // Padded inline-code measurement policy.
                    writer.Word(static_cast<uint32_t>(required));
                    writer.Word(0);
                    writer.Word(static_cast<uint32_t>(_key.theme_generation));
                    writer.Word(static_cast<uint32_t>(_key.theme_generation >> 32));
                    writer.Word(static_cast<uint32_t>(family.size()));
                    writer.Word(static_cast<uint32_t>(_theme.codeFontFamily.size()));
                    writer.Word(0); // The actual DWrite locale is empty.
                    writer.Word(static_cast<uint32_t>(content.size()));
                    writer.Word(static_cast<uint32_t>(text.runs.count));
                    writer.Word(static_cast<uint32_t>(scopeCount));
                    writer.Word(weight);
                    writer.Word(DWRITE_FONT_STYLE_NORMAL);
                    writer.Word(DWRITE_FONT_STRETCH_NORMAL);
                    writer.Word(DWRITE_LINE_SPACING_METHOD_UNIFORM);
                    writer.Word(noWrap ? DWRITE_WORD_WRAPPING_NO_WRAP : DWRITE_WORD_WRAPPING_WRAP);
                    writer.Word(textAlignment);
                    writer.Float(fontSize);
                    writer.Float(spacing);
                    writer.Float(baseline);
                    writer.Float(maxWidth);
                    writer.Float(maxHeight);
                    writer.Text(family);
                    writer.Text(_theme.codeFontFamily);
                    writer.Text({content.data(), content.size()});
                    for (uint64_t i = text.runs.start; i < text.runs.start + text.runs.count; ++i)
                    {
                        auto const &run = batch.runs[static_cast<size_t>(i)];
                        writer.Word(run.start);
                        writer.Word(run.end);
                        writer.Word(run.flags);
                        writer.Word(run.link != NOTEPADS_MARKDOWN_PREVIEW_NONE ? 1u : 0u);
                        writer.Double(run.size_scale);
                        writer.Double(run.relative_em);
                        writer.Double(run.absolute_dip);
                    }
                    for (auto scope = firstScope; scope != lastScope; ++scope)
                    {
                        writer.Word(scope->start);
                        writer.Word(scope->end);
                        writer.Word(scope->font_flags);
                        writer.Double(scope->size_scale);
                        writer.Double(scope->relative_em);
                        writer.Double(scope->absolute_dip);
                    }
                    if (writer.valid && writer.size == required)
                    {
                        keyBytes = static_cast<uint32_t>(writer.size);
                        auto found = workspace.Lookup(keyBytes, _measurementEpoch);
                        cacheEpoch = found.epoch == _measurementEpoch ? _measurementEpoch : 0;
                        if (found.disposition == NOTEPADS_MARKDOWN_METRIC_CACHE_HIT &&
                            found.copied_bytes <= workspace.Payload().size())
                        {
                            TextMetricView metrics{workspace.Payload().first(found.copied_bytes)};
                            if (metrics.Valid(result.utf16Length))
                            {
                                if (text.logical_start > text.logical_end ||
                                    text.logical_end > _main->Length())
                                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                                // Mandatory current navigation allocation failures propagate;
                                // never retry after a partial append as an optional cache miss.
                                result.metricToken =
                                    _navigation->AppendTextMetrics(static_cast<uint32_t>(text.logical_start),
                                                                   static_cast<uint32_t>(text.logical_end), metrics,
                                                                   result.terminalBreak);
                                result.width = metrics.Width();
                                result.height = metrics.Height();
                                return result;
                            }
                        }
                    }
                }
            }
        }
        // Private script loaders remain bound to the isolated renderer factory.
        // Even shape-only scopes and custom inline objects retain that path.
        auto factory = scopeCount == 0 && !hasRuby && text.images.count == 0 && firstDerived == lastDerived
            ? SystemTextFactory() : _factory.get();
        auto format = TextFormat({factory, weight, textAlignment, fontSize, spacing, baseline, code, noWrap});
        winrt::check_hresult(factory->CreateTextLayout(content.data(), static_cast<UINT32>(content.size()),
                                                        format.get(), maxWidth, maxHeight, result.layout.put()));
        for (uint64_t i = text.runs.start; i < text.runs.start + text.runs.count; ++i)
        {
            auto const &run = batch.runs[static_cast<size_t>(i)];
            if (run.start > run.end || run.end > content.size())
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            DWRITE_TEXT_RANGE range{run.start, run.end - run.start};
            if (run.flags & 1)
                winrt::check_hresult(result.layout->SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, range));
            if (run.flags & 2)
                winrt::check_hresult(result.layout->SetFontStyle(DWRITE_FONT_STYLE_ITALIC, range));
            if (run.flags & 4)
                winrt::check_hresult(result.layout->SetStrikethrough(TRUE, range));
            if (run.flags & NOTEPADS_MARKDOWN_PREVIEW_RUN_UNDERLINE)
            {
                winrt::check_hresult(result.layout->SetUnderline(TRUE, range));
                result.authoredUnderlines.push_back(range);
            }
            if (run.flags & (8u | NOTEPADS_MARKDOWN_PREVIEW_RUN_MONOSPACE))
                winrt::check_hresult(result.layout->SetFontFamilyName(_theme.codeFontFamily.c_str(), range));
        }
        std::shared_ptr<NativeScriptLayout> scripts;
        std::shared_ptr<NativeScriptFontBinding> fontBinding;
        auto textRuns = std::span<notepads_markdown_preview_run const>{batch.runs}.subspan(
            static_cast<size_t>(text.runs.start), static_cast<size_t>(text.runs.count));
        if (hasScriptScopes || hasRuby)
        {
            scripts = NativeScriptLayout::Create(
                _layout, _factory.get(), fontSize, family, _theme.codeFontFamily, weight,
                textRuns, {firstScope, lastScope}, result.utf16Length);
            scripts->ApplyFontSizes(result.layout.get());
        }
        if (hasScriptScopes)
        {
            if (!_scriptFonts)
                _scriptFonts = NativeScriptFonts::Create(_layout, _factory.get());
            fontBinding = _scriptFonts->Bind(result.layout.get(), textRuns, result.utf16Length, shapeBarriers);
            result.scriptFonts = fontBinding;
        }
        auto scalarBoundary = [&](uint32_t offset)
        {
            return offset == content.size() || (offset < content.size() && !IS_LOW_SURROGATE(content[offset]));
        };
        for (auto unit = firstRuby; unit != lastRuby; ++unit)
        {
            if (code || !result.matchesMainText || !scalarBoundary(unit->start) ||
                !scalarBoundary(unit->base_end) || !scalarBoundary(unit->end) ||
                std::wstring_view{content}.substr(unit->start, unit->end - unit->start)
                    .find_first_of(L"\r\n\x2028\x2029\xfffc") != std::wstring_view::npos)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            auto metrics = scripts->AddRuby(factory, content, *unit, weight);
            winrt::com_ptr<IDWriteInlineObject> slot;
            slot.attach(new (_layout) ImageInline(metrics.width, metrics.height, metrics.baseline));
            const DWRITE_TEXT_RANGE range{unit->start, unit->end - unit->start};
            winrt::check_hresult(result.layout->SetInlineObject(slot.get(), range));
            // The two child layouts own ruby decorations at their individual
            // baselines; the parent inline slot must not paint a second line.
            winrt::check_hresult(result.layout->SetUnderline(FALSE, range));
            winrt::check_hresult(result.layout->SetStrikethrough(FALSE, range));
        }
        auto fontAt = [&](uint32_t offset) { return scripts ? scripts->FontSizeAt(offset) : fontSize; };
        slice(text.images, batch.images.size());
        winrt::com_ptr<IDWriteInlineObject> imageEllipsis;
        auto enclosingLink = result.links.begin();
        for (uint64_t i = text.images.start; i < text.images.start + text.images.count; ++i)
        {
            auto const &image = batch.images[static_cast<size_t>(i)];
            if (image.offset_utf16 >= content.size() || content[image.offset_utf16] != L'\xfffc')
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            if (!result.images.empty() && result.images.back().offset >= image.offset_utf16)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            PreviewImageVisual visual;
            visual.offset = image.offset_utf16;
            visual.object = image.object;
            auto imageFontSize = fontAt(visual.offset);
            auto setGeometry = [&](double imageWidth, double imageHeight)
            {
                double scale = 1.0;
                if (image.width && image.height)
                {
                    imageWidth = image.width;
                    imageHeight = image.height;
                }
                else if (image.width)
                    scale = image.width / imageWidth;
                else if (image.height)
                    scale = image.height / imageHeight;
                scale = std::min(scale, std::max(1.0, static_cast<double>(width)) / imageWidth);
                if (image.width || image.height)
                    scale = std::min(scale, ImageMaximumIntrinsicSide / imageHeight);
                auto displayWidth = static_cast<float>(imageWidth * scale);
                auto displayHeight = static_cast<float>(imageHeight * scale);
                if (!std::isfinite(displayWidth) || displayWidth <= 0 ||
                    !std::isfinite(displayHeight) || displayHeight <= 0)
                    return false;
                visual.width = displayWidth;
                visual.height = visual.baseline = displayHeight;
                return true;
            };
            // Both dimensions are independent of the decoded aspect ratio.
            // Reserve the same inline box before a provider has any metadata.
            if (image.width && image.height)
                setGeometry(image.width, image.height);
            uint32_t state = NOTEPADS_MARKDOWN_IMAGE_UNREQUESTED;
            if (_images)
            {
                if (auto entry = _images->Find(image.object))
                {
                    state = entry->state.state;
                    if (state == NOTEPADS_MARKDOWN_IMAGE_READY)
                    {
                        if (!entry->pixels)
                        {
                            state = NOTEPADS_MARKDOWN_IMAGE_FAILED;
                        }
                        else
                        {
                            auto const &metadata = entry->pixels->metadata;
                            if (!entry->state.published_request ||
                                !SameImageMetadata(metadata, entry->state.metadata) ||
                                !ValidImageMetadata(metadata, entry->pixels->pixels.size()))
                                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                            visual.hasPixels = setGeometry(metadata.natural_width_dip, metadata.natural_height_dip);
                            if (!visual.hasPixels)
                                state = NOTEPADS_MARKDOWN_IMAGE_FAILED;
                        }
                    }
                    if (!visual.hasPixels && (entry->state.status_flags & NOTEPADS_MARKDOWN_IMAGE_HAS_METADATA))
                    {
                        auto const &metadata = entry->state.metadata;
                        auto capacity = uint64_t(metadata.stride) * metadata.decoded_height;
                        if (capacity > SIZE_MAX || !ValidImageMetadata(metadata, static_cast<size_t>(capacity)) ||
                            !setGeometry(metadata.natural_width_dip, metadata.natural_height_dip))
                            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                    }
                }
                else if (_images->policy.header.count >= NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES)
                {
                    state = NOTEPADS_MARKDOWN_IMAGE_LIMITED;
                }
            }
            if (!visual.hasPixels)
            {
                const auto retainedWidth = visual.width;
                const auto retainedHeight = visual.height;
                while (enclosingLink != result.links.end() &&
                       enclosingLink->start + enclosingLink->length <= visual.offset)
                    ++enclosingLink;
                visual.linked = enclosingLink != result.links.end() && enclosingLink->start <= visual.offset;
                visual.pending = state == NOTEPADS_MARKDOWN_IMAGE_PENDING || state == NOTEPADS_MARKDOWN_IMAGE_UNREQUESTED;
                // The bounded display caption is separate from the full alt used by copy and UIA.
                auto caption = batch.ImageCaption(image.alt);
                std::replace_if(caption.begin(), caption.end(), [](wchar_t character)
                                { return character < L' ' || character == L'\x85' ||
                                         character == L'\x2028' || character == L'\x2029'; }, L' ');
                auto placeholderWidth = retainedWidth > 0 ? std::min(maxWidth, retainedWidth) : maxWidth;
                const bool hasCaption = caption.find_first_not_of(L' ') != PreviewString::npos &&
                                        (retainedHeight <= 0 || retainedHeight >= imageFontSize);
                if (visual.linked || visual.pending || hasCaption)
                {
                    visual.inset = std::min(3.0f, placeholderWidth / 8.0f);
                    visual.iconSize = std::min(std::min(imageFontSize * 0.85f, 20.0f), placeholderWidth - 2 * visual.inset);
                    if (retainedHeight > 0)
                        visual.iconSize = std::min(visual.iconSize, retainedHeight);
                    visual.width = visual.iconSize + 2 * visual.inset;
                    visual.height = visual.baseline = visual.iconSize;
                    const auto captionWidth = std::min(placeholderWidth, imageFontSize * 18) - visual.width - visual.inset;
                    if (hasCaption && captionWidth >= imageFontSize)
                    {
                        const auto length = static_cast<UINT32>(caption.size());
                        winrt::check_hresult(_factory->CreateTextLayout(caption.data(), length, format.get(),
                                                                        captionWidth, maxHeight, visual.label.put()));
                        winrt::check_hresult(visual.label->SetFontSize(imageFontSize, {0, length}));
                        winrt::check_hresult(visual.label->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP));
                        winrt::check_hresult(visual.label->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING));
                        winrt::check_hresult(visual.label->SetLineSpacing(DWRITE_LINE_SPACING_METHOD_DEFAULT, 0, 0));
                        if (!imageEllipsis)
                            winrt::check_hresult(_factory->CreateEllipsisTrimmingSign(format.get(), imageEllipsis.put()));
                        DWRITE_TRIMMING trimming{DWRITE_TRIMMING_GRANULARITY_CHARACTER, 0, 0};
                        winrt::check_hresult(visual.label->SetTrimming(&trimming, imageEllipsis.get()));
                        DWRITE_TEXT_METRICS metrics{};
                        DWRITE_LINE_METRICS line{};
                        UINT32 count{};
                        winrt::check_hresult(visual.label->GetMetrics(&metrics));
                        winrt::check_hresult(visual.label->GetLineMetrics(&line, 1, &count));
                        if (count != 1 || !std::isfinite(line.height) || !std::isfinite(line.baseline) || line.height <= 0)
                            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                        visual.width += std::min(captionWidth, metrics.width) + visual.inset;
                        visual.height = line.height;
                        visual.baseline = line.baseline;
                    }
                }
                if (retainedWidth > 0)
                {
                    visual.width = retainedWidth;
                    visual.height = visual.baseline = retainedHeight;
                }
            }
            winrt::com_ptr<IDWriteInlineObject> object;
            object.attach(new (_layout) ImageInline(visual.width, visual.height, visual.baseline));
            winrt::check_hresult(result.layout->SetInlineObject(object.get(), {image.offset_utf16, 1}));
            result.images.push_back(std::move(visual));
        }
        for (auto row = firstDerived; row != lastDerived; ++row)
        {
            if (row->kind != NOTEPADS_MARKDOWN_DERIVED_NOTE_EMBED)
                continue;
            if (row->offset_utf16 >= content.size() || content[row->offset_utf16] != L'\xfffc')
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            PreviewImageVisual visual;
            visual.note = true;
            visual.object = row->object;
            visual.offset = row->offset_utf16;
            visual.width = maxWidth;
            visual.inset = std::min(12.0f, maxWidth * 0.25f);
            // Unresolved notes and cycles keep one compact caption. Loaded
            // children publish their fitted extent, capped for independent scrolling.
            visual.height = std::min(NoteEmbedStyle::maximumHeight,
                                     spacing + 2 * visual.inset);
            if (_noteEmbeds)
            {
                auto extent = std::lower_bound(_noteEmbeds->begin(), _noteEmbeds->end(), row->object,
                    [](auto const &entry, uint32_t object) { return entry.object < object; });
                if (extent != _noteEmbeds->end() && extent->object == row->object)
                    visual.height = extent->height;
            }
            visual.baseline = visual.height;
            auto caption = batch.ImageCaption(row->input);
            std::replace_if(caption.begin(), caption.end(), [](wchar_t character)
                { return character < L' ' || character == L'\x85' ||
                         character == L'\x2028' || character == L'\x2029'; }, L' ');
            auto length = static_cast<uint32_t>(caption.size());
            winrt::check_hresult(_factory->CreateTextLayout(caption.data(), length, format.get(),
                                                            maxWidth - 2 * visual.inset,
                                                            std::max(1.0f, visual.height - 2 * visual.inset),
                                                            visual.label.put()));
            winrt::check_hresult(visual.label->SetFontSize(fontAt(visual.offset), {0, length}));
            winrt::check_hresult(visual.label->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP));
            if (!imageEllipsis)
                winrt::check_hresult(_factory->CreateEllipsisTrimmingSign(format.get(), imageEllipsis.put()));
            DWRITE_TRIMMING trimming{DWRITE_TRIMMING_GRANULARITY_CHARACTER, 0, 0};
            winrt::check_hresult(visual.label->SetTrimming(&trimming, imageEllipsis.get()));
            winrt::com_ptr<IDWriteInlineObject> slot;
            slot.attach(new (_layout) ImageInline(visual.width, visual.height, visual.baseline,
                                                  true, visual.offset + 1 == content.size()));
            winrt::check_hresult(result.layout->SetInlineObject(slot.get(), {visual.offset, 1}));
            result.images.push_back(std::move(visual));
        }
        std::sort(result.images.begin(), result.images.end(), [](auto const &left, auto const &right)
            { return left.offset < right.offset; });
        auto mathCount = std::count_if(firstDerived, lastDerived,
                                       [](notepads_markdown_derived_row const &row) { return row.kind == NOTEPADS_MARKDOWN_DERIVED_MATH; });
        result.math.reserve(static_cast<size_t>(mathCount));
        auto fallback = [&](PreviewMathVisual &visual)
        {
            auto row = std::find_if(firstDerived, lastDerived, [&](notepads_markdown_derived_row const &value)
                                    { return value.kind == NOTEPADS_MARKDOWN_DERIVED_MATH && value.object == visual.object; });
            if (row == lastDerived)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            // Reuse the bounded scalar-safe caption decoder. Full formula and
            // exact delimiters remain in the independent semantic descriptor.
            auto input = batch.ImageCaption(row->input);
            SetMathFallback(visual, result.layout.get(), format.get(), {content.data(), content.size()},
                            {input.data(), input.size()}, width, fontAt(visual.offset));
        };
        for (auto row = firstDerived; row != lastDerived; ++row)
        {
            if (row->kind != NOTEPADS_MARKDOWN_DERIVED_MATH)
                continue;
            if (row->offset_utf16 >= content.size() || content[row->offset_utf16] != L'\xfffc')
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            PreviewMathVisual visual;
            visual.object = row->object;
            visual.offset = row->offset_utf16;
            visual.syntax = row->syntax;
            auto ordinal = MathOrdinal(row->object);
            visual.state = MathAttempt(_math, ordinal);
            if (_math.enabled && ordinal < _math.attempts.size())
                if (auto const *owner = _math.presentations.Find(_math.key, row->object);
                    owner && (*owner)->Key().syntax == row->syntax)
                    visual.presentation = owner->operator->();
            if (visual.presentation)
            {
                visual.state = visual.presentation->State();
                if (!_measuring && ordinal < _mathStates.size() && _mathStates[ordinal] != NativeMathState::None)
                    visual.state = _mathStates[ordinal];
                if (visual.state == NativeMathState::Ready)
                {
                    auto const &drawing = visual.presentation->Drawing();
                    auto const &memory = visual.presentation->Memory();
                    if (!drawing || !memory || !memory->IsFinalized())
                        visual.state = NativeMathState::Unavailable;
                    else
                    {
                        auto objectFontSize = fontAt(visual.offset);
                        visual.placement = drawing->Place(objectFontSize, std::max(1.0f, width),
                                                          DisplayMath(row->syntax) ? MathPlacementMode::Display
                                                                                   : MathPlacementMode::Inline,
                                                          objectFontSize * 0.3, objectFontSize * 0.3);
                        if (visual.placement.status != MathPlacementStatus::Ready)
                            visual.state = visual.placement.status == MathPlacementStatus::Limited
                                               ? NativeMathState::Limited
                                               : NativeMathState::Failed;
                        else
                        {
                            try
                            {
                                // Freeze arena admission during the same pass
                                // that submits Rust block heights. Later Frames
                                // share this immutable wrapper, so an old Frame
                                // cannot change this renderer's line metrics by
                                // consuming the last free arena slot afterward.
                                auto object = _mathInlines[ordinal];
                                if (!object)
                                    object = NativeMathDrawing::CreateInlineObject(
                                        memory, visual.placement, {content.data(), content.size()}, visual.offset);
                                winrt::check_hresult(result.layout->SetInlineObject(object.get(), {visual.offset, 1}));
                                _mathInlines[ordinal] = object;
                                visual.width = visual.placement.metrics.width;
                                visual.height = visual.placement.metrics.height;
                                visual.baseline = visual.placement.metrics.baseline;
                            }
                            catch (NativeFailure const &error)
                            {
                                visual.state = MathFailureState(error.status);
                            }
                            catch (std::bad_alloc const &)
                            {
                                visual.state = NativeMathState::Limited;
                            }
                            catch (winrt::hresult_error const &error)
                            {
                                visual.state = MathFailureState(error.code().value);
                            }
                            catch (...)
                            {
                                visual.state = NativeMathState::Failed;
                            }
                        }
                    }
                }
            }
            if (visual.state != NativeMathState::Ready)
                fallback(visual);
            result.math.push_back(std::move(visual));
        }
        auto diagramCount = std::count_if(firstDerived, lastDerived,
                                          [](notepads_markdown_derived_row const &row) { return row.kind == NOTEPADS_MARKDOWN_DERIVED_DIAGRAM; });
        result.diagrams.reserve(static_cast<size_t>(diagramCount));
        for (auto row = firstDerived; row != lastDerived; ++row)
        {
            if (row->kind != NOTEPADS_MARKDOWN_DERIVED_DIAGRAM)
                continue;
            if (row->offset_utf16 >= content.size() || content[row->offset_utf16] != L'\xfffc' ||
                (!result.diagrams.empty() && result.diagrams.back().offset >= row->offset_utf16))
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            PreviewDiagramVisual visual;
            visual.object = row->object;
            visual.offset = row->offset_utf16;
            auto ordinal = DiagramOrdinal(row->object);
            visual.ordinal = ordinal;
            visual.state = DiagramAttempt(_diagram, ordinal);
            if (_diagram.enabled && ordinal < _diagram.attempts.size())
                if (auto const *owner = _diagram.presentations.Find(_diagram.key, row->object))
                {
                    visual.presentation = *owner;
                    visual.state = (*owner)->State();
                }
            if (!_measuring && ordinal < _diagramStates.size() && _diagramStates[ordinal] != NativeDiagramState::None)
                visual.state = _diagramStates[ordinal];
            if (visual.state == NativeDiagramState::Ready)
            {
                auto status = visual.presentation ? NativeDiagramDrawing::Place(*visual.presentation.operator->(),
                                                                                width, visual.placement)
                                                  : NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
                if (status == NOTEPADS_MARKDOWN_CORE_OK)
                {
                    visual.width = visual.placement.width;
                    visual.height = visual.placement.height;
                    visual.baseline = visual.placement.baseline;
                    winrt::com_ptr<IDWriteInlineObject> object = _diagramInlines[ordinal];
                    if (!object)
                        status = NativeDiagramArtifact::CreateInlineObject(
                            visual.presentation, visual.width, visual.height, visual.baseline,
                            visual.offset + 1 == content.size(), object.put());
                    if (SUCCEEDED(status))
                        status = result.layout->SetInlineObject(object.get(), {visual.offset, 1});
                    if (SUCCEEDED(status))
                    {
                        visual.hasInline = true;
                        _diagramInlines[ordinal] = object;
                    }
                }
                if (status != NOTEPADS_MARKDOWN_CORE_OK)
                    visual.state = DiagramFailureState(status);
            }
            if (visual.state != NativeDiagramState::Ready)
            {
                SetDiagramFallback(visual, result.layout.get(), format.get(), {content.data(), content.size()},
                                   width, fontAt(visual.offset));
            }
            else
            {
                constexpr std::array states{NativeDiagramState::Failed, NativeDiagramState::Limited,
                                            NativeDiagramState::Unavailable};
                for (size_t index = 0; index < states.size(); ++index)
                    visual.drawFailureLabels[index] = DiagramCaption(
                        states[index], result.layout.get(), format.get(), visual.offset, visual.width,
                        visual.height - 6, fontAt(visual.offset));
            }
            if (content.size() == 1 && visual.offset == 0)
                winrt::check_hresult(result.layout->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER));
            result.diagrams.push_back(std::move(visual));
        }
        if (hasInlineCode)
        {
            // Actual advances reserve padding at the inline span's two ends;
            // text, selection and wrapping all use this same padded layout.
            // Apply spacing to complete shaping clusters, never surrogate or
            // combining/ZWJ interiors. Continuations follow inline slice rules.
            struct ClusterPadding
            {
                uint32_t end;
                uint8_t edges;
            };
            PreviewVector<ClusterPadding> clusterEnds{PreviewAllocator<ClusterPadding>{_layout}};
            {
                UINT32 count{};
                auto status = result.layout->GetClusterMetrics(nullptr, 0, &count);
                if (FAILED(status) && status != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                    winrt::check_hresult(status);
                PreviewVector<DWRITE_CLUSTER_METRICS> clusters{PreviewAllocator<DWRITE_CLUSTER_METRICS>{_layout}};
                clusters.resize(count);
                if (count)
                    winrt::check_hresult(result.layout->GetClusterMetrics(clusters.data(), count, &count));
                clusters.resize(count);
                clusterEnds.reserve(count);
                uint32_t end{};
                for (auto const &cluster : clusters)
                {
                    if (!cluster.length || cluster.length > content.size() - end)
                        throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                    end += cluster.length;
                    clusterEnds.push_back({end, 0});
                }
                if (end != content.size())
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
            }
            auto padded = result.layout.as<IDWriteTextLayout1>();

            codeSpans([&](uint32_t start, uint32_t end)
            {
                auto first = std::upper_bound(clusterEnds.begin(), clusterEnds.end(), start,
                                               [](uint32_t value, ClusterPadding const &cluster) { return value < cluster.end; });
                auto last = std::lower_bound(clusterEnds.begin(), clusterEnds.end(), end,
                                              [](ClusterPadding const &cluster, uint32_t value) { return cluster.end < value; });
                if (first == clusterEnds.end() || last == clusterEnds.end())
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                first->edges |= 1;
                last->edges |= 2;
            });
            uint32_t start{};
            for (auto const &cluster : clusterEnds)
            {
                if (cluster.edges)
                    winrt::check_hresult(padded->SetCharacterSpacing(cluster.edges & 1 ? std::min(fontAt(start) * 0.2f, maxWidth * 0.25f) : 0,
                                                                      cluster.edges & 2 ? std::min(fontAt(start) * 0.2f, maxWidth * 0.25f) : 0, 0,
                                                                      {start, cluster.end - start}));
                start = cluster.end;
            }
        }
        std::shared_ptr<NativeScriptDrawing> scriptDrawing;
        if (scripts)
        {
            // Permanent semantic effects participate in the final shaping pass.
            // Hover and selection later change paint only, never layout properties.
            scriptDrawing = NativeScriptDrawing::Create(_layout, scripts, _factory.get(), textRuns);
            scriptDrawing->Prepare(result.layout.get(), result.links, result.images);
        }
        // Uniform line height overrides inline metrics. Content-based spacing
        // lets loaded images and measured placeholders determine their lines;
        // ordinary paragraphs keep the theme's existing uniform text spacing.
        if (!result.images.empty() || !result.math.empty() || !result.diagrams.empty())
        {
            winrt::check_hresult(result.layout->SetLineSpacing(DWRITE_LINE_SPACING_METHOD_DEFAULT, 0, 0));
            // Anchor can fail after an inline was installed (notably the
            // funded 256-line bound). Replace only that object, then repeat
            // against the final layout so prior anchors cannot become stale.
            bool replaced{};
            do
            {
                replaced = false;
                for (auto &math : result.math)
                {
                    if (math.state != NativeMathState::Ready)
                        continue;
                    try
                    {
                        math.anchor = math.presentation->Drawing()->TextAnchor(result.layout.get(), math.offset);
                    }
                    catch (NativeFailure const &error)
                    {
                        math.state = MathFailureState(error.status);
                    }
                    catch (std::bad_alloc const &)
                    {
                        math.state = NativeMathState::Limited;
                    }
                    catch (winrt::hresult_error const &error)
                    {
                        math.state = MathFailureState(error.code().value);
                    }
                    catch (...)
                    {
                        math.state = NativeMathState::Failed;
                    }
                    if (math.state != NativeMathState::Ready)
                    {
                        fallback(math);
                        replaced = true;
                    }
                }
            } while (replaced);
            UINT32 count{};
            auto status = result.layout->GetLineMetrics(nullptr, 0, &count);
            if (FAILED(status) && status != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                winrt::check_hresult(status);
            PreviewVector<DWRITE_LINE_METRICS> lines{PreviewAllocator<DWRITE_LINE_METRICS>{_layout}};
            lines.resize(count);
            if (count)
                winrt::check_hresult(result.layout->GetLineMetrics(lines.data(), count, &count));
            size_t imageIndex{};
            size_t mathIndex{};
            size_t diagramIndex{};
            uint32_t lineStart{};
            float lineY{};
            for (auto const &line : lines)
            {
                if (line.length > content.size() - lineStart || !std::isfinite(line.height) ||
                    !std::isfinite(line.baseline))
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                while (imageIndex < result.images.size() && result.images[imageIndex].offset < lineStart + line.length)
                {
                    auto &image = result.images[imageIndex++];
                    FLOAT x{}, y{};
                    DWRITE_HIT_TEST_METRICS hit{};
                    winrt::check_hresult(result.layout->HitTestTextPosition(image.offset, FALSE, &x, &y, &hit));
                    image.x = hit.left;
                    // Hit-test regions can include text leading/descent. The
                    // actual bitmap aligns its own baseline with this line.
                    image.y = lineY + line.baseline - image.baseline;
                }
                while (mathIndex < result.math.size() && result.math[mathIndex].offset < lineStart + line.length)
                {
                    auto &math = result.math[mathIndex++];
                    FLOAT x{}, y{};
                    DWRITE_HIT_TEST_METRICS hit{};
                    winrt::check_hresult(result.layout->HitTestTextPosition(math.offset, FALSE, &x, &y, &hit));
                    math.x = hit.left;
                    math.y = lineY + line.baseline - math.baseline;
                }
                while (diagramIndex < result.diagrams.size() &&
                       result.diagrams[diagramIndex].offset < lineStart + line.length)
                {
                    auto &diagram = result.diagrams[diagramIndex++];
                    FLOAT x{}, y{};
                    DWRITE_HIT_TEST_METRICS hit{};
                    winrt::check_hresult(result.layout->HitTestTextPosition(diagram.offset, FALSE, &x, &y, &hit));
                    diagram.x = diagram.hasInline ? hit.left : 0.0f;
                    diagram.y = diagram.hasInline ? lineY + line.baseline - diagram.baseline : lineY;
                    if (!diagram.hasInline)
                    {
                        diagram.height = (std::max)(line.height, diagram.height);
                        diagram.baseline = line.baseline;
                    }
                }
                lineStart += line.length;
                lineY += line.height;
            }
            if (imageIndex != result.images.size() || mathIndex != result.math.size() ||
                diagramIndex != result.diagrams.size())
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
        }
        if (scripts)
        {
            // Resolve only after character spacing and all object fallback/reflow.
            scripts->Resolve(result.layout.get(), spacing);
            for (auto &image : result.images)
                image.y += scripts->BaselineDeltaAt(image.offset);
            for (auto &math : result.math)
            {
                auto delta = scripts->BaselineDeltaAt(math.offset);
                math.y += delta;
                math.anchor.baseline += delta;
            }
            for (auto &diagram : result.diagrams)
                diagram.y += scripts->BaselineDeltaAt(diagram.offset);
            result.scripts = scripts;
            scriptDrawing->Seal(result.layout.get());
            if (fontBinding)
                fontBinding->Freeze();
            // Measurement can retain this sealed drawing for loading-stage raster preparation.
            result.scriptDrawing = std::move(scriptDrawing);
        }
        DWRITE_TEXT_METRICS metrics{};
        winrt::check_hresult(result.layout->GetMetrics(&metrics));
        result.width = std::max(1.0f, width);
        // Uniformly spaced code keeps its original text layout and clusters. A terminal
        // line break adds an empty DirectWrite row after the last visible code line.
        const auto trailingRowHeight = result.terminalBreak && metrics.lineCount > 1 ? spacing : 0.0f;
        result.height = scripts ? scripts->FinalHeight() : std::max(spacing, metrics.height - trailingRowHeight);
        for (auto const &math : result.math)
        {
            auto ordinal = MathOrdinal(math.object);
            if (ordinal < _mathStates.size())
                _mathStates[ordinal] = math.state;
        }
        for (auto const &diagram : result.diagrams)
        {
            result.height = (std::max)(result.height, diagram.y + diagram.height);
            auto ordinal = DiagramOrdinal(diagram.object);
            if (ordinal < _diagramStates.size())
                _diagramStates[ordinal] = diagram.state;
        }
        if (!_measuring && hasInlineCode)
        {
            // Keep the real text/cluster layout intact. A code span may wrap or
            // reorder into multiple visual fragments, each with its own badge.
            struct LineBox
            {
                uint32_t end;
                float top, bottom, baseline;
            };
            PreviewVector<LineBox> boxes{PreviewAllocator<LineBox>{_layout}};
            {
                UINT32 count{};
                auto status = result.layout->GetLineMetrics(nullptr, 0, &count);
                if (FAILED(status) && status != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                    winrt::check_hresult(status);
                PreviewVector<DWRITE_LINE_METRICS> lines{PreviewAllocator<DWRITE_LINE_METRICS>{_layout}};
                lines.resize(count);
                if (count)
                    winrt::check_hresult(result.layout->GetLineMetrics(lines.data(), count, &count));
                lines.resize(count);
                boxes.reserve(count);
                uint32_t end{};
                float top{};
                for (auto const &line : lines)
                {
                    if (line.length > content.size() - end || !std::isfinite(line.height) ||
                        !std::isfinite(line.baseline))
                        throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                    end += line.length;
                    boxes.push_back({end, top, top + line.height, top + line.baseline});
                    top += line.height;
                }
            }
            PreviewVector<DWRITE_HIT_TEST_METRICS> fragments{PreviewAllocator<DWRITE_HIT_TEST_METRICS>{_layout}};
            auto capture = [&](uint32_t start, uint32_t end)
            {
                if (start == end)
                    return;
                FLOAT x{}, y{};
                DWRITE_HIT_TEST_METRICS firstCluster{}, lastCluster{};
                winrt::check_hresult(result.layout->HitTestTextPosition(start, FALSE, &x, &y, &firstCluster));
                winrt::check_hresult(result.layout->HitTestTextPosition(end - 1, FALSE, &x, &y, &lastCluster));
                if (firstCluster.textPosition > start || lastCluster.textPosition > content.size() ||
                    lastCluster.length > content.size() - lastCluster.textPosition)
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                const auto geometryStart = firstCluster.textPosition;
                const auto geometryEnd = std::max(end, lastCluster.textPosition + lastCluster.length);
                UINT32 needed{};
                auto hr = result.layout->HitTestTextRange(geometryStart, geometryEnd - geometryStart, 0, 0, nullptr, 0, &needed);
                if (FAILED(hr) && hr != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                    winrt::check_hresult(hr);
                fragments.resize(needed);
                if (needed)
                    winrt::check_hresult(result.layout->HitTestTextRange(geometryStart, geometryEnd - geometryStart, 0, 0,
                                                                          fragments.data(), needed, &needed));
                fragments.resize(needed);
                for (auto fragment : fragments)
                {
                    if (!fragment.isText || !fragment.length)
                        continue;
                    if (fragment.textPosition > content.size() || fragment.length > content.size() - fragment.textPosition)
                        throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                    // DirectWrite may enclose a cluster that straddles a style
                    // boundary. Keep its real geometry, but retain only code's
                    // semantic UTF16 range in the decoration metadata.
                    auto first = std::max(start, fragment.textPosition);
                    auto last = std::min(end, fragment.textPosition + fragment.length);
                    if (first >= last)
                        continue;
                    fragment = NativeScriptLayout::CanonicalHitMetric(fragment);
                    if (fragment.width == 0)
                    {
                        winrt::check_hresult(result.layout->HitTestTextPosition(first, FALSE, &x, &y, &fragment));
                        fragment = NativeScriptLayout::CanonicalHitMetric(fragment);
                        if (!fragment.isText)
                            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                    }
                    auto line = std::upper_bound(boxes.begin(), boxes.end(), fragment.textPosition,
                                                  [](uint32_t position, LineBox const &box) { return position < box.end; });
                    if (line == boxes.end())
                        throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                    // Decoration follows the same final baseline and effective
                    // font as the actual code glyphs on their original owner row.
                    auto row = static_cast<size_t>(line - boxes.begin());
                    auto baseline = scripts ? scripts->FinalBaselineAt(fragment.textPosition) : line->baseline;
                    auto top = scripts ? scripts->Lines()[row].finalY : line->top;
                    auto bottom = scripts ? top + scripts->Lines()[row].finalHeight : line->bottom;
                    auto badgeFontSize = fontAt(first);
                    auto bounds = D2D1::RectF(std::max(0.0f, fragment.left),
                                              std::max(top, baseline - badgeFontSize * 0.95f),
                                              std::min(maxWidth, fragment.left + fragment.width),
                                              std::min(bottom, baseline + badgeFontSize * 0.25f));
                    if (!std::isfinite(bounds.left) || !std::isfinite(bounds.top) ||
                        !std::isfinite(bounds.right) || !std::isfinite(bounds.bottom))
                        throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                    if (bounds.bottom > bounds.top && (bounds.right > bounds.left ||
                        (fragment.width == 0 && bounds.right == bounds.left)))
                        result.inlineCode.push_back({first, last - first, bounds});
                }
            };
            codeSpans(capture);
        }
        if (!_measuring && hasHighlight)
        {
            PreviewVector<DWRITE_HIT_TEST_METRICS> raw{PreviewAllocator<DWRITE_HIT_TEST_METRICS>{_layout}};
            ReadingVector<NativeScriptRangeMetric> adjusted{ReadingAllocator<NativeScriptRangeMetric>{_main->Memory()}};
            auto save = [&](DWRITE_HIT_TEST_METRICS fragment)
            {
                if (!fragment.isText)
                    return;
                fragment = NativeScriptLayout::CanonicalHitMetric(fragment);
                const auto bounds = D2D1::RectF(std::max(0.0f, fragment.left), std::max(0.0f, fragment.top),
                                               std::min(maxWidth, fragment.left + fragment.width),
                                               std::min(result.height, fragment.top + fragment.height));
                if (bounds.right > bounds.left && bounds.bottom > bounds.top)
                    result.highlights.push_back(bounds);
            };
            auto capture = [&](uint32_t start, uint32_t end)
            {
                if (start == end)
                    return;
                if (scripts)
                {
                    scripts->Range(start, end - start, adjusted);
                    for (auto const &fragment : adjusted)
                        save(fragment.metrics);
                    return;
                }
                UINT32 count{};
                auto status = result.layout->HitTestTextRange(start, end - start, 0, 0, nullptr, 0, &count);
                if (FAILED(status) && status != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                    winrt::check_hresult(status);
                raw.resize(count);
                if (count)
                    winrt::check_hresult(result.layout->HitTestTextRange(start, end - start, 0, 0,
                                                                         raw.data(), count, &count));
                for (auto const &fragment : std::span{raw.data(), count})
                    save(fragment);
            };
            uint32_t start{}, end{};
            for (uint64_t i = text.runs.start; i < text.runs.start + text.runs.count; ++i)
            {
                auto const &run = batch.runs[static_cast<size_t>(i)];
                if (!(run.flags & NOTEPADS_MARKDOWN_PREVIEW_RUN_HIGHLIGHT) || run.start == run.end)
                    continue;
                if (run.start != end)
                {
                    capture(start, end);
                    start = run.start;
                }
                end = run.end;
            }
            capture(start, end);

            // Bidi fragments can share shaping clusters. Union them in visual order.
            std::sort(result.highlights.begin(), result.highlights.end(), [](auto const &a, auto const &b)
            {
                if (a.top != b.top)
                    return a.top < b.top;
                return a.bottom == b.bottom ? a.left < b.left : a.bottom < b.bottom;
            });
            size_t used{};
            for (auto const &bounds : result.highlights)
            {
                if (used && bounds.top == result.highlights[used - 1].top &&
                    bounds.bottom == result.highlights[used - 1].bottom && bounds.left <= result.highlights[used - 1].right)
                    result.highlights[used - 1].right = std::max(result.highlights[used - 1].right, bounds.right);
                else
                    result.highlights[used++] = bounds;
            }
            result.highlights.resize(used);
        }
        if (workspace && keyBytes && cacheEpoch)
        {
            auto valueBytes = CaptureTextMetrics(result.layout.get(), workspace.Payload(), result.utf16Length,
                                                 result.height, result.width, scripts ? scripts->FinalLineMetrics() : std::span<DWRITE_LINE_METRICS const>{},
                                                 scripts ? scripts->LineOverflowAbove() : 0,
                                                 scripts ? scripts->LineOverflowBelow() : 0);
            if (valueBytes && uint64_t(valueBytes) + keyBytes <= NOTEPADS_MARKDOWN_METRIC_CACHE_ENTRY_BYTES_LIMIT)
            {
                TextMetricView capturedMetrics{workspace.Payload().first(valueBytes)};
                if (capturedMetrics.Valid(result.utf16Length))
                {
                    if (text.logical_start > text.logical_end || text.logical_end > _main->Length())
                        throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                    // Build navigation from the metrics just read for this cache miss.
                    result.metricToken = _navigation->AppendTextMetrics(static_cast<uint32_t>(text.logical_start),
                        static_cast<uint32_t>(text.logical_end), capturedMetrics, result.terminalBreak);
                    workspace.Put(keyBytes, valueBytes, cacheEpoch);
                }
            }
        }
        return result;
    }

    void NativeDocumentRenderer::MeasureHtmlTable(Presentation const &batch, notepads_markdown_preview_block const &block,
                                                 PreviewBlockVisual &result)
    {
        if (!block.columns || block.columns > 64 || !block.row_count || block.row_count > 256 ||
            block.cells.count != uint64_t{block.columns} * block.row_count || block.cells.count > 4096 ||
            !std::isfinite(block.width) || block.width <= 0 ||
            block.cells.start > batch.cells.size() || block.cells.count > batch.cells.size() - block.cells.start)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        result.htmlTable = true;
        result.tableWidth = static_cast<float>(block.width);
        if (!std::isfinite(result.tableWidth) || result.tableWidth <= 0)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        result.tableOwners.resize(static_cast<size_t>(block.cells.count));
        auto start = static_cast<size_t>(block.cells.start);
        auto count = static_cast<size_t>(block.cells.count);
        ValidatePreviewHtmlTable(std::span<notepads_markdown_preview_cell const>{batch.cells}.subspan(start, count),
                                 std::span<notepads_markdown_preview_html_cell const>{batch.htmlCells}.subspan(start, count),
                                 block.row_count, block.columns, result.tableOwners);
        result.rowHeights.assign(block.row_count, _theme.lineHeight + 12.0f);
        result.tableRowFlags.assign(block.row_count, 0);
        result.tableCells.reserve(static_cast<size_t>(block.cells.count));
        result.texts.reserve(static_cast<size_t>(block.cells.count));
        auto const widths = TableWidthPlan::Html(block.width, block.columns);
        auto padding = widths.padding;
        for (uint32_t column = 0; column <= block.columns; ++column)
            result.columnEdges.push_back(widths.Edge(column));
        auto textRow = [&](notepads_markdown_preview_cell const &cell) -> notepads_markdown_preview_text const &
        {
            if (cell.text >= batch.texts.size())
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            auto const &text = batch.texts[cell.text];
            if (text.node != cell.node || text.logical_start > text.logical_end ||
                text.logical_end > _main->Length() ||
                text.utf16_length != text.logical_end - text.logical_start ||
                (text.utf16_length == 0 && text.utf8.length != 0))
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            return text;
        };
        auto noDerived = [&](uint32_t text)
        {
            auto at = std::lower_bound(batch.derived.begin(), batch.derived.end(), text,
                                      [](auto const &value, uint32_t index) { return value.text < index; });
            return at == batch.derived.end() || at->text != text;
        };
        auto noScopes = [&](uint32_t text)
        {
            auto at = std::lower_bound(batch.scriptScopes.begin(), batch.scriptScopes.end(), text,
                                      [](auto const &value, uint32_t index) { return value.text < index; });
            return at == batch.scriptScopes.end() || at->text != text;
        };
        bool emptyCaption{};
        uint32_t captionStart{};
        std::optional<ReadingTableCaretAlias> coveredRows;
        std::optional<uint32_t> precedingCellEnd;
        auto resolveCoveredRows = [&](uint32_t target)
        {
            if (!coveredRows)
                return;
            coveredRows->target = target;
            result.tableAliases.push_back(*coveredRows);
            coveredRows.reset();
        };
        for (uint32_t row = 0; row < block.row_count; ++row)
        {
            auto rowStart = static_cast<size_t>(block.cells.start + uint64_t{row} * block.columns);
            auto caption = (batch.htmlCells[rowStart].flags & NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_CAPTION) != 0;
            result.tableRowFlags[row] = caption ? NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_CAPTION : 0;
            for (uint32_t column = 0; column < block.columns; ++column)
            {
                auto slot = rowStart + column;
                auto const &cell = batch.cells[slot];
                auto const &metadata = batch.htmlCells[slot];
                auto const &text = textRow(cell);
                if (metadata.flags & NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_COVERED)
                {
                    if (text.logical_start != text.logical_end || text.utf16_length != 0 || text.utf8.length != 0 ||
                        text.runs.count != 0 || text.images.count != 0 || !noDerived(cell.text) || !noScopes(cell.text))
                        throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                    if (result.tableOwners[slot - start] / block.columns < row)
                    {
                        auto at = static_cast<uint32_t>(text.logical_start);
                        if (coveredRows)
                            coveredRows->last = at;
                        else
                            coveredRows = ReadingTableCaretAlias{at, at, 0, ReadingTableAliasKind::RowSpan};
                    }
                    continue;
                }
                resolveCoveredRows(static_cast<uint32_t>(text.logical_start));
                precedingCellEnd = static_cast<uint32_t>(text.logical_end);
                auto omitted = caption && text.utf16_length == 0 && text.images.count == 0 && noDerived(cell.text);
                if (omitted && (text.runs.count != 0 || !noScopes(cell.text)))
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                PreviewTableCellVisual owner;
                owner.node = cell.node;
                owner.row = row;
                owner.column = column;
                owner.columnSpan = metadata.colspan;
                owner.rowSpan = metadata.rowspan;
                owner.batchTextIndex = cell.text;
                owner.flags = metadata.flags;
                owner.bounds.left = result.columnEdges[column];
                owner.bounds.right = result.columnEdges[column + metadata.colspan];
                if (omitted)
                {
                    emptyCaption = true;
                    result.rowHeights[0] = 0;
                    captionStart = static_cast<uint32_t>(text.logical_start);
                }
                if (!omitted && metadata.colspan > 1)
                {
                    auto lastCovered = textRow(batch.cells[slot + metadata.colspan - 1]).logical_start;
                    if (text.logical_end >= lastCovered)
                        throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                    result.tableAliases.push_back({static_cast<uint32_t>(text.logical_end + 1), static_cast<uint32_t>(lastCovered),
                                                   static_cast<uint32_t>(text.logical_end), ReadingTableAliasKind::ColumnSpan});
                }
                result.tableCells.push_back(owner);
            }
        }
        if (coveredRows)
        {
            if (!precedingCellEnd)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            resolveCoveredRows(*precedingCellEnd);
        }
        if (emptyCaption)
        {
            auto bodyStart = textRow(batch.cells[static_cast<size_t>(block.cells.start + block.columns)]).logical_start;
            if (bodyStart <= captionStart || bodyStart > UINT32_MAX)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            result.tableAliases.push_back({captionStart, static_cast<uint32_t>(bodyStart - 1),
                                           static_cast<uint32_t>(bodyStart), ReadingTableAliasKind::EmptyCaption});
        }
        for (auto const &alias : result.tableAliases)
            alias.Validate(*_main);
        // Ownership and covered payloads are valid before any measurement
        // can append cached navigation metrics or allocate a text layout.
        for (auto &owner : result.tableCells)
        {
            if (emptyCaption && owner.row == 0)
                continue;
            auto const &cell = batch.cells[static_cast<size_t>(block.cells.start +
                uint64_t{owner.row} * block.columns + owner.column)];
            auto measured = MeasureText(batch, batch.texts[owner.batchTextIndex],
                                        widths.TextWidth(owner.column, owner.columnSpan),
                                        _theme.fontSize, false, cell.header != 0, cell.alignment, false, false);
            measured.x = owner.bounds.left + padding;
            if (owner.rowSpan == 1)
                result.rowHeights[owner.row] = std::max(result.rowHeights[owner.row], measured.height + 12.0f);
            owner.textIndex = static_cast<uint32_t>(result.texts.size());
            result.texts.push_back(std::move(measured));
        }
        for (auto const &owner : result.tableCells)
        {
            if (owner.rowSpan <= 1)
                continue;
            auto end = owner.row + owner.rowSpan;
            float height{};
            for (auto row = owner.row; row < end; ++row)
                height += result.rowHeights[row];
            auto required = result.texts[owner.textIndex].height + 12.0f;
            result.rowHeights[end - 1] += std::max(0.0f, required - height);
        }
        std::array<float, 257> rowEdges{};
        for (uint32_t row = 0; row < block.row_count; ++row)
        {
            rowEdges[row + 1] = rowEdges[row] + result.rowHeights[row];
            if (!std::isfinite(rowEdges[row + 1]))
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
        }
        for (auto &owner : result.tableCells)
        {
            owner.bounds.top = rowEdges[owner.row];
            owner.bounds.bottom = rowEdges[owner.row + owner.rowSpan];
            if (owner.textIndex != NOTEPADS_MARKDOWN_PREVIEW_NONE)
                result.texts[owner.textIndex].y = owner.bounds.top + 6.0f;
        }
        result.measuredHeight = rowEdges[block.row_count];
    }

    namespace
    {
        constexpr uint32_t MaximumAuthoredCodeLine = 1'000'000'000;
        constexpr size_t MaximumDecoratedCodeLines = 32768;
        constexpr uint32_t MaximumDecoratedVisualRows = 131072;
        constexpr float CodeNumberGap = 8.0f;

        winrt::com_ptr<IDWriteTextFormat> CodeNumberFormat(PreviewTheme const &theme)
        {
            winrt::com_ptr<IDWriteTextFormat> format;
            winrt::check_hresult(SystemTextFactory()->CreateTextFormat(
                theme.codeFontFamily.c_str(), nullptr, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL, theme.fontSize, L"", format.put()));
            winrt::check_hresult(format->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP));
            winrt::check_hresult(format->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING));
            winrt::check_hresult(format->SetReadingDirection(DWRITE_READING_DIRECTION_LEFT_TO_RIGHT));
            winrt::check_hresult(format->SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, theme.lineHeight,
                                                          CodeBlockStyle::TextBaseline(theme.fontSize, theme.lineHeight)));
            return format;
        }
    }

    void NativeDocumentRenderer::PlaceCodeLines(PreviewBlockVisual &visual,
                                                 std::span<std::array<uint32_t, 2> const> highlights)
    {
        auto numberStart = visual.codeGutterWidth > 0 ? visual.block.code_first_line_number : 0;
        uint32_t physicalLine = 1;
        bool beginsPhysicalLine = true;
        bool lastRecorded = false;
        for (auto const &text : visual.texts)
        {
            if (!text.layout)
                continue;
            UINT32 count{};
            auto status = text.layout->GetLineMetrics(nullptr, 0, &count);
            if (FAILED(status) && status != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                winrt::check_hresult(status);
            if (count > MaximumDecoratedVisualRows)
            {
                visual.codeLines.clear();
                return;
            }
            PreviewVector<DWRITE_LINE_METRICS> lines{PreviewAllocator<DWRITE_LINE_METRICS>{_layout}};
            lines.resize(count);
            if (count)
                winrt::check_hresult(text.layout->GetLineMetrics(lines.data(), count, &count));
            lines.resize(count);
            uint64_t offset = text.logicalStart;
            const auto main = _main->Text();
            if (offset > text.logicalEnd || text.logicalEnd > main.size())
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            float top = text.y;
            const auto textBottom = text.y + text.height;
            for (auto const &line : lines)
            {
                if (top >= textBottom - 0.01f)
                    break; // DWrite's zero-length row after a terminal line break.
                if (!std::isfinite(line.height) || line.height <= 0 || line.length > text.logicalEnd - offset)
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                auto bottom = std::min(top + line.height, textBottom);
                if (beginsPhysicalLine)
                {
                    auto highlight = std::any_of(highlights.begin(), highlights.end(), [&](auto const &range)
                        { return range[0] <= physicalLine && physicalLine <= range[1]; });
                    lastRecorded = highlight || numberStart;
                    if (lastRecorded)
                        visual.codeLines.push_back({top, bottom, numberStart ? numberStart + physicalLine - 1 : 0, highlight});
                }
                else if (lastRecorded)
                    visual.codeLines.back().bottom = bottom;
                top += line.height;
                offset += line.length;
                // A Unicode line separator can wrap DWrite without ending an authored Markdown line.
                beginsPhysicalLine = line.length != 0 && main[static_cast<size_t>(offset - 1)] == L'\n';
                if (beginsPhysicalLine)
                    ++physicalLine;
            }
        }
    }

    PreviewBlockVisual NativeDocumentRenderer::MeasureBlock(Presentation const &batch, notepads_markdown_preview_block const &block)
    {
        PreviewBlockVisual result{_layout};
        result.block = block;
        if (block.code_reserved || block.code_first_line_number > MaximumAuthoredCodeLine ||
            (block.kind != 3 && (block.code_first_line_number || block.code_highlight_lines.length)))
            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        auto index = static_cast<size_t>(&block - batch.blocks.data());
        if (index >= batch.geometry.size())
            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
        result.markerX = batch.geometry[index].marker_x;
        if (block.decorations.start > batch.decorations.size() ||
            block.decorations.count > batch.decorations.size() - block.decorations.start)
            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        result.decorations.assign(batch.decorations.begin() + static_cast<size_t>(block.decorations.start),
                                  batch.decorations.begin() +
                                      static_cast<size_t>(block.decorations.start + block.decorations.count));
        for (auto const &decoration : result.decorations)
            ValidatePreviewDecoration(decoration);
        result.markers.resize(result.decorations.size());
        auto finishMarkers = [&]
        {
            if (!result.decorations.empty())
                result.measuredHeight = std::max(result.measuredHeight, static_cast<double>(_theme.lineHeight));
            // Image metadata can change a block's height without changing any
            // later text. Reserve its trailing whitespace in complete physical
            // pixels, so that replacement cannot change every later glyph phase.
            // Internal line/image/row geometry and actual image dimensions stay
            // untouched; only the block's submitted trailing extent grows.
            if (std::any_of(result.texts.begin(), result.texts.end(),
                            [](auto const &text) { return !text.images.empty(); }))
            {
                const auto scale = _theme.physicalScale;
                if (!(scale > 0) || !std::isfinite(scale))
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
                result.measuredHeight = std::ceil(result.measuredHeight * scale) / scale;
                if (!std::isfinite(result.measuredHeight))
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
            }
            // Measurement collects semantic/task facts but needs no throwaway marker COM objects.
            // Frame creation has the actual body layout, including script baselines.
            if (!_measuring || (result.texts.empty() && !result.decorations.empty()))
                PlaceListMarkers(result);
        };
        if (block.kind == 5)
        {
            result.measuredHeight = 16.0;
            finishMarkers();
            return result;
        }
        if (block.kind == 7)
        {
            result.measuredHeight = _theme.lineHeight;
            finishMarkers();
            return result;
        }
        if (block.kind == 6)
        {
            if (!block.columns || !block.row_count || !block.cells.count || block.cells.start > batch.cells.size() ||
                block.cells.count > batch.cells.size() - block.cells.start)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            if (batch.htmlCells[static_cast<size_t>(block.cells.start)].flags & NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_HTML)
            {
                MeasureHtmlTable(batch, block, result);
                finishMarkers();
                return result;
            }
            for (uint64_t i = block.cells.start; i < block.cells.start + block.cells.count; ++i)
                if (batch.htmlCells[static_cast<size_t>(i)].flags != 0)
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            result.rowHeights.assign(block.row_count, _theme.lineHeight + 12.0f);
            result.texts.reserve(static_cast<size_t>(block.cells.count));
            // Row headers are used by front matter. Ordinary Markdown tables
            // keep their existing first-row headers and equal-width geometry.
            auto const tableCells = std::span<notepads_markdown_preview_cell const>{batch.cells}.subspan(
                static_cast<size_t>(block.cells.start), static_cast<size_t>(block.cells.count));
            const bool rowHeaders = TableHasRowHeaders(tableCells, block.columns, block.row_count);
            for (uint64_t i = block.cells.start; i < block.cells.start + block.cells.count; ++i)
            {
                auto const &cell = batch.cells[static_cast<size_t>(i)];
                if (cell.text >= batch.texts.size() || cell.row >= block.row_count || cell.column >= block.columns)
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            }
            auto const tableWidth = static_cast<float>(block.width);
            // At narrow widths padding yields first, so both columns retain
            // space for a normal glyph whenever the viewport can fit two.
            auto const minimumContent = std::min(_theme.fontSize * 1.25f, tableWidth * 0.5f);
            float horizontalPadding = rowHeaders
                ? std::min(13.0f, std::max(0.0f, (tableWidth - 2 * minimumContent) * 0.25f)) : 6.0f;
            float naturalKeyWidth{};
            if (rowHeaders)
            {
                winrt::com_ptr<IDWriteTextFormat> keyFormat;
                winrt::check_hresult(SystemTextFactory()->CreateTextFormat(_theme.fontFamily.c_str(), nullptr,
                    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL,
                    _theme.fontSize, L"", keyFormat.put()));
                winrt::check_hresult(keyFormat->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP));
                for (uint64_t i = block.cells.start; i < block.cells.start + block.cells.count; ++i)
                {
                    auto const &cell = batch.cells[static_cast<size_t>(i)];
                    if (cell.column != 0)
                        continue;
                    auto content = batch.Text(batch.texts[cell.text].utf8);
                    if (content.size() > 4096)
                    {
                        // An unusually long key is measured once by the real
                        // wrapping layout below; avoid a duplicate large layout.
                        naturalKeyWidth = static_cast<float>(block.width);
                        continue;
                    }
                    winrt::com_ptr<IDWriteTextLayout> keyLayout;
                    winrt::check_hresult(SystemTextFactory()->CreateTextLayout(content.data(), static_cast<uint32_t>(content.size()),
                        keyFormat.get(), std::max(1.0f, static_cast<float>(block.width)), _theme.lineHeight, keyLayout.put()));
                    DWRITE_TEXT_METRICS metrics{};
                    winrt::check_hresult(keyLayout->GetMetrics(&metrics));
                    naturalKeyWidth = std::max(naturalKeyWidth, metrics.widthIncludingTrailingWhitespace + 2 * horizontalPadding);
                }
            }
            auto const minimumColumn = minimumContent + 2 * horizontalPadding;
            float keyWidth = rowHeaders ? std::clamp(naturalKeyWidth, minimumColumn,
                std::max(minimumColumn, std::min(tableWidth - minimumColumn, tableWidth * 0.45f))) : 0;
            auto const widths = TableWidthPlan::Markdown(block.width, block.columns, horizontalPadding, rowHeaders, keyWidth);
            for (uint32_t column = 0; column <= block.columns; ++column)
                result.columnEdges.push_back(widths.Edge(column));
            for (uint64_t i = block.cells.start; i < block.cells.start + block.cells.count; ++i)
            {
                auto const &cell = batch.cells[static_cast<size_t>(i)];
                if (cell.text >= batch.texts.size() || cell.row >= block.row_count || cell.column >= block.columns)
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
                auto text = MeasureText(batch, batch.texts[cell.text], widths.TextWidth(cell.column),
                                        _theme.fontSize, false, cell.header != 0, cell.alignment, false, false,
                                        rowHeaders && cell.column == 0 && naturalKeyWidth <= keyWidth);
                text.x = result.columnEdges[cell.column] + horizontalPadding;
                result.rowHeights[cell.row] = std::max(result.rowHeights[cell.row], text.height + 12.0f);
                result.texts.push_back(std::move(text));
            }
            PreviewVector<float> tops{PreviewAllocator<float>{_layout}};
            tops.reserve(block.row_count);
            float y{};
            for (auto height : result.rowHeights)
            {
                tops.push_back(y);
                y += height;
            }
            for (size_t i = 0; i < result.texts.size(); ++i)
            {
                result.texts[i].y = tops[batch.cells[static_cast<size_t>(block.cells.start) + i].row] + 6.0f;
            }
            result.measuredHeight = y;
        }
        else
        {
            if (block.texts.start > batch.texts.size() || block.texts.count > batch.texts.size() - block.texts.start)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            bool code = block.kind == 3;
            const bool htmlFallback = block.kind == 4;
            float scale = 1.0f;
            if (block.kind == 2)
            {
                constexpr float sizes[]{2.0f, 1.6f, 1.35f, 1.15f, 1.0f, 0.95f};
                if (block.heading_level < 1 || block.heading_level > 6)
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
                scale = sizes[block.heading_level - 1];
            }
            const auto width = static_cast<float>(block.width);
            const auto fallbackInset = htmlFallback ? std::min(8.0f, width * 0.25f) : 0.0f;
            const auto padding = code ? CodeBlockStyle::padding : fallbackInset;
            auto left = code ? CodeBlockStyle::LeftPadding(width) : fallbackInset;
            const auto right = code ? CodeBlockStyle::RightPadding(width) : fallbackInset;
            if (_visibility && _visibility->SummaryIndex(block.node) && width >= 24.0f)
            {
                result.disclosureInset = 12.0f;
                left += result.disclosureInset;
            }
            std::array<std::array<uint32_t, 2>, 64> highlightStorage{};
            std::span<std::array<uint32_t, 2> const> codeHighlights;
            if (code && (block.code_first_line_number || block.code_highlight_lines.length))
            {
                const auto slice = block.code_highlight_lines;
                if (slice.start > batch.blob.size() || slice.length > batch.blob.size() - slice.start ||
                    slice.length > sizeof(highlightStorage) || slice.length % sizeof(highlightStorage.front()))
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                if (slice.length)
                {
                    static_assert(std::endian::native == std::endian::little);
                    std::memcpy(highlightStorage.data(), batch.blob.data() + slice.start, static_cast<size_t>(slice.length));
                    codeHighlights = {highlightStorage.data(), static_cast<size_t>(slice.length) / sizeof(highlightStorage.front())};
                    for (auto const &range : codeHighlights)
                        if (!range[0] || range[0] > range[1] || range[1] > MaximumAuthoredCodeLine)
                            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                }
                if (block.logical_start > block.logical_end || block.logical_end > _main->Length())
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                auto body = _main->Text().substr(block.logical_start, block.logical_end - block.logical_start);
                auto lineCount = static_cast<size_t>(std::count(body.begin(), body.end(), L'\n'));
                if (!body.empty() && body.back() != L'\n')
                    ++lineCount;
                if (lineCount > MaximumDecoratedCodeLines)
                    codeHighlights = {};
                else if (block.code_first_line_number && lineCount)
                {
                    auto digits = static_cast<uint32_t>(std::to_wstring(block.code_first_line_number + lineCount - 1).size());
                    if (!_codeDigitWidth)
                    {
                        auto format = CodeNumberFormat(_theme);
                        winrt::com_ptr<IDWriteTextLayout> sample;
                        winrt::check_hresult(SystemTextFactory()->CreateTextLayout(
                            L"0123456789", 10, format.get(), 2048, _theme.lineHeight, sample.put()));
                        std::array<DWRITE_CLUSTER_METRICS, 10> clusters{};
                        uint32_t count{};
                        winrt::check_hresult(sample->GetClusterMetrics(clusters.data(), static_cast<uint32_t>(clusters.size()), &count));
                        float digitWidth{};
                        for (uint32_t i = 0; i < count; ++i)
                            digitWidth = std::max(digitWidth, clusters[i].width);
                        _codeDigitWidth = digitWidth;
                    }
                    const auto gutter = std::ceil(*_codeDigitWidth * digits) + 2 * CodeNumberGap;
                    const auto admitted = std::min(gutter, std::max(0.0f, width - left - right - 2 * _theme.fontSize));
                    if (admitted == gutter)
                    {
                        left += gutter;
                        result.codeGutterWidth = gutter;
                    }
                }
            }
            const auto textWidth = code ? width - left - right : OrdinaryTextWidth(width, htmlFallback,
                _visibility && _visibility->SummaryIndex(block.node));
            float y = padding;
            for (uint64_t i = block.texts.start; i < block.texts.start + block.texts.count; ++i)
            {
                auto text = MeasureText(batch, batch.texts[static_cast<size_t>(i)], textWidth,
                                        _theme.fontSize * scale, code || htmlFallback, block.kind == 2,
                                        block.alignment, code, block.kind == 2, false, block.quote_depth != 0 && !code);
                text.x = left;
                text.y = y;
                y += text.height;
                result.texts.push_back(std::move(text));
            }
            if (code && !_measuring && (result.codeGutterWidth > 0 || !codeHighlights.empty()))
                PlaceCodeLines(result, codeHighlights);
            const auto minimumHeight = block.kind == 2 ? _theme.fontSize * scale * 1.25f : _theme.lineHeight;
            result.measuredHeight = std::max<double>(minimumHeight, y + padding);
        }
        finishMarkers();
        if (!std::isfinite(result.measuredHeight))
            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
        return result;
    }

    void NativeDocumentRenderer::PlaceListMarkers(PreviewBlockVisual &visual)
    {
        if (visual.decorations.empty())
            return;
        float baseline = _theme.fontSize;
        if (!visual.texts.empty())
        {
            auto const &text = visual.texts.front();
            if (text.scripts && !text.scripts->FinalLineMetrics().empty())
                baseline = text.y + text.scripts->FinalLineMetrics().front().baseline;
            else if (text.layout)
            {
                UINT32 count{};
                auto status = text.layout->GetLineMetrics(nullptr, 0, &count);
                if (FAILED(status) && status != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                    winrt::check_hresult(status);
                PreviewVector<DWRITE_LINE_METRICS> lines{PreviewAllocator<DWRITE_LINE_METRICS>{_layout}};
                lines.resize(count);
                if (count)
                {
                    winrt::check_hresult(text.layout->GetLineMetrics(lines.data(), count, &count));
                    baseline = text.y + lines.front().baseline;
                }
            }
        }
        // Align the box with the first line's ink, including when a tall
        // inline object shifts the baseline.
        visual.taskMarkerTop = std::clamp(baseline - 0.36f * _theme.fontSize - 7.0f, 0.0f,
                                         std::max(0.0f, static_cast<float>(visual.measuredHeight) - 14.0f));
        // Geometry::enter preserves at least min(documentWidth, indent) for the body.
        // Descendant quotes only shrink its right edge. This lower bound on the
        // owning list's advance avoids borrowing a quote's left inset after clamp.
        auto const &block = visual.block;
        auto safeAdvance = std::min(_key.indent, std::max(0.0,
            block.x + block.width - visual.markerX - std::min(_key.width, _key.indent)));
        safeAdvance = std::min(safeAdvance, std::max(0.0, block.x - visual.markerX));
        auto available = std::max(0.0, safeAdvance - 6.0);
        auto clipWidth = static_cast<float>(available);
        if (static_cast<double>(clipWidth) > available)
            clipWidth = std::nextafter(clipWidth, 0.0f);
        for (size_t i = 0; i < visual.decorations.size(); ++i)
        {
            auto const &decoration = visual.decorations[i];
            if (_measuring || IsPreviewTaskDecoration(decoration.kind))
                continue;
            auto &marker = visual.markers[i];
            marker.clipWidth = clipWidth;
            if (clipWidth <= 0)
            {
                marker.display = ListMarkerDisplay::Clipped;
                continue;
            }
            if (!_markerFormat)
            {
                _markerFormat = MarkerFormat(SystemTextFactory(), _theme);
                winrt::check_hresult(_markerFormat->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING));
            }
            std::array<wchar_t, 21> value{};
            auto length = static_cast<uint32_t>(MarkerText(decoration, value));
            marker.x = _markerLeftGuard;
            // Both guards participate in the layout box, not just the paint clip.
            // A width that cannot hold the full ink must also force CHARACTER
            // trimming; merely clipping a negative left side-bearing is insufficient.
            auto layoutWidth = std::max(0.01f, clipWidth - _markerLeftGuard - _markerRightGuard);
            ++_listMarkerStats.frameLayouts;
            winrt::check_hresult(SystemTextFactory()->CreateTextLayout(value.data(), length, _markerFormat.get(),
                layoutWidth, _theme.lineHeight, marker.layout.put()));
            DWRITE_OVERHANG_METRICS overhang{};
            winrt::check_hresult(marker.layout->GetOverhangMetrics(&overhang));
            if (!std::isfinite(overhang.left) || !std::isfinite(overhang.right))
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            bool full = marker.x - overhang.left >= 0 && marker.x + layoutWidth + overhang.right <= clipWidth;
            if (!full)
            {
                // A short marker can have a larger own side-bearing than the
                // multi-line batch's bounding ink. Recover its tight guards
                // from this finite trailing box before enabling trimming.
                DWRITE_TEXT_METRICS metrics{};
                winrt::check_hresult(marker.layout->GetMetrics(&metrics));
                if (!std::isfinite(metrics.widthIncludingTrailingWhitespace) || metrics.widthIncludingTrailingWhitespace <= 0)
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                auto ownLeft = RoundMarkerWidth(std::max(0.0f,
                    overhang.left + layoutWidth - metrics.widthIncludingTrailingWhitespace));
                auto ownRight = RoundMarkerWidth(std::max(0.0f, overhang.right));
                marker.x = ownLeft;
                layoutWidth = std::max(0.01f, clipWidth - ownLeft - ownRight);
                winrt::check_hresult(marker.layout->SetMaxWidth(layoutWidth));
                if (!_markerEllipsis)
                    winrt::check_hresult(SystemTextFactory()->CreateEllipsisTrimmingSign(_markerFormat.get(), _markerEllipsis.put()));
                DWRITE_INLINE_OBJECT_METRICS sign{};
                winrt::check_hresult(_markerEllipsis->GetMetrics(&sign));
                marker.display = ListMarkerDisplay::Clipped;
                if (std::isfinite(sign.width) && sign.width > 0 && sign.width <= layoutWidth)
                {
                    DWRITE_TRIMMING trimming{DWRITE_TRIMMING_GRANULARITY_CHARACTER, 0, 0};
                    winrt::check_hresult(marker.layout->SetTrimming(&trimming, _markerEllipsis.get()));
                }
            }
            DWRITE_LINE_METRICS line{};
            UINT32 lines{};
            winrt::check_hresult(marker.layout->GetLineMetrics(&line, 1, &lines));
            if (lines != 1 || !std::isfinite(line.baseline))
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            if (!full && line.isTrimmed)
                marker.display = ListMarkerDisplay::Trimmed;
            marker.y = baseline - line.baseline;
        }
    }

    void NativeDocumentRenderer::CollectNavigation(notepads_markdown_preview_block const &block,
                                         PreviewBlockVisual const &visual, uint32_t blockIndex)
    {
        auto logicalStart = static_cast<uint32_t>(block.logical_start);
        auto logicalEnd = static_cast<uint32_t>(block.logical_end);
        if (block.logical_start > block.logical_end || block.logical_end > _main->Length())
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        bool hasTask{};
        for (auto const &decoration : visual.decorations)
        {
            if (!IsPreviewTaskDecoration(decoration.kind))
                continue;
            if (decoration.logical_start > UINT32_MAX || decoration.logical_end != decoration.logical_start + 1 ||
                decoration.logical_end > _main->Length())
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            hasTask = true;
            logicalStart = std::min(logicalStart, static_cast<uint32_t>(decoration.logical_start));
            logicalEnd = std::max(logicalEnd, static_cast<uint32_t>(decoration.logical_end));
            if (visual.texts.empty())
                _navigation->AddObjectLine(blockIndex, static_cast<uint32_t>(decoration.logical_start),
                                           static_cast<uint32_t>(decoration.logical_end),
                                           MarkerOrigin(visual) + 6, visual.taskMarkerTop, 14);
        }
        if (block.kind != 6)
            _navigation->AddParagraph(logicalStart, logicalEnd);
        uint32_t textIndex{};
        for (auto const &text : visual.texts)
        {
            auto prefixStart = textIndex == 0 && hasTask ? logicalStart : NOTEPADS_MARKDOWN_PREVIEW_NONE;
            if (text.metricToken)
                _navigation->BindTextMetrics(text.metricToken, blockIndex, static_cast<uint32_t>(text.logicalStart),
                                             static_cast<uint32_t>(text.logicalEnd),
                                             text.x + static_cast<float>(block.x), text.y,
                                             prefixStart, block.kind == 6, textIndex);
            else
                _navigation->AddText(blockIndex, static_cast<uint32_t>(text.logicalStart),
                                     static_cast<uint32_t>(text.logicalEnd), text.layout.get(),
                                     text.x + static_cast<float>(block.x), text.y,
                                     prefixStart, block.kind == 6, text.terminalBreak,
                                     text.scripts ? text.scripts->FinalLineMetrics() : std::span<DWRITE_LINE_METRICS const>{},
                                     text.scripts ? text.scripts->RubyCaretStops() : std::span<uint32_t const>{}, textIndex,
                                     text.scripts ? text.scripts->LineOverflowAbove() : 0,
                                     text.scripts ? text.scripts->LineOverflowBelow() : 0);
            ++textIndex;
            if (block.kind == 6)
                _navigation->AddParagraph(static_cast<uint32_t>(text.logicalStart),
                                          static_cast<uint32_t>(text.logicalEnd));
        }
        for (auto const &alias : visual.tableAliases)
            _navigation->AddTableCaretAlias(alias);
    }

    void NativeDocumentRenderer::CollectSemantics(Presentation const &batch, notepads_markdown_preview_block const &block)
    {
        if (block.logical_start > block.logical_end || block.logical_end > _main->Length())
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        if (block.kind == 6 && block.cells.count)
        {
            auto first = static_cast<size_t>(block.cells.start);
            auto const &metadata = batch.htmlCells.at(first);
            if (metadata.flags & NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_CAPTION)
            {
                auto const &cell = batch.cells.at(first);
                ValidatePreviewHtmlCell(cell, metadata);
                auto const &caption = batch.texts.at(cell.text);
                if (metadata.flags & NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_COVERED ||
                    caption.logical_start != block.logical_start || caption.logical_start > caption.logical_end ||
                    caption.logical_end > block.logical_end)
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                if (caption.logical_start != caption.logical_end)
                {
                    _semanticBuilder->tables.push_back({
                        block.node,
                        {static_cast<uint32_t>(block.logical_start), static_cast<uint32_t>(block.logical_end)},
                        {static_cast<uint32_t>(caption.logical_start), static_cast<uint32_t>(caption.logical_end)}});
                }
            }
        }
        for (uint64_t i = block.decorations.start; i < uint64_t{block.decorations.start} + block.decorations.count; ++i)
        {
            auto const &decoration = batch.decorations.at(static_cast<size_t>(i));
            if (!IsPreviewTaskDecoration(decoration.kind))
                continue;
            if (decoration.logical_start >= _main->Length() || decoration.logical_end != decoration.logical_start + 1)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            ReadingObjectDescriptor object{_reading->memory};
            object.kind = ReadingObjectKind::Task;
            object.object = decoration.object;
            object.node = decoration.item;
            object.checked = decoration.checked != 0;
            object.staticHtmlCheckbox = decoration.kind == NOTEPADS_MARKDOWN_PREVIEW_DECORATION_STATIC_HTML_CHECKBOX;
            if (decoration.checked > 1 || decoration.has_action > 1 || decoration.reserved != 0)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            if (decoration.has_action)
            {
                // Core validates this action against its original UTF8
                // source. Retain the exact wire fact, never infer a marker
                // from Main text or the visual checkbox geometry.
                auto expected = decoration.expected;
                auto middle = static_cast<uint8_t>(expected >> 8);
                if (decoration.action_start > UINT64_MAX - 3 ||
                    decoration.action_end != decoration.action_start + 3 || (expected & 0xFF000000u) != 0 ||
                    (expected & 0xFFu) != '[' || ((expected >> 16) & 0xFFu) != ']' ||
                    (middle != ' ' && middle != 'x' && middle != 'X') || object.checked != (middle != ' '))
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                object.hasTaskAction = true;
                object.taskActionStart = decoration.action_start;
                object.taskActionEnd = decoration.action_end;
                object.taskExpected = {'[', middle, ']'};
            }
            object.text = {static_cast<uint32_t>(decoration.logical_start),
                           static_cast<uint32_t>(decoration.logical_end)};
            if (object.staticHtmlCheckbox)
            {
                // This is projected display text from the same immutable
                // Main, not raw HTML or an unfunded temporary string.
                auto start = std::max(object.text.end, static_cast<uint32_t>(block.logical_start));
                auto end = static_cast<uint32_t>(block.logical_end);
                if (start > end)
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                auto text = _main->Text();
                while (start < end && (text[start] == L' ' || text[start] == L'\t' ||
                                      text[start] == L'\r' || text[start] == L'\n'))
                    ++start;
                while (end > start && (text[end - 1] == L' ' || text[end - 1] == L'\t' ||
                                      text[end - 1] == L'\r' || text[end - 1] == L'\n'))
                    --end;
                object.labelText = {start, end};
            }
            _semanticBuilder->objects.push_back(std::move(object));
        }
        auto collectText = [&](uint64_t i)
        {
            auto const &text = batch.texts.at(static_cast<size_t>(i));
            for (uint64_t j = text.runs.start; j < text.runs.start + text.runs.count; ++j)
            {
                auto const &run = batch.runs.at(static_cast<size_t>(j));
                if (run.link == NOTEPADS_MARKDOWN_PREVIEW_NONE)
                    continue;
                auto const &link = batch.links.at(run.link);
                ReadingLinkDescriptor descriptor{_reading->memory};
                descriptor.node = link.node;
                if (link.local_position_plus_one)
                    descriptor.localPosition = link.local_position_plus_one - 1;
                descriptor.localHeadingNode = link.local_heading_node;
                if (link.wiki > 1)
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                descriptor.wiki = link.wiki != 0;
                descriptor.text = {static_cast<uint32_t>(text.logical_start + run.start),
                                   static_cast<uint32_t>(text.logical_start + run.end)};
                auto destination = batch.Text(link.destination);
                auto title = batch.Text(link.title);
                descriptor.destination.assign(destination.data(), destination.size());
                descriptor.title.assign(title.data(), title.size());
                _semanticBuilder->links.push_back(std::move(descriptor));
            }
            for (uint64_t j = text.images.start; j < text.images.start + text.images.count; ++j)
            {
                auto const &image = batch.images.at(static_cast<size_t>(j));
                ReadingObjectDescriptor descriptor{_reading->memory};
                descriptor.kind = ReadingObjectKind::Image;
                descriptor.object = image.object;
                descriptor.node = image.node;
                auto position = text.logical_start + image.offset_utf16;
                if (position >= _main->Length())
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                descriptor.text = {static_cast<uint32_t>(position), static_cast<uint32_t>(position + 1)};
                descriptor.imageHasFixedSize = image.width && image.height;
                auto scope = std::lower_bound(batch.scriptScopes.begin(), batch.scriptScopes.end(), i,
                    [](auto const &row, uint64_t index) { return row.text < index; });
                for (; scope != batch.scriptScopes.end() && scope->text == i; ++scope)
                {
                    if (!(scope->font_flags & NOTEPADS_MARKDOWN_PREVIEW_SCRIPT_SHAPE_ONLY))
                    {
                        descriptor.imageReadinessAffectsText = true;
                        break;
                    }
                }
                auto ruby = std::lower_bound(batch.rubyUnits.begin(), batch.rubyUnits.end(), i,
                    [](auto const &unit, uint64_t index) { return unit.text < index; });
                descriptor.imageReadinessAffectsText |= ruby != batch.rubyUnits.end() && ruby->text == i;
                auto alt = batch.Text(image.alt);
                descriptor.alt.assign(alt.data(), alt.size());
                auto destination = batch.Text(image.destination);
                auto title = batch.Text(image.title);
                auto light = batch.Text(image.light_source);
                auto dark = batch.Text(image.dark_source);
                descriptor.destination.assign(destination.data(), destination.size());
                descriptor.title.assign(title.data(), title.size());
                descriptor.lightSource.assign(light.data(), light.size());
                descriptor.darkSource.assign(dark.data(), dark.size());
                _semanticBuilder->objects.push_back(std::move(descriptor));
            }
            // Rows are ordered by batch-local text and UTF16 offset. Main stays
            // atomic; the two complete strings belong only to copy/object UIA.
            auto first = std::lower_bound(batch.derived.begin(), batch.derived.end(), i,
                                          [](notepads_markdown_derived_row const &row, uint64_t index) { return row.text < index; });
            for (auto row = first; row != batch.derived.end() && row->text == i; ++row)
            {
                if (text.logical_start > text.logical_end || text.logical_end > _main->Length() ||
                    row->offset_utf16 >= text.logical_end - text.logical_start)
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                auto position = text.logical_start + row->offset_utf16;
                if (position >= _main->Length() ||
                    _main->Text()[static_cast<size_t>(position)] != L'\xfffc')
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                ReadingObjectDescriptor descriptor{_reading->memory};
                descriptor.object = row->object;
                descriptor.node = row->node;
                descriptor.kind = row->kind == NOTEPADS_MARKDOWN_DERIVED_MATH ? ReadingObjectKind::Math
                                : row->kind == NOTEPADS_MARKDOWN_DERIVED_DIAGRAM ? ReadingObjectKind::Diagram
                                                                               : ReadingObjectKind::NoteEmbed;
                descriptor.syntax = row->syntax;
                descriptor.text = {static_cast<uint32_t>(position), static_cast<uint32_t>(position + 1)};
                auto input = batch.Text(row->input);
                auto source = batch.Text(row->export_source);
                descriptor.input.assign(input.data(), input.size());
                descriptor.source.assign(source.data(), source.size());
                if (descriptor.kind == ReadingObjectKind::Diagram)
                {
                    auto title = batch.Text(row->accessible_title);
                    auto description = batch.Text(row->accessible_description);
                    descriptor.title.assign(title.data(), title.size());
                    descriptor.label.assign(description.data(), description.size());
                }
                _semanticBuilder->objects.push_back(std::move(descriptor));
            }
        };
        for (uint64_t i = block.texts.start; i < uint64_t{block.texts.start} + block.texts.count; ++i)
            collectText(i);
    }

    void NativeDocumentRenderer::ReadVisibility()
    {
        auto error = CorePod<notepads_markdown_core_error>();
        auto summary = PreviewPod<notepads_markdown_preview_visibility_result>();
        Check(notepads_markdown_preview_visibility(_layout, &summary, &error), L"visibility summary");
        if (std::memcmp(&summary.key, &_key, sizeof(_key)) != 0 || summary.hidden_ranges > summary.disclosures)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        if (!summary.disclosures)
            return;
        auto model = CorePod<notepads_markdown_core_model_summary_result>();
        Check(notepads_markdown_core_model_summary(_reading->memory->ModelOwner().Get(), &model, &error), L"visibility model");
        if (summary.disclosures > model.nodes)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        auto memory = _reading->memory;
        auto visibility = std::allocate_shared<ReadingVisibility>(ReadingAllocator<ReadingVisibility>{memory}, memory);
        auto read = [&](auto *rows, uint32_t table, uint64_t offset, uint64_t total)
        {
            auto count = std::min<uint64_t>(256, total - offset);
            auto result = CorePod<notepads_markdown_core_read_result>();
            Check(notepads_markdown_preview_read(_layout, table, offset, count, sizeof(*rows), rows, &result, &error), L"visibility rows");
            if (result.written != count || result.next != offset + count || result.reserved || result.done != (result.next == total))
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            return result.next;
        };
        visibility->disclosures.resize(static_cast<size_t>(summary.disclosures));
        for (uint64_t offset = 0; offset < summary.disclosures;)
            offset = read(visibility->disclosures.data() + offset, NOTEPADS_MARKDOWN_PREVIEW_DISCLOSURES, offset, summary.disclosures);
        for (size_t i = 0; i < visibility->disclosures.size(); ++i)
        {
            auto const &row = visibility->disclosures[i];
            if (!row.node || row.node > row.summary || row.summary >= row.first_body_node || row.first_body_node > row.next_node ||
                row.next_node > model.nodes + 1 || row.expanded > 1 || row.reserved ||
                row.summary_start > row.body_start || row.body_start > row.body_end || row.body_end > _main->Length() ||
                (i && (visibility->disclosures[i - 1].node >= row.node || visibility->disclosures[i - 1].summary >= row.summary ||
                       visibility->disclosures[i - 1].body_start > row.body_start)))
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            _main->ValidateRange(static_cast<uint32_t>(row.summary_start), static_cast<uint32_t>(row.body_end - row.summary_start));
            _main->ValidateRange(static_cast<uint32_t>(row.body_start), 0);
        }
        visibility->hidden.reserve(static_cast<size_t>(summary.hidden_ranges));
        std::array<notepads_markdown_preview_hidden_range, 256> buffer{};
        for (uint64_t offset = 0; offset < summary.hidden_ranges;)
        {
            auto next = read(buffer.data(), NOTEPADS_MARKDOWN_PREVIEW_HIDDEN_RANGES, offset, summary.hidden_ranges);
            for (size_t i = 0; i < next - offset; ++i)
            {
                auto const &row = buffer[i];
                if (row.start >= row.end || row.end > _main->Length() ||
                    (!visibility->hidden.empty() && visibility->hidden.back().end >= row.start))
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                _main->ValidateRange(static_cast<uint32_t>(row.start), static_cast<uint32_t>(row.end - row.start));
                visibility->hidden.push_back({static_cast<uint32_t>(row.start), static_cast<uint32_t>(row.end)});
            }
            offset = next;
        }
        _visibility = std::move(visibility);
    }

    bool NativeDocumentRenderer::ReadUnfilteredSemantics(PreviewImageWidths *imageWidths)
    {
        bool provenWidths = true;
        auto error = CorePod<notepads_markdown_core_error>();
        notepads_markdown_preview_layout *handle{};
        Check(notepads_markdown_preview_layout_create(_reading->memory->ModelOwner().Get(), _cancel->Get(), &_key, nullptr, 0,
                  NOTEPADS_MARKDOWN_PREVIEW_LAYOUT_SEMANTICS, _section ? _section->heading : 0,
                  &handle, &error), L"semantic projection create");
        std::unique_ptr<notepads_markdown_preview_layout, decltype(&notepads_markdown_preview_layout_destroy)>
            projection{handle, notepads_markdown_preview_layout_destroy};
        auto total = PreviewPod<notepads_markdown_preview_visibility_result>();
        Check(notepads_markdown_preview_visibility(handle, &total, &error), L"semantic projection count");
        if (std::memcmp(&total.key, &_key, sizeof(_key)) != 0 || total.disclosures || total.hidden_ranges)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        for (uint64_t start = 0; start < total.blocks;)
        {
            auto count = static_cast<uint32_t>(std::min<uint64_t>(32, total.blocks - start));
            auto summary = PreviewPod<notepads_markdown_preview_summary>();
            Check(notepads_markdown_preview_semantic_batch(handle, start, count, &summary, &error), L"semantic projection batch");
            if (summary.start != start || summary.blocks != count)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            auto batch = Read(summary, handle);
            for (auto const &block : batch.blocks)
            {
                CollectSemantics(batch, block);
                if (!imageWidths)
                    continue;
                auto const texts = std::span<notepads_markdown_preview_text const>{batch.texts}.subspan(
                    static_cast<size_t>(block.texts.start), static_cast<size_t>(block.texts.count));
                if (std::none_of(texts.begin(), texts.end(), [](auto const &text) { return text.images.count != 0; }))
                    continue;
                auto appendWidths = [&](auto const &text, float width)
                {
                    if (!std::isfinite(width))
                        throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                    for (uint64_t image = text.images.start; image < uint64_t{text.images.start} + text.images.count; ++image)
                        imageWidths->push_back({batch.images.at(static_cast<size_t>(image)).object, width});
                };
                std::optional<TableWidthPlan> tableWidths;
                std::span<notepads_markdown_preview_cell const> cells;
                std::span<notepads_markdown_preview_html_cell const> htmlCells;
                if (block.kind == 6)
                {
                    if (!block.columns || !block.row_count || !block.cells.count ||
                        block.cells.start > batch.cells.size() || block.cells.count > batch.cells.size() - block.cells.start)
                        throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                    const auto cellStart = static_cast<size_t>(block.cells.start);
                    const auto cellCount = static_cast<size_t>(block.cells.count);
                    cells = std::span<notepads_markdown_preview_cell const>{batch.cells}.subspan(cellStart, cellCount);
                    htmlCells = std::span<notepads_markdown_preview_html_cell const>{batch.htmlCells}.subspan(cellStart, cellCount);
                    if (htmlCells.front().flags & NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_HTML)
                    {
                        PreviewVector<uint32_t> owners{PreviewAllocator<uint32_t>{_layout}};
                        owners.resize(cellCount);
                        ValidatePreviewHtmlTable(cells, htmlCells, block.row_count, block.columns, owners);
                        tableWidths = TableWidthPlan::Html(block.width, block.columns);
                    }
                    else if (!TableHasRowHeaders(cells, block.columns, block.row_count))
                        tableWidths = TableWidthPlan::Markdown(block.width, block.columns);
                }
                if (tableWidths)
                {
                    // Visit each physical slot once. A per-image text search
                    // rescans the whole table when many cells contain images.
                    for (size_t cellIndex = 0; cellIndex < cells.size(); ++cellIndex)
                    {
                        auto const &cell = cells[cellIndex];
                        auto const &metadata = htmlCells[cellIndex];
                        ValidatePreviewHtmlCell(cell, metadata);
                        if (cell.text < block.texts.start || cell.text - block.texts.start >= block.texts.count)
                            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                        auto const &text = batch.texts.at(cell.text);
                        if (metadata.flags & NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_COVERED)
                        {
                            if (text.logical_start != text.logical_end || text.utf16_length || text.utf8.length ||
                                text.runs.count || text.images.count)
                                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                            continue;
                        }
                        appendWidths(text, tableWidths->TextWidth(cell.column, tableWidths->html ? metadata.colspan : 1));
                    }
                }
                else if (block.kind == 1 || block.kind == 2 || block.kind == 4)
                {
                    const auto width = std::max(1.0f, OrdinaryTextWidth(static_cast<float>(block.width), block.kind == 4,
                        _visibility && _visibility->SummaryIndex(block.node)));
                    for (auto const &text : texts)
                        appendWidths(text, width);
                }
                else
                {
                    // Front matter's natural key width and code gutters still
                    // require the ordinary measured path. No previous width
                    // or old resource generation is accepted as proof.
                    provenWidths = false;
                }
            }
            start += count;
        }
        return provenWidths;
    }

    void NativeDocumentRenderer::FinishSemantics()
    {
        if (_visibility)
            for (auto const &row : _visibility->disclosures)
                _semanticBuilder->disclosures.push_back({row.node, row.summary,
                    {static_cast<uint32_t>(row.summary_start), static_cast<uint32_t>(row.body_start)}});
        _semanticBuilder->outline = ReadingOutline::Build(*_main, _cancel->Get());
        _semanticBuilder->Finish(*_main);
        _descriptors = _semanticBuilder;
        _reading->PublishDescriptors(_descriptors);
        _semanticBuilder.reset();
    }

    void NativeDocumentRenderer::PrepareEditResources(PreviewEditResources &resources)
    {
        ReadVisibility();
        auto memory = _reading->memory;
        _semanticBuilder = std::allocate_shared<SemanticDescriptors>(ReadingAllocator<SemanticDescriptors>{memory},
                                                                     memory, _reading->textGeneration);
        auto widths = std::allocate_shared<PreviewImageWidths>(ReadingAllocator<PreviewImageWidths>{memory},
                                                               ReadingAllocator<PreviewImageWidth>{memory});
        const bool provenWidths = ReadUnfilteredSemantics(widths.get());
        FinishSemantics();
        resources.main = _main;
        resources.descriptors = _descriptors;
        auto imageCount = std::count_if(_descriptors->objects.begin(), _descriptors->objects.end(),
            [](auto const &object) { return object.kind == ReadingObjectKind::Image; });
        if (provenWidths && static_cast<size_t>(imageCount) == widths->size())
            resources.imageWidths = std::move(widths);
    }

    void NativeDocumentRenderer::ReadQuotes()
    {
        auto summary = CorePod<notepads_markdown_preview_quote_summary_result>();
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_preview_quote_summary(_layout, &summary, &error), L"quote summary");
        auto model = CorePod<notepads_markdown_core_model_summary_result>();
        Check(notepads_markdown_core_model_summary(_reading->memory->ModelOwner().Get(), &model, &error), L"quote model");
        if (std::memcmp(&summary.key, &_key, sizeof(_key)) != 0 ||
            std::memcmp(&model.identity, &_key.model, sizeof(_key.model)) != 0 || summary.rows > model.nodes)
            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        auto quotes = std::allocate_shared<PreviewVector<notepads_markdown_preview_quote>>(
            PreviewAllocator<PreviewVector<notepads_markdown_preview_quote>>{_layout}, PreviewAllocator<notepads_markdown_preview_quote>{_layout});
        if (summary.rows > quotes->max_size())
            throw std::bad_alloc();
        quotes->resize(static_cast<size_t>(summary.rows));
        for (uint64_t offset = 0; offset < summary.rows;)
        {
            auto count = std::min<uint64_t>(256, summary.rows - offset);
            auto output = CorePod<notepads_markdown_core_read_result>();
            Check(notepads_markdown_preview_quote_read(_layout, offset, count, sizeof(notepads_markdown_preview_quote), quotes->data() + offset,
                                        &output, &error), L"quote read");
            if (output.written != count || output.next != offset + count || output.reserved != 0 ||
                output.done != (offset + count == summary.rows ? 1u : 0u))
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
            offset += count;
        }
        PreviewVector<size_t> ancestors{PreviewAllocator<size_t>{_layout}};
        PreviewVector<uint32_t> nodes{PreviewAllocator<uint32_t>{_layout}};
        nodes.reserve(quotes->size());
        constexpr double tolerance = 0.000001;
        for (size_t index = 0; index < quotes->size(); ++index)
        {
            auto const &quote = (*quotes)[index];
            auto right = quote.x + quote.width;
            auto bottom = quote.y + quote.height;
            if (!quote.node || quote.node == NOTEPADS_MARKDOWN_PREVIEW_NONE || quote.node > model.nodes || !quote.depth ||
                quote.depth > quotes->size() || quote.depth > ancestors.size() + 1 ||
                quote.callout > NOTEPADS_MARKDOWN_CALLOUT_QUOTE || quote.reserved != 0 ||
                !std::isfinite(quote.x) || quote.x < 0 || !std::isfinite(quote.width) || quote.width <= 0 ||
                !std::isfinite(right) || right > _key.width + tolerance ||
                !std::isfinite(quote.y) || quote.y < 0 || !std::isfinite(quote.height) || quote.height < 0 ||
                !std::isfinite(bottom) || bottom > _extent.height + tolerance ||
                !std::isfinite(quote.border_width) || quote.border_width < 0 ||
                quote.border_width > std::min(5.0, quote.width))
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            while (ancestors.size() >= quote.depth)
            {
                auto const &prior = (*quotes)[ancestors.back()];
                if (quote.height > 0 && prior.height > 0 && quote.y + tolerance < prior.y + prior.height)
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                ancestors.pop_back();
            }
            if (!ancestors.empty())
            {
                auto const &parent = (*quotes)[ancestors.back()];
                if (quote.x + tolerance < parent.x || right > parent.x + parent.width + tolerance ||
                    (quote.height > 0 && (quote.y + tolerance < parent.y ||
                                         bottom > parent.y + parent.height + tolerance)))
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            }
            ancestors.push_back(index);
            nodes.push_back(quote.node);
        }
        std::sort(nodes.begin(), nodes.end());
        if (std::adjacent_find(nodes.begin(), nodes.end()) != nodes.end())
            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        _quotes = std::move(quotes);
    }

    void NativeDocumentRenderer::Measure(InitialViewport const *initialViewport)
    {
        _measuring = true;
        ReadVisibility();
        if (!_reading->textPrewarm)
            _reading->textPrewarm = NativeTextBitmapCache::CreatePrewarm();
        NativeTextBitmapCache::BeginPrewarm(_reading->textPrewarm, _key, _theme, _main);
        auto memory = _reading->memory;
        _navigation = std::allocate_shared<ReadingNavigationIndex>(ReadingAllocator<ReadingNavigationIndex>{memory},
                                                                   memory, _key, _main->Length(), _visibility,
                                                                   _section ? std::optional{_section->text} : std::nullopt);
        bool const unfilteredSemantics = !_descriptors && _visibility && !_visibility->hidden.empty();
        if (!_descriptors)
            _semanticBuilder = std::allocate_shared<SemanticDescriptors>(ReadingAllocator<SemanticDescriptors>{memory},
                                                                         memory, _reading->textGeneration);
        for (;;)
        {
            auto summary = PreviewPod<notepads_markdown_preview_summary>();
            auto error = CorePod<notepads_markdown_core_error>();
            Check(notepads_markdown_preview_measure_begin(_layout, initialViewport ? 8 : 32, &summary, &error), L"measure begin");
            if (!summary.blocks)
                break;
            {
                auto batch = Read(summary);
                std::array<notepads_markdown_preview_metric, 256> metrics{};
                if (batch.blocks.size() > metrics.size())
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                for (size_t i = 0; i < batch.blocks.size(); ++i)
                {
                    auto measured = MeasureBlock(batch, batch.blocks[i]);
                    NativeTextBitmapCache::CollectPrewarm(_reading->textPrewarm, measured);
                    CollectNavigation(batch.blocks[i], measured, static_cast<uint32_t>(summary.start + i));
                    if (_semanticBuilder && !unfilteredSemantics)
                        CollectSemantics(batch, batch.blocks[i]);
                    metrics[i] = {batch.blocks[i].node, 0, measured.measuredHeight};
                }
                Check(notepads_markdown_preview_submit(_layout, &_key, summary.start, metrics.data(),
                                        static_cast<uint32_t>(batch.blocks.size()), &error),
                      L"measure submit");
            }
            if (initialViewport)
            {
                auto const &target = *initialViewport;
                if (target.latestToken.load(std::memory_order_acquire) != target.token)
                {
                    initialViewport = nullptr;
                    continue;
                }
                _extent = PreviewPod<notepads_markdown_preview_extent>();
                // Leave measured geometry below the clip for pixel rounding
                // and the radius of a continuing quote's border.
                auto bottom = std::max(0.0, target.height * 1.5 - _theme.padding) + 4.0;
                Check(notepads_markdown_preview_begin_prefix(_layout, bottom, &_extent, &error), L"initial viewport");
                if (_extent.blocks)
                {
                    _initialViewportAttempted = true;
                    ReadQuotes();
                    _measuring = false;
                    _initialFrame = BuildFrame(0, target.height, target.token, target.latestToken, true);
                    _measuring = true;
                    if (_initialFrame)
                    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                        StartupTrace::Mark(StartupTrace::Phase::PrefixFrame, _key.layout_generation);
#endif
                        target.publish(_initialFrame);
                    }
                    _quotes.reset();
                    initialViewport = nullptr;
                }
            }
        }
        _extent = PreviewPod<notepads_markdown_preview_extent>();
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_preview_finish(_layout, &_extent, &error), L"layout finish");
        ReadQuotes();
        for (uint64_t start = 0; start < _extent.blocks;)
        {
            auto count = static_cast<uint32_t>(std::min<uint64_t>(256, _extent.blocks - start));
            auto summary = PreviewPod<notepads_markdown_preview_summary>();
            Check(notepads_markdown_preview_present(_layout, start, count, &summary, &error), L"presentation batch");
            std::array<notepads_markdown_preview_block, 256> rows{};
            auto read = CorePod<notepads_markdown_core_read_result>();
            Check(notepads_markdown_preview_read(_layout, NOTEPADS_MARKDOWN_PREVIEW_BLOCKS, 0, count, sizeof(notepads_markdown_preview_block), rows.data(), &read,
                                  &error));
            if (read.written != count)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
            NativeTextBitmapCache::PositionPrewarm(_reading->textPrewarm, {rows.data(), count});
            for (uint32_t i = 0; i < count; ++i)
                _navigation->AddBlock(rows[i]);
            start += count;
        }
        _navigation->Finish(*_main, &_reading->wordSegments);
        if (_semanticBuilder)
        {
            if (unfilteredSemantics)
                ReadUnfilteredSemantics();
            FinishSemantics();
        }
        _measuring = false;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        StartupTrace::Mark(StartupTrace::Phase::TextFormatsCreated, _textFormatCreated);
        StartupTrace::Mark(StartupTrace::Phase::TextFormatsReused, _textFormatReused);
#endif
    }

    std::shared_ptr<PreviewFrame> NativeDocumentRenderer::CreateFrame(double y, double height, uint64_t token,
                                                                      std::atomic<uint64_t> const &latestToken)
    {
        return BuildFrame(y, height, token, latestToken, false);
    }

    std::shared_ptr<PreviewFrame> NativeDocumentRenderer::BuildFrame(double y, double height, uint64_t token,
                                                                     std::atomic<uint64_t> const &latestToken, bool prefix)
    {
        if (latestToken.load(std::memory_order_acquire) != token)
            return {};
        auto viewport = PreviewPod<notepads_markdown_preview_viewport>();
        viewport.key = _key;
        if (!std::isfinite(y) || !std::isfinite(height) || y < 0 || height < 0 || !std::isfinite(y + height))
            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_BOUNDARY};
        // Scroll offsets address the Canvas including its top padding. Rust
        // addresses the document with its first block at zero. Preserve y in
        // the Frame for drawing, but query the exact clipped document interval.
        auto documentStart = y - _theme.padding;
        auto documentEnd = documentStart + height;
        viewport.y = std::clamp(documentStart, 0.0, _extent.height);
        viewport.height = std::clamp(documentEnd, 0.0, _extent.height) - viewport.y;
        viewport.overscan = height * 0.5;
        auto range = PreviewPod<notepads_markdown_preview_range>();
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_preview_visible(_layout, &viewport, &range, &error), L"visible range");
        if (range.count > 1024)
            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT};
        auto frame = std::allocate_shared<PreviewFrame>(PreviewAllocator<PreviewFrame>{_layout});
        if (viewport.height > 0)
        {
            frame->measuredTop = std::max(0.0, viewport.y - viewport.overscan);
            frame->measuredBottom = viewport.y + viewport.height + viewport.overscan;
        }
        frame->key = _key;
        frame->token = token;
        frame->viewportY = y;
        frame->viewportHeight = height;
        frame->totalHeight = _extent.height;
        frame->totalBlocks = _extent.blocks;
        frame->firstBlock = range.start;
        frame->theme = _theme;
        frame->math = _math;
        frame->diagram = _diagram;
        frame->images = _images;
        frame->highlights = _reading->highlights;
        frame->highlightingStatus = _reading->highlightingStatus;
        frame->main = _main;
        frame->section = _section;
        if (!prefix)
        {
            frame->textPrewarm = _reading->textPrewarm;
            frame->descriptors = _descriptors;
            frame->navigation = _navigation;
        }
        frame->quotes = _quotes;
        frame->visibility = _visibility;
        if (!prefix)
        {
            auto initial = std::exchange(_initialFrame, {});
            if (initial && initial->viewportY == y && initial->viewportHeight == height &&
                initial->token == token && initial->firstBlock == range.start && initial->Blocks().size() == range.count)
            {
                // The draft already measured through the same lookahead. Its
                // immutable block visuals still have the final exact positions.
                frame->measuredBlocks = initial->measuredBlocks;
                frame->mathStates = _mathStates;
                frame->diagramStates = _diagramStates;
                return frame;
            }
        }
        auto blocks = std::allocate_shared<PreviewVector<PreviewBlockVisual>>(
            PreviewAllocator<PreviewVector<PreviewBlockVisual>>{_layout}, PreviewAllocator<PreviewBlockVisual>{_layout});
        blocks->reserve(static_cast<size_t>(range.count));
        for (uint64_t start = range.start; start < range.start + range.count;)
        {
            if (latestToken.load(std::memory_order_acquire) != token)
                return {};
            auto count = static_cast<uint32_t>(std::min<uint64_t>(16, range.start + range.count - start));
            auto summary = PreviewPod<notepads_markdown_preview_summary>();
            Check(notepads_markdown_preview_present(_layout, start, count, &summary, &error), L"presentation batch");
            auto batch = Read(summary);
            for (auto const &block : batch.blocks)
            {
                if (latestToken.load(std::memory_order_acquire) != token)
                    return {};
                blocks->push_back(MeasureBlock(batch, block));
            }
            start += count;
        }
        frame->measuredBlocks = std::move(blocks);
        frame->mathStates = _mathStates;
        frame->diagramStates = _diagramStates;
        return frame;
    }

    std::shared_ptr<PreviewFrame> PreviewFrame::Reposition(double y, double height) const
    {
        // Use the same document-space interval as CreateFrame, clipped at both ends.
        auto start = std::max(0.0, y - theme.padding);
        auto end = std::min(totalHeight, y - theme.padding + height);
        if (start >= end || start < measuredTop || end > measuredBottom)
            return {};
        // The model's allocator can run on the UI thread without borrowing the worker's layout.
        auto repositioned = std::allocate_shared<PreviewFrame>(ReadingAllocator<PreviewFrame>{main->Memory()}, *this);
        repositioned->viewportY = y;
        repositioned->viewportHeight = height;
        return repositioned;
    }

    std::shared_ptr<PreviewFrame> PreviewFrame::WithImages(std::shared_ptr<ImagePresentationSnapshot const> const &snapshot) const
    {
        auto frame = std::allocate_shared<PreviewFrame>(ReadingAllocator<PreviewFrame>{main->Memory()}, *this);
        frame->images = snapshot;
        // A status-only refresh can publish without replaying the surface.
        for (size_t index = 0; index < diagramDrawOutcomes.size(); ++index)
            frame->diagramDrawOutcomes[index].status.store(
                diagramDrawOutcomes[index].status.load(std::memory_order_acquire), std::memory_order_relaxed);
        return frame;
    }

    std::shared_ptr<ImagePixels const> const &PreviewFrame::ImagePixelsFor(uint32_t object) const noexcept
    {
        static const std::shared_ptr<ImagePixels const> empty;
        auto entry = images ? images->Find(object) : nullptr;
        return entry ? entry->pixels : empty;
    }

    uint32_t PreviewFrame::ImageFrameIndex(uint32_t object) const noexcept
    {
        auto entry = images ? images->Find(object) : nullptr;
        return entry ? entry->frameIndex : 0;
    }

    NativeMathState PreviewFrame::MathState(uint32_t object) const noexcept
    {
        for (auto const &block : Blocks())
        {
            for (auto const &text : block.texts)
            {
                for (auto const &visual : text.math)
                {
                    if (visual.object == object)
                        return visual.state;
                }
            }
        }
        uint32_t ordinal{};
        if (descriptors)
        {
            for (auto const &descriptor : descriptors->objects)
            {
                if (descriptor.kind != ReadingObjectKind::Math)
                    continue;
                if (descriptor.object == object)
                {
                    if (ordinal >= mathStates.size())
                        return NativeMathState::Limited;
                    auto state = mathStates[ordinal];
                    return state == NativeMathState::None ? MathAttempt(math, ordinal) : state;
                }
                ++ordinal;
            }
        }
        return NativeMathState::Unavailable;
    }

    void PreviewFrame::TakeDiagramDrawingFrom(PreviewFrame const &previous) const noexcept
    {
        if (!diagram.presentations.count || !previous.diagram.presentations.count ||
            !measuredBlocks || !previous.measuredBlocks || measuredBlocks == previous.measuredBlocks)
            return;
        // Ordinals are bounded by the existing per-control resident policy. Scan
        // each frame once, including tables, rather than rescanning it per SVG.
        std::array<PreviewDiagramVisual const *, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT> donors{};
        for (auto const &block : previous.Blocks())
            for (auto const &text : block.texts)
                for (auto const &visual : text.diagrams)
                    if (visual.ordinal < donors.size() && visual.state == NativeDiagramState::Ready &&
                        visual.presentation && visual.drawing.owner.operator->() == visual.presentation.operator->() &&
                        visual.drawing.document && visual.drawing.stream && visual.drawing.context && visual.drawing.device &&
                        visual.drawing.deviceEpoch == visual.presentation->Key().deviceEpoch &&
                        SUCCEEDED(visual.drawing.status.load(std::memory_order_acquire)) &&
                        SUCCEEDED(previous.diagramDrawOutcomes[visual.ordinal].status.load(std::memory_order_acquire)))
                        donors[visual.ordinal] = &visual;
        for (auto const &block : Blocks())
            for (auto const &text : block.texts)
                for (auto const &visual : text.diagrams)
                {
                    if (visual.ordinal >= donors.size() || visual.state != NativeDiagramState::Ready ||
                        !visual.presentation || visual.drawing.owner || visual.drawing.document ||
                        visual.drawing.stream || visual.drawing.context || visual.drawing.device ||
                        FAILED(visual.drawing.status.load(std::memory_order_acquire)) ||
                        diagramDrawOutcomes[visual.ordinal].status.load(std::memory_order_acquire) != S_FALSE)
                        continue;
                    auto donor = donors[visual.ordinal];
                    if (!donor || donor->object != visual.object ||
                        donor->presentation.operator->() != visual.presentation.operator->())
                        continue;
                    // The old surface keeps its pixels. A rejected candidate may
                    // leave that frame cold, but neither frame's published draw
                    // outcome changes and its next draw can recreate the cache.
                    visual.drawing = std::move(donor->drawing);
                    donors[visual.ordinal] = nullptr;
                }
    }

    NativeDiagramState PreviewFrame::DiagramState(uint32_t object) const noexcept
    {
        for (auto const &block : Blocks())
            for (auto const &text : block.texts)
                for (auto const &visual : text.diagrams)
                    if (visual.object == object)
                    {
                        if (visual.state != NativeDiagramState::Ready)
                            return visual.state;
                        if (visual.ordinal >= diagramDrawOutcomes.size())
                            return NativeDiagramState::Limited;
                        auto status = diagramDrawOutcomes[visual.ordinal].status.load(std::memory_order_acquire);
                        return FAILED(status) ? DiagramFailureState(status) : visual.state;
                    }
        uint32_t ordinal{};
        if (descriptors)
            for (auto const &descriptor : descriptors->objects)
            {
                if (descriptor.kind != ReadingObjectKind::Diagram)
                    continue;
                if (descriptor.object == object)
                {
                    if (ordinal >= diagramStates.size())
                        return NativeDiagramState::Limited;
                    auto state = diagramStates[ordinal];
                    return state == NativeDiagramState::None ? DiagramAttempt(diagram, ordinal) : state;
                }
                ++ordinal;
            }
        return NativeDiagramState::Unavailable;
    }

    std::optional<ReadingImageHit> NativeDocumentRenderer::ImageAt(PreviewFrame const &frame, float x, float y) noexcept
    {
        for (auto const &block : frame.Blocks())
        {
            const auto blockY = static_cast<float>(block.block.y - frame.viewportY);
            if (y < blockY || y >= blockY + block.block.height)
                continue;
            for (auto const &text : block.texts)
            {
                for (auto const &image : text.images)
                {
                    if (image.note)
                        continue;
                    const auto left = static_cast<float>(block.block.x) + text.x + image.x;
                    const auto top = blockY + text.y + image.y;
                    if (!(x >= left && x < left + image.width && y >= top && y < top + image.height))
                        continue;
                    const bool ready = static_cast<bool>(frame.ImagePixelsFor(image.object));
                    if (ready || image.iconSize > 0)
                        return ReadingImageHit{static_cast<uint32_t>(text.logicalStart + image.offset), ready};
                }
            }
        }
        return {};
    }

    std::optional<uint32_t> NativeDocumentRenderer::MathFallbackAt(PreviewFrame const &frame, float x, float y) noexcept
    {
        for (auto const &block : frame.Blocks())
        {
            const auto blockY = static_cast<float>(block.block.y - frame.viewportY);
            if (y < blockY || y >= blockY + block.block.height)
                continue;
            for (auto const &text : block.texts)
            {
                for (auto const &math : text.math)
                {
                    // Only an actually painted terminal fallback has a diagnostic.
                    // Ready formulas and temporary loading labels have no error tip.
                    if (!math.label || math.state == NativeMathState::Ready ||
                        math.state == NativeMathState::Pending || math.state == NativeMathState::None)
                        continue;
                    const auto left = static_cast<float>(block.block.x) + text.x + math.x;
                    const auto top = blockY + text.y + math.y;
                    if (x >= left && x < left + math.width && y >= top && y < top + math.height)
                        return static_cast<uint32_t>(text.logicalStart + math.offset);
                }
            }
        }
        return {};
    }

    std::optional<uint32_t> NativeDocumentRenderer::DisclosureAt(PreviewFrame const &frame, float x, float y) noexcept
    {
        if (!frame.visibility || !frame.descriptors)
            return {};
        auto documentY = frame.viewportY + y;
        for (auto const &visual : frame.Blocks())
        {
            auto const &block = visual.block;
            if (documentY < block.y)
                break;
            if (documentY >= block.y + block.height || x < block.x || x >= block.x + block.width)
                continue;
            if (auto index = frame.visibility->SummaryIndex(block.node); index && *index <= UINT32_MAX)
                return static_cast<uint32_t>(*index);
        }
        return {};
    }

    std::optional<ReadingHit> NativeDocumentRenderer::HitTest(PreviewFrame const &frame, float x, float y)
    {
        return HitTestWithin(frame, x, y, nullptr);
    }
    std::optional<ReadingHit> NativeDocumentRenderer::HitTestLine(PreviewFrame const &frame, size_t lineIndex,
                                                                  float preferredX)
    {
        if (!frame.main || !frame.navigation || !std::isfinite(preferredX))
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        auto const &line = frame.navigation->Line(lineIndex);
        frame.main->ValidateRange(line.text.start, line.text.end - line.text.start);
        return HitTestWithin(frame, preferredX, static_cast<float>(line.y - frame.viewportY + line.height * 0.5),
                             &line);
    }
    std::optional<ReadingHit> NativeDocumentRenderer::HitTestWithin(PreviewFrame const &frame, float x, float y,
                                                                    ReadingLine const *line)
    {
        std::optional<ReadingHit> best;
        float distance = std::numeric_limits<float>::infinity();
        bool materialized{};
        PreviewBlockVisual const *ownerBlock{};
        PreviewTextVisual const *ownerText{};
        bool ownerPadding{};
        if (!line)
            for (auto const &block : frame.Blocks())
            {
                if (!block.htmlTable)
                    continue;
                auto localX = x - static_cast<float>(block.block.x);
                auto localY = y - static_cast<float>(block.block.y - frame.viewportY);
                for (auto const &cell : block.tableCells)
                    if (cell.textIndex != NOTEPADS_MARKDOWN_PREVIEW_NONE && localX >= cell.bounds.left &&
                        localX < cell.bounds.right && localY >= cell.bounds.top && localY < cell.bounds.bottom)
                    {
                        ownerBlock = &block;
                        ownerText = &block.texts.at(cell.textIndex);
                        ownerPadding = localX < ownerText->x || localX >= ownerText->x + ownerText->width ||
                                       localY < ownerText->y || localY >= ownerText->y + ownerText->height;
                        break;
                    }
                if (ownerBlock)
                    break;
            }
        auto rowContains = [&](float top, float height)
        {
            if (!line)
                return true;
            auto lineTop = line->y - frame.viewportY;
            return height > 0 &&
                   std::min<double>(top + height, lineTop + line->height) > std::max<double>(top, lineTop) + 0.01;
        };
        auto allowed = [&](ReadingHit const &hit, float height)
        {
            if (!line)
                return true;
            // Shared soft-wrap endpoints are not enough: the actual cluster
            // must belong to this logical line AND this physical row. Table
            // cells at the same Y remain separated by their Main intervals.
            return hit.position >= line->text.start && hit.position <= line->text.end &&
                   hit.cluster.start >= line->text.start && hit.cluster.end <= line->text.end &&
                   rowContains(hit.y, height);
        };
        for (auto const &block : frame.Blocks())
        {
            if (ownerBlock && &block != ownerBlock)
                continue;
            auto bx = static_cast<float>(block.block.x);
            auto by = static_cast<float>(block.block.y - frame.viewportY);
            for (auto const &decoration : block.decorations)
            {
                if (!IsPreviewTaskDecoration(decoration.kind))
                    continue;
                auto left = MarkerOrigin(block) + 6;
                auto dx = std::max({left - x, x - left - 14, 0.0f});
                auto top = by + block.taskMarkerTop;
                auto dy = std::max({top - y, y - top - 14, 0.0f});
                auto squared = dx * dx + dy * dy;
                ReadingHit candidate{
                    static_cast<uint32_t>(decoration.logical_start + (x > left + 7 ? 1 : 0)),
                    squared == 0,
                    left,
                    top,
                    {static_cast<uint32_t>(decoration.logical_start), static_cast<uint32_t>(decoration.logical_end)}};
                if (!allowed(candidate, 14))
                    continue;
                materialized = true;
                if (squared < distance)
                {
                    best = candidate;
                    distance = squared;
                }
                if (squared == 0)
                    return candidate;
            }
            for (auto const &text : block.texts)
            {
                if (ownerText && &text != ownerText)
                    continue;
                if (line)
                {
                    bool logical = line->text.start == line->text.end
                                       ? text.logicalStart <= line->text.start && line->text.start <= text.logicalEnd
                                       : text.logicalStart < line->text.end && line->text.start < text.logicalEnd;
                    if (!logical || !rowContains(by + text.y, text.height))
                        continue;
                    materialized = true;
                }
                for (auto const &diagram : text.diagrams)
                {
                    auto left = bx + text.x + diagram.x;
                    auto top = by + text.y + diagram.y;
                    if (x >= left && x <= left + diagram.width && y >= top && y <= top + diagram.height)
                    {
                        auto start = static_cast<uint32_t>(text.logicalStart + diagram.offset);
                        ReadingHit candidate{
                            start + (x > left + diagram.width / 2.0f ? 1u : 0u), true, left, top, {start, start + 1}};
                        if (ownerPadding)
                            candidate.inside = false;
                        if (allowed(candidate, diagram.height))
                            return candidate;
                    }
                }
                if (text.scripts)
                {
                    std::optional<size_t> ownerLine;
                    if (line)
                    {
                        auto rows = text.scripts->Lines();
                        auto targetY = static_cast<float>(line->y - frame.viewportY) - by - text.y;
                        auto row = std::lower_bound(rows.begin(), rows.end(), targetY - 0.01f,
                            [](NativeScriptLine const &value, float top) { return value.finalY < top; });
                        if (row == rows.end() || std::abs(row->finalY - targetY) > 0.02f ||
                            text.logicalStart + row->start + row->length - row->newlineLength != line->text.end ||
                            line->text.start > text.logicalStart + row->start)
                            continue;
                        ownerLine = static_cast<size_t>(row - rows.begin());
                    }
                    auto hit = text.scripts->HitTestPoint(x - bx - text.x, y - by - text.y, ownerLine);
                    auto const &metric = hit.metrics;
                    auto position = text.logicalStart + metric.textPosition + (hit.trailing ? metric.length : 0);
                    auto clusterStart = text.logicalStart + metric.textPosition;
                    auto clusterEnd = clusterStart + metric.length;
                    if (position > text.logicalEnd || clusterEnd > text.logicalEnd)
                        throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                    if (line && clusterStart == line->text.end && clusterEnd > line->text.end)
                        position = clusterEnd = clusterStart;
                    if (line && (position < line->text.start || position > line->text.end ||
                                 clusterStart < line->text.start || clusterEnd > line->text.end))
                        continue;
                    auto px = bx + text.x + metric.left;
                    auto py = by + text.y + metric.top;
                    auto dx = std::max({px - x, x - px - metric.width, 0.0f});
                    auto dy = std::max({py - y, y - py - metric.height, 0.0f});
                    auto squared = dx * dx + dy * dy;
                    materialized = true;
                    if (squared < distance)
                    {
                        best = ReadingHit{static_cast<uint32_t>(position), hit.inside, px, py,
                                          {static_cast<uint32_t>(clusterStart), static_cast<uint32_t>(clusterEnd)},
                                          hit.trailing && clusterStart < clusterEnd};
                        distance = squared;
                    }
                    continue;
                }
                BOOL trailing{}, inside{};
                DWRITE_HIT_TEST_METRICS metric{};
                auto localY = y - by - text.y;
                bool afterCode = text.terminalBreak && localY >= text.height;
                if (afterCode)
                    localY = std::nextafter(text.height, 0.0f);
                auto localX = x - bx - text.x;
                winrt::check_hresult(
                    text.layout->HitTestPoint(localX, localY, &trailing, &inside, &metric));
                auto negativeAdvance = metric.width < 0;
                if (negativeAdvance)
                {
                    FLOAT leadingX{}, trailingX{}, caretY{};
                    DWRITE_HIT_TEST_METRICS ignored{};
                    winrt::check_hresult(text.layout->HitTestTextPosition(metric.textPosition, FALSE,
                        &leadingX, &caretY, &ignored));
                    winrt::check_hresult(text.layout->HitTestTextPosition(metric.textPosition, TRUE,
                        &trailingX, &caretY, &ignored));
                    if (!std::isfinite(leadingX) || !std::isfinite(trailingX))
                        throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                    trailing = std::abs(double(localX) - trailingX) < std::abs(double(localX) - leadingX);
                }
                metric = NativeScriptLayout::CanonicalHitMetric(metric);
                if (negativeAdvance)
                    inside = localX >= metric.left && double(localX) <= double(metric.left) + metric.width && metric.width > 0 &&
                             localY >= metric.top && double(localY) <= double(metric.top) + metric.height && metric.height > 0;
                if (afterCode)
                    inside = FALSE;
                auto px = bx + text.x + metric.left;
                auto py = by + text.y + metric.top;
                auto dx = std::max({px - x, x - (px + metric.width), 0.0f});
                auto dy = std::max({py - y, y - (py + metric.height), 0.0f});
                auto squared = dx * dx + dy * dy;
                bool acceptedPoint{};
                if (line || squared < distance)
                {
                    auto position = text.logicalStart + metric.textPosition + (trailing ? metric.length : 0);
                    if (position > text.logicalEnd)
                        throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                    auto clusterStart = text.logicalStart + metric.textPosition;
                    auto clusterEnd = clusterStart + metric.length;
                    if (clusterEnd > text.logicalEnd)
                        throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                    ReadingHit candidate{static_cast<uint32_t>(position),
                                         inside != FALSE,
                                         px,
                                         py,
                                         {static_cast<uint32_t>(clusterStart), static_cast<uint32_t>(clusterEnd)},
                                         trailing && clusterStart < clusterEnd};
                    acceptedPoint = allowed(candidate, metric.height);
                    if (acceptedPoint && squared < distance)
                    {
                        best = candidate;
                        distance = squared;
                    }
                }
                if (line && !acceptedPoint)
                {
                    // A newline cluster can extend beyond the line's selectable
                    // text, including an empty line. Obtain real DWrite caret
                    // geometry instead of clamping a foreign logical position.
                    // Soft-wrap endpoints on the next row are rejected here.
                    for (auto position : {line->text.start, line->text.end})
                    {
                        if (position < text.logicalStart || position > text.logicalEnd)
                            continue;
                        FLOAT left{}, top{};
                        DWRITE_HIT_TEST_METRICS caret{};
                        auto offset = static_cast<UINT32>(position - text.logicalStart);
                        const bool atWrappedEnd = line->wrappedEnd && position == line->text.end && offset > 0;
                        winrt::check_hresult(text.layout->HitTestTextPosition(
                            offset - (atWrappedEnd ? 1 : 0), atWrappedEnd, &left, &top, &caret));
                        ReadingHit candidate{
                            position, false, bx + text.x + left, by + text.y + top, {position, position}, atWrappedEnd};
                        if (!allowed(candidate, caret.height))
                            continue;
                        auto caretDx = candidate.x - x;
                        auto caretDy = std::max({candidate.y - y, y - candidate.y - caret.height, 0.0f});
                        auto caretDistanceSquared = caretDx * caretDx + caretDy * caretDy;
                        if (caretDistanceSquared < distance)
                        {
                            best = candidate;
                            distance = caretDistanceSquared;
                        }
                    }
                }
            }
        }
        if (!best && line && materialized)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        if (best && ownerText)
        {
            if (best->position < ownerText->logicalStart || best->position > ownerText->logicalEnd ||
                best->cluster.start < ownerText->logicalStart || best->cluster.end > ownerText->logicalEnd)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            if (ownerPadding)
                best->inside = false;
        }
        if (!best && !line && frame.main && !frame.main->Length())
            return ReadingHit{0, false, 0, 0};
        if (best && best->trailingCaret)
        {
            // A trailing cluster edge requires an affinity only at a shared
            // wrap endpoint. The cluster must belong to the preceding row.
            auto const *navigation = frame.navigation.get();
            best->trailingCaret = false;
            if (navigation)
            {
                auto prior = navigation->LineAt(best->position, true);
                if (prior != navigation->LineAt(best->position))
                {
                    auto const &row = navigation->Line(prior);
                    best->trailingCaret = row.text.end == best->position &&
                                          row.text.Contains(best->cluster.start, best->cluster.end);
                }
            }
        }
        return best;
    }
    ReadingVector<ReadingRect> NativeDocumentRenderer::Rectangles(PreviewFrame const &frame, uint32_t start,
                                                                  uint32_t end, bool trailingCaret)
    {
        if (!frame.main)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        frame.main->ValidateRange(start, end - start);
        if (start == end)
        {
            if (frame.navigation)
            {
                start = end = frame.navigation->CanonicalCaret(start);
                trailingCaret = trailingCaret &&
                    frame.navigation->LineAt(start, true) != frame.navigation->LineAt(start);
            }
            else
                trailingCaret = false;
            ReadingVector<ReadingRect> result{ReadingAllocator<ReadingRect>{frame.main->Memory()}};
            std::optional<ReadingRect> objectCaret;
            for (auto const &block : frame.Blocks())
            {
                auto bx = static_cast<float>(block.block.x);
                auto by = static_cast<float>(block.block.y - frame.viewportY);
                for (auto const &decoration : block.decorations)
                {
                    if (IsPreviewTaskDecoration(decoration.kind) &&
                        (start == decoration.logical_start || start == decoration.logical_end))
                        objectCaret = ReadingRect{MarkerOrigin(block) +
                                                      (start == decoration.logical_start ? 6.0f : 20.0f),
                                                  by + block.taskMarkerTop,
                                                  1,
                                                  14,
                                                  start,
                                                  end};
                }
                for (auto const &text : block.texts)
                {
                    if (start < text.logicalStart || start > text.logicalEnd)
                        continue;
                    for (auto const &diagram : text.diagrams)
                    {
                        auto position = text.logicalStart + diagram.offset;
                        if (start == position || start == position + 1)
                        {
                            result.push_back({bx + text.x + diagram.x + (start == position ? 0.0f : diagram.width),
                                              by + text.y + diagram.y, 1.0f, diagram.height, start, end});
                            return result;
                        }
                    }
                    FLOAT left{}, top{};
                    DWRITE_HIT_TEST_METRICS metric{};
                    auto offset = static_cast<UINT32>(start - text.logicalStart);
                    if (start == text.logicalEnd)
                        offset -= text.terminalBreak;
                    const auto trailing = trailingCaret && offset > 0;
                    if (trailing)
                        --offset;
                    if (text.scripts)
                    {
                        auto hit = text.scripts->Caret(offset, trailing);
                        left = hit.caretX;
                        top = hit.caretY;
                        metric = hit.metrics;
                    }
                    else
                        winrt::check_hresult(text.layout->HitTestTextPosition(offset, trailing, &left, &top, &metric));
                    result.push_back({bx + text.x + left, by + text.y + top, 1, metric.height, start, end});
                    return result;
                }
            }
            if (objectCaret)
                result.push_back(*objectCaret);
            else if (!frame.main->Length())
                result.push_back({0, 0, 1, frame.theme.lineHeight, 0, 0});
            return result;
        }
        return RangeRectangles(frame, start, end, frame.Blocks());
    }

    ReadingVector<ReadingRect> NativeDocumentRenderer::LineRectangles(PreviewFrame const &frame, ReadingLine const &line)
    {
        if (!frame.main)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        auto blocks = std::span{frame.Blocks()};
        if (line.block >= frame.firstBlock && line.block - frame.firstBlock < blocks.size())
            blocks = blocks.subspan(static_cast<size_t>(line.block - frame.firstBlock), 1);
        else
            blocks = {};
        return RangeRectangles(frame, line.text.start, line.text.end, blocks, line.textIndex);
    }

    ReadingVector<ReadingRect> NativeDocumentRenderer::RangeRectangles(PreviewFrame const &frame, uint32_t start,
                                                                      uint32_t end,
                                                                      std::span<PreviewBlockVisual const> blocks,
                                                                      uint32_t textIndex)
    {
        ReadingVector<ReadingRect> result{ReadingAllocator<ReadingRect>{frame.main->Memory()}};
        if (start == end)
            return result;
        for (auto const &block : blocks)
        {
            auto bx = static_cast<float>(block.block.x);
            auto by = static_cast<float>(block.block.y - frame.viewportY);
            for (auto const &decoration : block.decorations)
            {
                if (IsPreviewTaskDecoration(decoration.kind) &&
                    decoration.logical_start >= start && decoration.logical_end <= end)
                    result.push_back({MarkerOrigin(block) + 6, by + block.taskMarkerTop, 14, 14,
                                      static_cast<uint32_t>(decoration.logical_start),
                                      static_cast<uint32_t>(decoration.logical_end)});
            }
            auto texts = std::span{block.texts};
            if (textIndex != NOTEPADS_MARKDOWN_PREVIEW_NONE)
            {
                if (textIndex >= texts.size())
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                texts = texts.subspan(textIndex, 1);
            }
            for (auto const &text : texts)
            {
                auto begin = std::max<uint64_t>(start, text.logicalStart);
                auto finish = std::min<uint64_t>(end, text.logicalEnd);
                if (begin >= finish)
                    continue;
                UINT32 count{};
                auto localStart = static_cast<UINT32>(begin - text.logicalStart);
                auto length = static_cast<UINT32>(finish - begin);
                ReadingVector<DWRITE_HIT_TEST_METRICS> metrics{
                    ReadingAllocator<DWRITE_HIT_TEST_METRICS>{frame.main->Memory()}};
                if (text.scripts)
                {
                    ReadingVector<NativeScriptRangeMetric> adjusted{
                        ReadingAllocator<NativeScriptRangeMetric>{frame.main->Memory()}};
                    text.scripts->Range(localStart, length, adjusted);
                    metrics.reserve(adjusted.size());
                    for (auto const &row : adjusted)
                        metrics.push_back(row.metrics);
                }
                else
                {
                    auto hr = text.layout->HitTestTextRange(localStart, length, 0, 0, nullptr, 0, &count);
                    if (FAILED(hr) && hr != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                        winrt::check_hresult(hr);
                    metrics.resize(count);
                    if (count)
                        winrt::check_hresult(
                            text.layout->HitTestTextRange(localStart, length, 0, 0, metrics.data(), count, &count));
                    metrics.resize(count);
                }
                for (auto metric : metrics)
                {
                    metric = NativeScriptLayout::CanonicalHitMetric(metric);
                    auto diagram = std::find_if(text.diagrams.begin(), text.diagrams.end(), [&](auto const &value)
                                                { return metric.textPosition == value.offset && metric.length == 1; });
                    if (diagram != text.diagrams.end())
                    {
                        auto position = static_cast<uint32_t>(text.logicalStart + diagram->offset);
                        result.push_back({bx + text.x + diagram->x, by + text.y + diagram->y, diagram->width,
                                          diagram->height, position, position + 1});
                        continue;
                    }
                    result.push_back({bx + text.x + metric.left, by + text.y + metric.top, metric.width, metric.height,
                                      static_cast<uint32_t>(text.logicalStart + metric.textPosition),
                                      static_cast<uint32_t>(text.logicalStart + metric.textPosition + metric.length)});
                }
            }
        }
        return result;
    }

    // Draw just the shaped text fragments so wrapping, alignment and bidi gaps
    // agree with the content that will replace the placeholder.
    static bool DrawPendingText(ID2D1DeviceContext *context, PreviewTextVisual const &text,
                                D2D1_POINT_2F origin, ID2D1SolidColorBrush *brush)
    {
        std::array<DWRITE_HIT_TEST_METRICS, 64> fragments;
        UINT32 count{};
        if (FAILED(text.layout->HitTestTextRange(0, text.utf16Length, origin.x, origin.y,
                                                fragments.data(), static_cast<UINT32>(fragments.size()), &count)))
            return false;
        bool painted{};
        for (UINT32 i = 0; i < count; ++i)
        {
            auto const &part = fragments[i];
            if (!(part.width > 0 && part.height > 0)) continue;
            const auto halfHeight = part.height * 0.17f;
            const auto center = part.top + part.height * 0.54f;
            const auto radius = std::min(halfHeight, part.width * 0.5f);
            context->FillRoundedRectangle(D2D1::RoundedRect(D2D1::RectF(part.left, center - halfHeight,
                part.left + part.width, center + halfHeight), radius, radius), brush);
            painted = true;
        }
        return painted;
    }

    void NativeDocumentRenderer::Draw(PreviewFrame const &frame, ID2D1DeviceContext *context, float originX,
                                      float originY, uint32_t selectionStart, uint32_t selectionEnd,
                                      ReadingActionTarget focusedAction, bool focused, ImageDrawCache *imageCache,
                                      bool markInactiveCaret, std::optional<size_t> hoveredLink,
                                      std::optional<D2D1_RECT_F> paintClip, NativeTextBitmapCache *textCache,
                                      std::function<void()> const *repaint, bool allowPendingText,
                                      std::shared_ptr<NativeHighlightSnapshot const> const &startupHighlights,
                                      bool trailingCaret)
    {
        auto mathPaintMargin = std::numeric_limits<double>::infinity();
        auto ordinaryPaintBottom = std::numeric_limits<double>::infinity();
        auto ordinaryCacheTop = -std::numeric_limits<double>::infinity();
        if (paintClip)
        {
            D2D1_MATRIX_3X2_F transform;
            context->GetTransform(&transform);
            // The physical-pixel guard is valid for translation-only Draw coordinates.
            // Unknown transforms and absent/invalid clips keep generic Draw unculled.
            if (transform._11 == 1 && transform._12 == 0 && transform._21 == 0 && transform._22 == 1 &&
                std::isfinite(transform._31) && std::isfinite(transform._32))
            {
                float dpiX{}, dpiY{};
                context->GetDpi(&dpiX, &dpiY);
                if (std::isfinite(dpiY) && dpiY > 0)
                    mathPaintMargin = (std::max)(2.0, 192.0 / dpiY);
                if (context->GetUnitMode() == D2D1_UNIT_MODE_DIPS && std::isfinite(mathPaintMargin) &&
                    std::isfinite(paintClip->left) && std::isfinite(paintClip->top) &&
                    std::isfinite(paintClip->right) && std::isfinite(paintClip->bottom) &&
                    paintClip->left < paintClip->right && paintClip->top < paintClip->bottom)
                {
                    ordinaryCacheTop = paintClip->top;
                    ordinaryPaintBottom = static_cast<double>(paintClip->bottom) + mathPaintMargin +
                        std::abs(static_cast<double>(paintClip->bottom)) * (8.0 * std::numeric_limits<float>::epsilon());
                }
            }
        }
        // D2D takes float origins. Subtracting a different viewport row can
        // otherwise give the same document position a different float ULP and
        // therefore a different exact bitmap key. Normalize the actual drawing
        // position, not the key, on a binary DIP grid no coarser than 1/64
        // physical pixel. The largest displacement is 1/128 physical pixel.
        // Static badges and paint-only range rectangles receive the same delta.
        double textOriginUnits{};
        bool stableScriptOrigins{};
        D2D1_MATRIX_3X2_F textTransform;
        context->GetTransform(&textTransform);
        if (context->GetUnitMode() == D2D1_UNIT_MODE_DIPS && textTransform._11 == 1 &&
            textTransform._12 == 0 && textTransform._21 == 0 && textTransform._22 == 1)
        {
            float dpiX{}, dpiY{};
            context->GetDpi(&dpiX, &dpiY);
            if (dpiY > 0 && std::isfinite(dpiY))
            {
                textOriginUnits = std::exp2(std::ceil(std::log2(double{dpiY} * (64.0 / 96.0))));
                stableScriptOrigins = NativeTextBitmapCache::SupportsScriptDpi(dpiX, dpiY);
            }
        }
        auto textPaintY = [&](PreviewTextVisual const &text, float value) noexcept
        {
            // The same physical grid applies to static and dynamic script paint,
            // including its selection rectangles, focus and pending skeleton.
            const auto script = stableScriptOrigins && text.scriptDrawing && text.scripts && text.matchesMainText;
            const auto ordinary = text.bitmapCandidate && !text.scriptDrawing && !text.scripts;
            if (!textOriginUnits || !std::isfinite(value) || (!ordinary && !script) ||
                !text.images.empty() || !text.math.empty() || !text.diagrams.empty())
                return value;
            return static_cast<float>(std::round(double{value} * textOriginUnits) / textOriginUnits);
        };
        auto paintRectangle = [&](ReadingRect rectangle)
        {
            if (!textOriginUnits)
                return rectangle;
            for (auto const &block : frame.Blocks())
            {
                if (rectangle.start < block.block.logical_start || rectangle.end > block.block.logical_end)
                    continue;
                const auto y = originY + static_cast<float>(block.block.y - frame.viewportY);
                for (auto const &text : block.texts)
                    if (rectangle.start >= text.logicalStart && rectangle.end <= text.logicalEnd)
                    {
                        const auto original = y + text.y;
                        rectangle.y += textPaintY(text, original) - original;
                        return rectangle;
                    }
            }
            return rectangle;
        };
        // Decoration is applied only during this draw. Layouts can be reused by subsequent frames.
        struct LinkUnderline
        {
            std::span<DWRITE_TEXT_RANGE const> authored{};
            IDWriteTextLayout *layout{};
            void Add(IDWriteTextLayout *target, DWRITE_TEXT_RANGE range)
            {
                layout = target;
                winrt::check_hresult(layout->SetUnderline(TRUE, range));
            }
            ~LinkUnderline()
            {
                if (layout)
                {
                    (void)layout->SetUnderline(FALSE, {0, UINT32_MAX});
                    for (auto const &range : authored)
                        (void)layout->SetUnderline(TRUE, range);
                }
            }
        };
        ReadingSpan hover{};
        if (hoveredLink && frame.descriptors && *hoveredLink < frame.descriptors->links.size())
            hover = frame.descriptors->links[*hoveredLink].text;
        ImageDrawCache temporaryImages;
        auto cache = imageCache ? imageCache : &temporaryImages;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        StartupTrace::DrawDetails drawDetails{textCache};
#endif
        cache->Begin(context);
        if (textCache)
        {
            textCache->Begin(context);
            textCache->PrepareFrame(frame, D2D1::Point2F(originX, originY));
        }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        drawDetails.Prepared();
#endif
        struct ImageDrawScope
        {
            ImageDrawCache *cache;
            ~ImageDrawScope() noexcept
            {
                cache->End();
            }
        } imageDrawScope{cache};
        winrt::com_ptr<ID2D1SolidColorBrush> foreground, muted, panel, link, selection, selectionText, focus;
        winrt::check_hresult(context->CreateSolidColorBrush(Color(frame.theme.foreground), foreground.put()));
        winrt::check_hresult(context->CreateSolidColorBrush(Color(frame.theme.borderColor), muted.put()));
        winrt::check_hresult(context->CreateSolidColorBrush(Color(frame.theme.panelBackground), panel.put()));
        winrt::check_hresult(context->CreateSolidColorBrush(Color(frame.theme.linkForeground), link.put()));
        winrt::check_hresult(context->CreateSolidColorBrush(Color(frame.theme.selectionBackground), selection.put()));
        winrt::check_hresult(
            context->CreateSolidColorBrush(Color(frame.theme.selectionForeground), selectionText.put()));
        winrt::check_hresult(context->CreateSolidColorBrush(Color(frame.theme.focusColor), focus.put()));
        winrt::com_ptr<ID2D1SolidColorBrush> warningText, errorText, authorHighlight;
        auto resourceLabelBrush = [&](ID2D1SolidColorBrush *normal, bool chosen, bool failed, bool warning)
        {
            if (chosen)
                return selectionText.get();
            if (frame.theme.highContrast || (!failed && !warning))
                return normal;
            auto &brush = failed ? errorText : warningText;
            if (!brush)
                winrt::check_hresult(context->CreateSolidColorBrush(ResourceNoticeColor(frame.theme, failed), brush.put()));
            return brush.get();
        };
        auto drawResourceNotice = [&](D2D1_RECT_F const &bounds, bool chosen)
        {
            const auto radius = std::min(4.0f, std::min(bounds.right - bounds.left, bounds.bottom - bounds.top) * 0.5f);
            const auto box = D2D1::RoundedRect(bounds, radius, radius);
            context->FillRoundedRectangle(box, chosen ? selection.get() : panel.get());
            context->DrawRoundedRectangle(box, muted.get());
        };
        // Containers are independent of materialized leaves. Opaque tint avoids
        // progressively darkening the same surface for nested quote backgrounds.
        if (frame.quotes && !frame.quotes->empty() && frame.viewportHeight > 0)
        {
            auto background = Color(frame.theme.background);
            background.a = 1.0f;
            if (!frame.theme.highContrast)
            {
                constexpr float neutral = 127.0f / 255.0f;
                background.r = background.r * 0.9f + neutral * 0.1f;
                background.g = background.g * 0.9f + neutral * 0.1f;
                background.b = background.b * 0.9f + neutral * 0.1f;
            }
            winrt::com_ptr<ID2D1SolidColorBrush> quoteBackground, quoteBorder;
            winrt::check_hresult(context->CreateSolidColorBrush(background, quoteBackground.put()));
            winrt::check_hresult(context->CreateSolidColorBrush(
                Color(frame.theme.highContrast ? frame.theme.foreground : frame.theme.borderColor), quoteBorder.put()));
            // Intersect in double precision before converting the bounded segment
            // to D2D floats; a very tall document never creates a tall paint surface.
            auto viewTop = frame.viewportY - static_cast<double>(originY);
            const auto surfaceBottom = paintClip && std::isfinite(paintClip->bottom)
                ? static_cast<double>(paintClip->bottom) : frame.viewportHeight;
            auto viewBottom = viewTop + surfaceBottom;
            for (auto const &quote : *frame.quotes)
            {
                auto top = std::max(quote.y, viewTop);
                auto bottom = std::min(quote.y + quote.height, viewBottom);
                if (top >= bottom)
                    continue;
                auto left = originX + static_cast<float>(quote.x);
                auto right = originX + static_cast<float>(quote.x + quote.width);
                auto paintTop = originY + static_cast<float>(top - frame.viewportY);
                auto paintBottom = originY + static_cast<float>(bottom - frame.viewportY);
                auto fill = background;
                if (!frame.theme.highContrast && quote.callout != NOTEPADS_MARKDOWN_CALLOUT_NONE)
                {
                    const auto accent = CalloutBorder(quote.callout, frame.theme);
                    fill.r = std::lerp(fill.r, accent.r, 0.12f);
                    fill.g = std::lerp(fill.g, accent.g, 0.12f);
                    fill.b = std::lerp(fill.b, accent.b, 0.12f);
                }
                quoteBackground->SetColor(fill);
                context->FillRectangle(D2D1::RectF(left, paintTop, right, paintBottom), quoteBackground.get());
            }
            // Paint bars after all backgrounds so a nested container cannot erase
            // an ancestor's border in a width-constrained layout.
            for (auto const &quote : *frame.quotes)
            {
                auto top = std::max(quote.y, viewTop);
                auto bottom = std::min(quote.y + quote.height, viewBottom);
                if (top >= bottom || quote.border_width <= 0)
                    continue;
                auto left = originX + static_cast<float>(quote.x);
                auto width = static_cast<float>(quote.border_width);
                auto radius = std::min(2.0f, std::min(width, static_cast<float>(quote.height)) * 0.5f);
                // Extend a clipped end by its corner radius so the viewport does
                // not invent a new rounded quote endpoint while scrolling.
                auto roundedTop = std::max(quote.y, top - radius);
                auto roundedBottom = std::min(quote.y + quote.height, bottom + radius);
                auto paintTop = originY + static_cast<float>(top - frame.viewportY);
                auto paintBottom = originY + static_cast<float>(bottom - frame.viewportY);
                quoteBorder->SetColor(CalloutBorder(quote.callout, frame.theme));
                context->PushAxisAlignedClip(D2D1::RectF(left, paintTop, left + width, paintBottom),
                                             D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                context->FillRoundedRectangle(D2D1::RoundedRect(
                    D2D1::RectF(left, originY + static_cast<float>(roundedTop - frame.viewportY), left + width,
                               originY + static_cast<float>(roundedBottom - frame.viewportY)), radius, radius),
                    quoteBorder.get());
                context->PopAxisAlignedClip();
            }
        }
        std::array<winrt::com_ptr<ID2D1SolidColorBrush>, NOTEPADS_MARKDOWN_HIGHLIGHT_ROLE_COUNT> codeBrushes;
        winrt::com_ptr<ID2D1SolidColorBrush> codeLineAccent;
        winrt::com_ptr<IDWriteTextFormat> codeNumberFormat;
        auto selected = selectionEnd > selectionStart;
        if (!frame.main)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        ReadingVector<ReadingRect> selectedRectangles{ReadingAllocator<ReadingRect>{frame.main->Memory()}};
        if (selected || focused || markInactiveCaret)
            selectedRectangles = Rectangles(frame, selectionStart, selectionEnd, trailingCaret);
        else
            frame.main->ValidateRange(selectionStart, selectionEnd - selectionStart);
        for (auto &rectangle : selectedRectangles)
            rectangle = paintRectangle(rectangle);
        struct TableLayer
        {
            ID2D1DeviceContext *context{};
            bool axisAligned{};
            void Pop() noexcept
            {
                if (auto target = std::exchange(context, nullptr))
                {
                    if (axisAligned)
                        target->PopAxisAlignedClip();
                    else
                        target->PopLayer();
                }
            }
            ~TableLayer()
            {
                Pop();
            }
        };
        for (auto const &visual : frame.Blocks())
        {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            StartupTrace::DrawDetails::Block blockTrace{drawDetails, visual};
#endif
            auto const &block = visual.block;
            auto x = originX + static_cast<float>(block.x);
            auto y = originY + static_cast<float>(block.y - frame.viewportY);
            auto width = visual.htmlTable ? visual.tableWidth : static_cast<float>(block.width);
            auto height = static_cast<float>(block.height);
            winrt::com_ptr<ID2D1RoundedRectangleGeometry> tableMask;
            TableLayer tableLayer;
            const auto tableRadius = std::min(4.0f, std::min(width, height) * 0.5f);
            if (block.kind == 3 || block.kind == 4)
            {
                const auto radius = std::min(CodeBlockStyle::radius, std::min(width, height) * 0.5f);
                const auto box = D2D1::RoundedRect(D2D1::RectF(x, y, x + width, y + height), radius, radius);
                if (!frame.theme.highContrast)
                    panel->SetOpacity(CodeBlockStyle::panelOpacity);
                context->FillRoundedRectangle(box, panel.get());
                panel->SetOpacity(1.0f);
                if (width > 1 && height > 1)
                {
                    const auto borderRadius = std::max(0.0f, radius - 0.5f);
                    const auto border = D2D1::RoundedRect(D2D1::RectF(x + 0.5f, y + 0.5f, x + width - 0.5f,
                                                                     y + height - 0.5f), borderRadius, borderRadius);
                    muted->SetOpacity(frame.theme.highContrast ? 1.0f : 0.4f);
                    context->DrawRoundedRectangle(border, muted.get());
                    muted->SetOpacity(1.0f);
                }
            }
            if (!visual.codeLines.empty())
            {
                const auto clipTop = paintClip && std::isfinite(paintClip->top) ? paintClip->top : 0.0f;
                const auto clipBottom = paintClip && std::isfinite(paintClip->bottom)
                    ? paintClip->bottom : static_cast<float>(frame.viewportHeight);
                auto first = std::lower_bound(visual.codeLines.begin(), visual.codeLines.end(), clipTop - y,
                    [](auto const &line, float offset) { return line.bottom <= offset; });
                const auto viewEnd = clipBottom - y;
                const auto inset = CodeBlockStyle::LeftPadding(width);
                for (auto line = first; line != visual.codeLines.end() && line->top < viewEnd; ++line)
                {
                    if (line->highlighted && width > 2)
                    {
                        if (!codeLineAccent)
                        {
                            auto tint = Color(frame.theme.linkForeground);
                            tint.a = frame.theme.highContrast ? 1.0f : 0.14f;
                            winrt::check_hresult(context->CreateSolidColorBrush(tint, codeLineAccent.put()));
                        }
                        auto band = D2D1::RectF(x + 1, y + line->top, x + width - 1, y + line->bottom);
                        if (frame.theme.highContrast)
                            context->DrawRectangle(band, codeLineAccent.get());
                        else
                            context->FillRectangle(band, codeLineAccent.get());
                    }
                    if (line->number)
                    {
                        if (!codeNumberFormat)
                            codeNumberFormat = CodeNumberFormat(frame.theme);
                        auto label = std::to_wstring(line->number);
                        auto labelRect = D2D1::RectF(x + inset, y + line->top,
                            x + inset + visual.codeGutterWidth - CodeNumberGap, y + line->top + frame.theme.lineHeight);
                        context->DrawText(label.data(), static_cast<uint32_t>(label.size()), codeNumberFormat.get(),
                                          labelRect, muted.get(), D2D1_DRAW_TEXT_OPTIONS_CLIP);
                    }
                }
            }
            if (block.kind == 5)
            {
                context->DrawLine(D2D1::Point2F(x, y + 8), D2D1::Point2F(x + width, y + 8), muted.get());
            }
            // List markers belong to the gutter, outside any table body mask.
            for (size_t i = 0; i < visual.decorations.size(); ++i)
            {
                auto const &decoration = visual.decorations[i];
                auto markerX = MarkerOrigin(visual, originX);
                if (IsPreviewTaskDecoration(decoration.kind))
                {
                    auto top = y + visual.taskMarkerTop;
                    if (selected && decoration.logical_start >= selectionStart &&
                        decoration.logical_end <= selectionEnd)
                        context->FillRoundedRectangle(
                            D2D1::RoundedRect(D2D1::RectF(markerX + 5, top - 1, markerX + 21, top + 15), 4, 4),
                            selection.get());
                    context->DrawRoundedRectangle(
                        D2D1::RoundedRect(D2D1::RectF(markerX + 6, top, markerX + 20, top + 14), 3, 3),
                        muted.get(), 1.5f);
                    if (decoration.checked)
                    {
                        auto check = decoration.kind == NOTEPADS_MARKDOWN_PREVIEW_DECORATION_STATIC_HTML_CHECKBOX ? muted.get() : foreground.get();
                        context->DrawLine(D2D1::Point2F(markerX + 8, top + 7), D2D1::Point2F(markerX + 12, top + 11),
                                          check, 2);
                        context->DrawLine(D2D1::Point2F(markerX + 12, top + 11), D2D1::Point2F(markerX + 18, top + 3),
                                          check, 2);
                    }
                }
                else if (visual.markers[i])
                {
                    auto const &marker = visual.markers[i];
                    context->PushAxisAlignedClip(D2D1::RectF(markerX, y, markerX + marker.clipWidth, y + height),
                                                 D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                    context->DrawTextLayout(D2D1::Point2F(markerX + marker.x, y + marker.y), marker.get(), foreground.get(),
                                            D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
                    context->PopAxisAlignedClip();
                }
            }
            if (block.kind == 6)
            {
                if (visual.htmlTable)
                {
                    context->PushAxisAlignedClip(D2D1::RectF(x, y, x + width, y + height),
                                                 D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                    tableLayer.context = context;
                    tableLayer.axisAligned = true;
                    float rowY = y;
                    bool body{};
                    for (size_t row = 0; row < visual.rowHeights.size(); ++row)
                    {
                        if (!(visual.tableRowFlags[row] & NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_CAPTION))
                        {
                            if (body)
                            {
                                auto crossesBoundary = [&](uint32_t column)
                                {
                                    return visual.tableOwners[(row - 1) * block.columns + column] ==
                                           visual.tableOwners[row * block.columns + column];
                                };
                                for (uint32_t column = 0; column < block.columns;)
                                {
                                    if (crossesBoundary(column))
                                    {
                                        ++column;
                                        continue;
                                    }
                                    auto start = column++;
                                    while (column < block.columns && !crossesBoundary(column))
                                        ++column;
                                    context->DrawLine(D2D1::Point2F(x + visual.columnEdges[start], rowY),
                                                      D2D1::Point2F(x + visual.columnEdges[column], rowY), muted.get());
                                }
                            }
                            body = true;
                        }
                        rowY += visual.rowHeights[row];
                    }
                }
                else
                {
                    winrt::com_ptr<ID2D1Factory> factory;
                    context->GetFactory(factory.put());
                    winrt::check_hresult(factory->CreateRoundedRectangleGeometry(
                        D2D1::RoundedRect(D2D1::RectF(x, y, x + width, y + height), tableRadius, tableRadius),
                        tableMask.put()));
                    context->PushLayer(D2D1::LayerParameters1(D2D1::RectF(x, y, x + width, y + height), tableMask.get()),
                                       nullptr);
                    tableLayer.context = context;
                    float rowY = y;
                    panel->SetOpacity(frame.theme.highContrast ? 1.0f : 0.45f);
                    for (size_t row = 0; row < visual.rowHeights.size(); ++row)
                    {
                        auto next = rowY + visual.rowHeights[row];
                        if (!row || row % 2 == 0)
                            context->FillRectangle(D2D1::RectF(x, rowY, x + width, next), panel.get());
                        if (row)
                            context->DrawLine(D2D1::Point2F(x, rowY), D2D1::Point2F(x + width, rowY), muted.get());
                        rowY = next;
                    }
                    panel->SetOpacity(1.0f);
                    for (uint32_t column = 1; column < block.columns; ++column)
                    {
                        auto columnX = x + (column < visual.columnEdges.size() ? visual.columnEdges[column]
                                                                              : width * column / block.columns);
                        context->DrawLine(D2D1::Point2F(columnX, y), D2D1::Point2F(columnX, rowY), muted.get());
                    }
                }
            }
            // Paint author decorations below selection and glyphs, including cached text.
            for (auto const &text : visual.texts)
            {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                blockTrace.ObserveText(text);
#endif
                std::optional<D2D1_RECT_F> previousCode;
                for (auto const &code : text.inlineCode)
                {
                    auto bounds = code.bounds;
                    if (bounds.right <= bounds.left || bounds.bottom <= bounds.top)
                        continue;
                    // Separate semantic spans may share one shaping cluster.
                    // Its translucent panel and border should be painted once.
                    if (previousCode && previousCode->left == bounds.left && previousCode->top == bounds.top &&
                        previousCode->right == bounds.right && previousCode->bottom == bounds.bottom)
                        continue;
                    previousCode = bounds;
                    bounds.left += x + text.x;
                    bounds.right += x + text.x;
                    const auto paintY = textPaintY(text, y + text.y);
                    bounds.top += paintY;
                    bounds.bottom += paintY;
                    const auto radius = std::min(3.0f, std::min(bounds.right - bounds.left,
                                                                bounds.bottom - bounds.top) * 0.5f);
                    const auto box = D2D1::RoundedRect(bounds, radius, radius);
                    context->FillRoundedRectangle(box, panel.get());
                    // A subtle edge also distinguishes code inside table rows
                    // painted with the panel color. HC uses the system text edge.
                    if (bounds.right - bounds.left > 1 && bounds.bottom - bounds.top > 1)
                    {
                        const auto inset = D2D1::RectF(bounds.left + 0.5f, bounds.top + 0.5f,
                                                       bounds.right - 0.5f, bounds.bottom - 0.5f);
                        const auto borderRadius = std::max(0.0f, radius - 0.5f);
                        muted->SetOpacity(frame.theme.highContrast ? 1.0f : 0.25f);
                        context->DrawRoundedRectangle(D2D1::RoundedRect(inset, borderRadius, borderRadius),
                                                      muted.get(), 1.0f);
                        muted->SetOpacity(1.0f);
                    }
                }
                if (!text.highlights.empty())
                {
                    if (!authorHighlight)
                        winrt::check_hresult(context->CreateSolidColorBrush(HighlightBackground(frame.theme),
                                                                            authorHighlight.put()));
                    const auto paintY = textPaintY(text, y + text.y);
                    for (auto const &bounds : text.highlights)
                        context->FillRectangle(D2D1::RectF(x + text.x + bounds.left, paintY + bounds.top,
                                                           x + text.x + bounds.right, paintY + bounds.bottom), authorHighlight.get());
                }
            }
            if (selected)
            {
                for (auto const &rect : selectedRectangles)
                {
                    if (rect.start < block.logical_start || rect.end > block.logical_end)
                        continue;
                    bool task = std::any_of(visual.decorations.begin(), visual.decorations.end(),
                                            [&](auto const &decoration)
                                            {
                                                return IsPreviewTaskDecoration(decoration.kind) &&
                                                       decoration.logical_start == rect.start &&
                                                       decoration.logical_end == rect.end;
                                            });
                    bool inlineObject =
                        std::any_of(visual.texts.begin(), visual.texts.end(),
                                    [&](auto const &text)
                                    {
                                        auto matches = [&](auto const &object)
                                        {
                                            auto position = text.logicalStart + object.offset;
                                            return rect.start == position && rect.end == position + 1;
                                        };
                                        return std::any_of(text.images.begin(), text.images.end(), matches) ||
                                               std::any_of(text.math.begin(), text.math.end(), matches) ||
                                               std::any_of(text.diagrams.begin(), text.diagrams.end(), matches);
                                    });
                    if (!task && !inlineObject)
                        context->FillRectangle(D2D1::RectF(originX + rect.x, originY + rect.y,
                                                           originX + rect.x + rect.width,
                                                           originY + rect.y + rect.height),
                                               selection.get());
                }
            }
            auto highlight = CodeHighlights(startupHighlights ? startupHighlights : frame.highlights,
                                            startupHighlights ? NOTEPADS_MARKDOWN_CORE_OK : frame.highlightingStatus,
                                            frame.main, block,
                                            {visual.texts.data(), visual.texts.size()});
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            blockTrace.PretextDone(highlight.spans.size());
#endif
            for (auto const &text : visual.texts)
            {
                if (text.scriptDrawing)
                {
                    const auto origin = D2D1::Point2F(x + text.x, textPaintY(text, y + text.y));
                    const auto textIndex = static_cast<uint32_t>(&text - visual.texts.data());
                    // Admission changes optional work only. The authoritative
                    // script draw still owns clipping and every overhanging mark.
                    const auto canBuildOrPromote = text.scripts &&
                        static_cast<double>(origin.y) + text.scripts->VisualBottom() >= ordinaryCacheTop &&
                        static_cast<double>(origin.y) + text.scripts->VisualTop() <= ordinaryPaintBottom;
                    // Only independent short paragraphs enter one nearby viewport
                    // of optional worker preparation. Tables keep visible admission.
                    const auto preparationMargin = paintClip ? static_cast<double>(paintClip->bottom) - paintClip->top : 0;
                    const auto prepareNearby = !canBuildOrPromote && block.kind == 1 && visual.texts.size() == 1 &&
                        text.utf16Length <= 512 && text.scripts && std::isfinite(ordinaryCacheTop) &&
                        std::isfinite(ordinaryPaintBottom) && preparationMargin > 0 &&
                        static_cast<double>(origin.y) + text.scripts->VisualBottom() >= ordinaryCacheTop - preparationMargin &&
                        static_cast<double>(origin.y) + text.scripts->VisualTop() <= ordinaryPaintBottom + preparationMargin;
                    if (textCache && text.scripts->RubyParts().empty() &&
                        (!selected || selectionEnd <= text.logicalStart || selectionStart >= text.logicalEnd) &&
                        !(hover.start < text.logicalEnd && text.logicalStart < hover.end))
                    {
                        bool pending{};
                        if (textCache->TryDraw(context, frame, visual, text, textIndex, origin,
                                               foreground.get(), canBuildOrPromote, canBuildOrPromote,
                                               repaint, &pending, link.get(), prepareNearby)) continue;
                        if (allowPendingText && pending && DrawPendingText(context, text, origin, muted.get())) continue;
                    }
                    ScriptPaint paint;
                    paint.foreground = foreground.get();
                    paint.link = link.get();
                    paint.selectionForeground = selectionText.get();
                    paint.memory = frame.main->Memory();
                    auto hoverBegin = std::max<uint64_t>(hover.start, text.logicalStart);
                    auto hoverEnd = std::min<uint64_t>(hover.end, text.logicalEnd);
                    if (hoverBegin < hoverEnd)
                        paint.hover = DWRITE_TEXT_RANGE{static_cast<uint32_t>(hoverBegin - text.logicalStart),
                                                       static_cast<uint32_t>(hoverEnd - hoverBegin)};
                    if (selected)
                    {
                        auto begin = std::max<uint64_t>(selectionStart, text.logicalStart);
                        auto end = std::min<uint64_t>(selectionEnd, text.logicalEnd);
                        if (begin < end)
                            paint.selection = DWRITE_TEXT_RANGE{static_cast<uint32_t>(begin - text.logicalStart),
                                                               static_cast<uint32_t>(end - begin)};
                    }
                    text.scriptDrawing->Draw(context, text.layout.get(), origin, paint);
                }
                // Ordinary text uses DRAW_TEXT_OPTIONS_CLIP, so a layout whose
                // origin is below the paint clip cannot contribute ink. This is
                // deliberately one-sided: glyph overhang above the layout's end
                // is not estimated from nominal metrics. Inline objects and
                // script drawing keep their independent paint paths.
                else if (text.utf16Length &&
                         (!std::isfinite(y + text.y) || static_cast<double>(y + text.y) <= ordinaryPaintBottom))
                {
                    const auto textOrigin = D2D1::Point2F(x + text.x, textPaintY(text, y + text.y));
                    const auto textIndex = static_cast<uint32_t>(&text - visual.texts.data());
                    // A whole table may include hundreds of cells above the
                    // viewport. Keep their original draw path without admitting
                    // their pixels into the visible working set's bitmap cache.
                    // Nominal height controls admission only, never ink culling.
                    const auto cacheAllowed = textCache && text.bitmapCandidate &&
                        (!selected || selectionEnd <= text.logicalStart || selectionStart >= text.logicalEnd) &&
                        !(hover.start < text.logicalEnd && text.logicalStart < hover.end);
                    const auto canBuildOrPromote = static_cast<double>(textOrigin.y) + text.height >= ordinaryCacheTop;
                    auto drawOrdinary = [&]
                    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                        StartupTrace::DrawDetails::Block::Part setupPart{
                            blockTrace, StartupTrace::DrawDetails::OrdinaryPart::Setup};
#endif
                        winrt::check_hresult(text.layout->SetDrawingEffect(nullptr, {0, UINT32_MAX}));
                        if (highlight.row)
                        {
                            auto const localStart = text.logicalStart - highlight.row->logical_start;
                            auto const localEnd = text.logicalEnd - highlight.row->logical_start;
                            auto first = std::lower_bound(highlight.spans.begin(), highlight.spans.end(), localStart,
                                                          [](notepads_markdown_highlight_span const &span, uint64_t start)
                                                          { return uint64_t{span.start_utf16} + span.length_utf16 <= start; });
                            for (auto token = first; token != highlight.spans.end() && token->start_utf16 < localEnd; ++token)
                            {
                                if (token->role == NOTEPADS_MARKDOWN_HIGHLIGHT_PLAIN)
                                    continue;
                                auto &brush = codeBrushes[token->role];
                                if (!brush)
                                    winrt::check_hresult(context->CreateSolidColorBrush(
                                        Color(frame.theme.codeForeground[token->role]), brush.put()));
                                auto start = std::max<uint64_t>(token->start_utf16, localStart);
                                auto end = std::min<uint64_t>(uint64_t{token->start_utf16} + token->length_utf16, localEnd);
                                winrt::check_hresult(
                                    text.layout->SetDrawingEffect(brush.get(), {static_cast<uint32_t>(start - localStart),
                                                                                static_cast<uint32_t>(end - start)}));
                            }
                        }
                        LinkUnderline underline{text.authoredUnderlines};
                        for (auto const &range : text.links)
                        {
                            winrt::check_hresult(text.layout->SetDrawingEffect(link.get(), {range.start, range.length}));
                            auto start = std::max<uint64_t>(hover.start, text.logicalStart + range.start);
                            auto end = std::min<uint64_t>(hover.end, text.logicalStart + range.start + range.length);
                            if (start < end)
                                underline.Add(text.layout.get(), {static_cast<UINT32>(start - text.logicalStart),
                                                                  static_cast<UINT32>(end - start)});
                        }
                        if (underline.layout)
                            for (auto const &image : text.images)
                                if (!frame.ImagePixelsFor(image.object))
                                    winrt::check_hresult(text.layout->SetUnderline(FALSE, {image.offset, 1}));
                        if (selected)
                        {
                            auto begin = std::max<uint64_t>(selectionStart, text.logicalStart);
                            auto end = std::min<uint64_t>(selectionEnd, text.logicalEnd);
                            if (begin < end)
                                winrt::check_hresult(text.layout->SetDrawingEffect(
                                    selectionText.get(),
                                    {static_cast<UINT32>(begin - text.logicalStart), static_cast<UINT32>(end - begin)}));
                        }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                        setupPart.Stop();
#endif
                        // Static link colors must be recorded after their drawing effects.
                        // Hover/selection intersections bypass the cache above.
                        if (cacheAllowed && canBuildOrPromote)
                        {
                            bool pending{};
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                            if (blockTrace.MeasureOrdinaryCache([&]
                                {
                                    return textCache->TryDraw(context, frame, visual, text, textIndex, textOrigin,
                                                             foreground.get(), true, true, repaint, &pending);
                                })) return;
#else
                            if (textCache->TryDraw(context, frame, visual, text, textIndex, textOrigin,
                                                   foreground.get(), true, true, repaint, &pending)) return;
#endif
                            if (allowPendingText && pending && DrawPendingText(context, text, textOrigin, muted.get())) return;
                        }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                        {
                            StartupTrace::DrawDetails::Block::Part directPart{
                                blockTrace, StartupTrace::DrawDetails::OrdinaryPart::DirectDraw};
#endif
                        context->DrawTextLayout(textOrigin, text.layout.get(), foreground.get(),
                                                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT | D2D1_DRAW_TEXT_OPTIONS_CLIP);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                        }
#endif
                    };
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    if (!cacheAllowed || !blockTrace.MeasureOrdinaryCache([&]
                        {
                            return textCache->TryDraw(context, frame, visual, text, textIndex, textOrigin,
                                                      foreground.get(), false, canBuildOrPromote);
                        })) drawOrdinary();
#else
                    if (!cacheAllowed || !textCache->TryDraw(context, frame, visual, text, textIndex, textOrigin,
                                                            foreground.get(), false, canBuildOrPromote)) drawOrdinary();
#endif
                }
                for (auto const &image : text.images)
                {
                    auto left = image.x + x + text.x;
                    auto top = image.y + y + text.y;
                    auto imageSelected = selected && text.logicalStart + image.offset >= selectionStart &&
                                         text.logicalStart + image.offset + 1 <= selectionEnd;
                    auto rectangle = D2D1::RectF(left, top, left + image.width, top + image.height);
                    if (image.note)
                    {
                        context->FillRectangle(rectangle, imageSelected ? selection.get() : panel.get());
                        context->DrawRectangle(rectangle, muted.get());
                        auto labelOrigin = D2D1::Point2F(left + image.inset, top + image.inset);
                        context->DrawTextLayout(labelOrigin, image.label.get(), imageSelected ? selectionText.get() : foreground.get(),
                                                D2D1_DRAW_TEXT_OPTIONS_CLIP | D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
                    }
                    else if (auto const &pixels = frame.ImagePixelsFor(image.object))
                    {
                        cache->Draw(context, pixels, rectangle, frame.ImageFrameIndex(image.object));
                        // Selection remains visible around the object without
                        // painting a highlight over the decoded pixels.
                        if (imageSelected)
                            context->DrawRectangle(rectangle, selection.get(), 2.0f);
                    }
                    else if (image.iconSize > 0)
                    {
                        if (imageSelected)
                            context->FillRectangle(rectangle, selection.get());
                        auto icon = imageSelected ? selectionText.get() : foreground.get();
                        const auto iconLeft = left + image.inset;
                        const auto iconTop = top + (image.height - image.iconSize) * 0.5f;
                        const auto size = image.iconSize;
                        auto point = [&](float u, float v)
                        {
                            return D2D1::Point2F(iconLeft + size * u, iconTop + size * v);
                        };
                        const auto opacity = icon->GetOpacity();
                        if (!imageSelected && !frame.theme.highContrast)
                            icon->SetOpacity(image.pending ? 0.55f : 0.72f);
                        const auto stroke = std::min(1.0f, size * 0.15f);
                        auto const start = point(0.12f, 0.2f);
                        auto const end = point(0.88f, 0.8f);
                        context->DrawRectangle(D2D1::RectF(start.x, start.y, end.x, end.y), icon, stroke);
                        context->DrawLine(point(0.14f, 0.72f), point(0.40f, 0.45f), icon, stroke);
                        context->DrawLine(point(0.40f, 0.45f), point(0.68f, 0.80f), icon, stroke);
                        context->DrawLine(point(0.56f, 0.65f), point(0.72f, 0.53f), icon, stroke);
                        context->DrawLine(point(0.72f, 0.53f), point(0.88f, 0.70f), icon, stroke);
                        icon->SetOpacity(opacity);
                        if (image.label)
                        {
                            auto label = imageSelected ? selectionText.get() : image.linked ? link.get() : foreground.get();
                            LinkUnderline labelUnderline;
                            const auto position = text.logicalStart + image.offset;
                            if (image.linked && position >= hover.start && position < hover.end)
                                labelUnderline.Add(image.label.get(), {0, UINT32_MAX});
                            context->DrawTextLayout(D2D1::Point2F(iconLeft + size + image.inset, top), image.label.get(), label,
                                                    D2D1_DRAW_TEXT_OPTIONS_CLIP | D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
                        }
                    }
                }
                for (auto const &math : text.math)
                {
                    auto position = text.logicalStart + math.offset;
                    auto mathSelected = selected && position >= selectionStart && position + 1 <= selectionEnd;
                    auto left = x + text.x + math.x;
                    auto top = y + text.y + math.y;
                    auto rectangle = D2D1::RectF(left, top, left + math.width, top + math.height);
                    bool linked =
                        std::any_of(text.links.begin(), text.links.end(), [&](auto const &range)
                                    { return math.offset >= range.start && math.offset - range.start < range.length; });
                    auto brush = mathSelected ? selectionText.get() : linked ? link.get() : foreground.get();
                    if (mathSelected && (math.state == NativeMathState::Ready || !math.label))
                        context->FillRectangle(rectangle, selection.get());
                    if (math.state == NativeMathState::Ready)
                    {
                        auto anchor = math.anchor;
                        anchor.left += x + text.x;
                        anchor.baseline += y + text.y;
                        auto const &drawing = math.presentation->Drawing();
                        if (!paintClip || !MathReplayOutsidePaintClip(drawing->OccupiedEm(), math.placement, anchor,
                                                                     *paintClip, mathPaintMargin))
                            winrt::check_hresult(drawing->Replay(context, math.placement, anchor, brush->GetColor(),
                                                                mathSelected || frame.theme.highContrast));
                    }
                    else if (math.label)
                    {
                        drawResourceNotice(rectangle, mathSelected);
                        auto labelBrush = resourceLabelBrush(brush, mathSelected, math.state == NativeMathState::Failed,
                            math.state == NativeMathState::Limited || math.state == NativeMathState::Unsupported);
                        context->DrawTextLayout(D2D1::Point2F(left + 6, top + 3), math.label.get(), labelBrush,
                                                D2D1_DRAW_TEXT_OPTIONS_CLIP | D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
                    }
                }
                for (auto const &diagram : text.diagrams)
                {
                    auto position = text.logicalStart + diagram.offset;
                    auto chosen = selected && position >= selectionStart && position + 1 <= selectionEnd;
                    auto left = x + text.x + diagram.x;
                    auto top = y + text.y + diagram.y;
                    auto rectangle = D2D1::RectF(left, top, left + diagram.width, top + diagram.height);
                    auto state = diagram.state;
                    if (state == NativeDiagramState::Ready)
                    {
                        if (diagram.ordinal >= frame.diagramDrawOutcomes.size())
                            state = NativeDiagramState::Limited;
                        else
                        {
                            auto &outcome = frame.diagramDrawOutcomes[diagram.ordinal].status;
                            // The first draw of a new Frame can retry a failed SVG creation.
                            if (outcome.load(std::memory_order_relaxed) == S_FALSE &&
                                FAILED(diagram.drawing.status.load(std::memory_order_relaxed)))
                                diagram.drawing.Reset();
                            auto status = NativeDiagramDrawing::Draw(diagram.presentation, diagram.placement, context,
                                                                     left, top, diagram.drawing);
                            outcome.store(status, std::memory_order_release);
                            if (FAILED(status))
                                state = DiagramFailureState(status);
                        }
                    }
                    if (state != NativeDiagramState::Ready)
                    {
                        drawResourceNotice(rectangle, chosen);
                        auto label = diagram.label.get();
                        if (diagram.state == NativeDiagramState::Ready)
                        {
                            auto index = state == NativeDiagramState::Limited       ? 1u
                                         : state == NativeDiagramState::Unavailable ? 2u
                                                                                    : 0u;
                            label = diagram.drawFailureLabels[index].get();
                        }
                        if (label)
                        {
                            auto labelBrush = resourceLabelBrush(foreground.get(), chosen, state == NativeDiagramState::Failed,
                                state == NativeDiagramState::Limited || state == NativeDiagramState::Unsupported ||
                                state == NativeDiagramState::FontUnavailable);
                            context->PushAxisAlignedClip(rectangle, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                            context->DrawTextLayout(D2D1::Point2F(left + 6, top + 3), label, labelBrush,
                                                    D2D1_DRAW_TEXT_OPTIONS_CLIP |
                                                        D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
                            context->PopAxisAlignedClip();
                        }
                    }
                    if (chosen && state == NativeDiagramState::Ready)
                    {
                        // An opaque SVG page must not hide atomic selection.
                        // Keep the diagram legible under a thin themed overlay.
                        auto opacity = selection->GetOpacity();
                        selection->SetOpacity(0.10f);
                        context->FillRectangle(rectangle, selection.get());
                        selection->SetOpacity(opacity);
                        context->DrawRectangle(rectangle, selection.get(), 2.0f);
                    }
                }
            }
            if (tableMask)
            {
                tableLayer.Pop();
                if (width > 1 && height > 1)
                {
                    const auto inset = D2D1::RectF(x + 0.5f, y + 0.5f, x + width - 0.5f, y + height - 0.5f);
                    const auto radius = std::max(0.0f, tableRadius - 0.5f);
                    context->DrawRoundedRectangle(D2D1::RoundedRect(inset, radius, radius), muted.get(), 1.0f);
                }
            }
        }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        drawDetails.Report();
#endif
        if (frame.visibility)
            for (auto const &visual : frame.Blocks())
            {
                auto const &block = visual.block;
                auto index = frame.visibility->SummaryIndex(block.node);
                if (!index || visual.disclosureInset == 0)
                    continue;
                auto const &row = frame.visibility->disclosures[*index];
                const auto x = originX + static_cast<float>(block.x) + visual.disclosureInset * 0.5f;
                const auto y = originY + static_cast<float>(block.y - frame.viewportY) +
                    std::min(static_cast<float>(block.height) * 0.5f, frame.theme.lineHeight * 0.5f);
                const auto a = row.expanded ? D2D1::Point2F(x - 3.5f, y - 2.0f) : D2D1::Point2F(x - 2.0f, y - 3.5f);
                const auto b = row.expanded ? D2D1::Point2F(x, y + 2.0f) : D2D1::Point2F(x + 2.0f, y);
                const auto c = row.expanded ? D2D1::Point2F(x + 3.5f, y - 2.0f) : D2D1::Point2F(x - 2.0f, y + 3.5f);
                context->DrawLine(a, b, foreground.get(), 1.5f);
                context->DrawLine(b, c, foreground.get(), 1.5f);
            }
        if (focused && !selected)
        {
            for (auto const &rect : selectedRectangles)
                context->FillRectangle(D2D1::RectF(originX + rect.x, originY + rect.y, originX + rect.x + rect.width,
                                                   originY + rect.y + rect.height),
                                       foreground.get());
        }
        if (markInactiveCaret && !focused && !selected && !selectedRectangles.empty())
        {
            auto const &line = selectedRectangles.front();
            const auto left = std::max(0.0f, originX - 8.0f);
            context->FillRectangle(D2D1::RectF(left, originY + line.y, left + 2.0f,
                                               originY + line.y + line.height), muted.get());
        }
        if (focused && frame.descriptors && focusedAction.kind != ReadingActionKind::None)
        {
            ReadingSpan range{};
            if (focusedAction.kind == ReadingActionKind::Link && focusedAction.index < frame.descriptors->links.size())
                range = frame.descriptors->links[focusedAction.index].text;
            else if (focusedAction.kind == ReadingActionKind::Disclosure && focusedAction.index < frame.descriptors->disclosures.size())
                range = frame.descriptors->disclosures[focusedAction.index].text;
            else if ((focusedAction.kind == ReadingActionKind::Task || focusedAction.kind == ReadingActionKind::Diagram) &&
                     focusedAction.index < frame.descriptors->objects.size())
            {
                auto const &object = frame.descriptors->objects[focusedAction.index];
                if ((focusedAction.kind == ReadingActionKind::Task && object.kind == ReadingObjectKind::Task &&
                     !object.staticHtmlCheckbox && object.hasTaskAction) ||
                    (focusedAction.kind == ReadingActionKind::Diagram && object.kind == ReadingObjectKind::Diagram &&
                     frame.DiagramState(object.object) == NativeDiagramState::Ready))
                    range = object.text;
            }
            if (range.start == range.end)
                return;
            auto rectangles = Rectangles(frame, range.start, range.end);
            for (auto rect : rectangles)
            {
                rect = paintRectangle(rect);
                context->DrawRectangle(D2D1::RectF(originX + rect.x - 1, originY + rect.y - 1,
                                                   originX + rect.x + rect.width + 1,
                                                   originY + rect.y + rect.height + 1),
                                       focus.get(), 1.5f);
            }
        }
    }
}
