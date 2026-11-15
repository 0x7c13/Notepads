// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Reading/NativeSourceNavigation.h"
#include <array>

namespace Notepads::Markdown::Native
{
    namespace
    {
        template <typename T> T Pod()
        {
            T value{};
            value.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(T)};
            return value;
        }
        void Check(int32_t status)
        {
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
                throw ReadingFailure{status};
        }
        struct QueryOwner
        {
            notepads_markdown_core_query *value{};
            ~QueryOwner() noexcept
            {
                if (value)
                    (void)notepads_markdown_core_query_destroy(value);
            }
        };
        std::optional<uint64_t> FollowingSourceAnchor(notepads_markdown_core_query *query,
            notepads_markdown_core_rows contributors, uint64_t sourceStart)
        {
            std::optional<uint64_t> anchor;
            std::array<notepads_markdown_core_contributor, 32> rows{};
            auto error = Pod<notepads_markdown_core_error>();
            for (uint64_t offset = 0; offset < contributors.count;)
            {
                auto count = std::min<uint64_t>(contributors.count - offset, rows.size());
                auto read = Pod<notepads_markdown_core_read_result>();
                Check(notepads_markdown_core_query_read(query, NOTEPADS_MARKDOWN_CORE_CONTRIBUTORS,
                    contributors.start + offset, count, sizeof(rows.front()), rows.data(), &read, &error));
                if (read.written != count)
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                for (size_t i = 0; i < count; ++i)
                {
                    auto const &row = rows[i];
                    if (row.source_start != sourceStart || row.source_end <= sourceStart)
                        continue;
                    if (row.correspondence != NOTEPADS_MARKDOWN_CORE_EXACT ||
                        (anchor && *anchor != row.target_start))
                        return {};
                    anchor = row.target_start;
                }
                offset += count;
            }
            return anchor;
        }

        template <size_t Count>
        std::array<ReadingSourcePosition, Count> MapPoints(MainTextSnapshot const &main, notepads_markdown_core_cancel *cancel,
                                                          bool fromSource, std::array<uint64_t, Count> const &offsets,
                                                          bool after, uint32_t *footnoteFlow = nullptr)
        {
            static_assert(Count == 1 || Count == 2);
            auto model = main.ModelOwner().Get();
            auto summary = Pod<notepads_markdown_core_model_summary_result>();
            auto error = Pod<notepads_markdown_core_error>();
            Check(notepads_markdown_core_model_summary(model, &summary, &error));
            if (footnoteFlow)
                *footnoteFlow = summary.has_footnotes ? summary.main_flow_id : 0;
            std::array<notepads_markdown_core_query_descriptor, Count> descriptors{};
            for (size_t index = 0; index < Count; ++index)
            {
                auto &descriptor = descriptors[index];
                descriptor = Pod<notepads_markdown_core_query_descriptor>();
                descriptor.identity = main.Identity();
                descriptor.domain = fromSource ? NOTEPADS_MARKDOWN_CORE_SOURCE_UTF8 : NOTEPADS_MARKDOWN_CORE_FLOW_UTF16;
                descriptor.target_kind = NOTEPADS_MARKDOWN_CORE_FLOW;
                descriptor.target_id = summary.main_flow_id;
                descriptor.affinity = after ? NOTEPADS_MARKDOWN_CORE_AFTER : NOTEPADS_MARKDOWN_CORE_BEFORE;
                descriptor.start = descriptor.end = offsets[index];
            }
            QueryOwner query;
            Check(notepads_markdown_core_map_points(model, cancel, descriptors.data(), static_cast<uint32_t>(Count), &query.value, &error));
            auto response = Pod<notepads_markdown_core_query_summary_result>();
            Check(notepads_markdown_core_query_summary(query.value, &response, &error));
            if (response.points != Count)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
            std::array<notepads_markdown_core_point, Count> points{};
            auto read = Pod<notepads_markdown_core_read_result>();
            Check(notepads_markdown_core_query_read(query.value, NOTEPADS_MARKDOWN_CORE_POINTS, 0, static_cast<uint64_t>(Count), sizeof(notepads_markdown_core_point),
                                     points.data(), &read, &error));
            if (read.written != Count)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
            std::array<ReadingSourcePosition, Count> result{};
            for (size_t index = 0; index < Count; ++index)
            {
                auto const &point = points[index];
                if (point.chosen.domain != static_cast<uint32_t>(fromSource ? NOTEPADS_MARKDOWN_CORE_FLOW_UTF16 : NOTEPADS_MARKDOWN_CORE_SOURCE_UTF8))
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                result[index] = {fromSource ? offsets[index] : point.chosen.value,
                                 fromSource ? point.chosen.value : offsets[index], point.correspondence, point.effects};
            }
            if constexpr (Count == 2)
            {
                if (fromSource && after && footnoteFlow && *footnoteFlow && offsets[0] < offsets[1] &&
                    result[0].previewUtf16Offset > result[1].previewUtf16Offset)
                {
                    // A moved footnote's synthetic tail can share the next
                    // line's source boundary. For a forward range, prefer
                    // the authored contributor on the inside of that range.
                    // The point API still exposes its full affinity envelope.
                    if (auto anchor = FollowingSourceAnchor(query.value, points[0].contributors, offsets[0]);
                        anchor && *anchor <= result[1].previewUtf16Offset)
                        result[0] = {offsets[0], *anchor, NOTEPADS_MARKDOWN_CORE_EXACT, 0};
                }
            }
            return result;
        }

        bool DiscontinuousRange(MainTextSnapshot const &main, notepads_markdown_core_cancel *cancel,
                                uint32_t flow, bool fromSource, uint64_t start, uint64_t end, bool after)
        {
            auto model = main.ModelOwner().Get();
            auto descriptor = Pod<notepads_markdown_core_query_descriptor>();
            descriptor.identity = main.Identity();
            descriptor.domain = fromSource ? NOTEPADS_MARKDOWN_CORE_SOURCE_UTF8 : NOTEPADS_MARKDOWN_CORE_FLOW_UTF16;
            descriptor.target_kind = NOTEPADS_MARKDOWN_CORE_FLOW;
            descriptor.target_id = flow;
            descriptor.affinity = after ? NOTEPADS_MARKDOWN_CORE_AFTER : NOTEPADS_MARKDOWN_CORE_BEFORE;
            descriptor.start = start;
            descriptor.end = end;
            QueryOwner query;
            auto error = Pod<notepads_markdown_core_error>();
            auto status = notepads_markdown_core_map_ranges(model, cancel, &descriptor, 1, &query.value, &error);
            if (status == NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT)
                return true;
            Check(status);
            auto response = Pod<notepads_markdown_core_query_summary_result>();
            Check(notepads_markdown_core_query_summary(query.value, &response, &error));
            if (response.ranges != 1 || response.selections != 1)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
            notepads_markdown_core_selection selection{};
            auto read = Pod<notepads_markdown_core_read_result>();
            Check(notepads_markdown_core_query_read(query.value, NOTEPADS_MARKDOWN_CORE_SELECTIONS, 0, 1,
                                                  sizeof(selection), &selection, &read, &error));
            if (read.written != 1)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
            return selection.related.spans.count > 1;
        }
    }

    ReadingSourcePosition MapReadingPosition(MainTextSnapshot const &main, notepads_markdown_core_cancel *cancel,
                                            bool fromSource, uint64_t offset, bool after)
    {
        return MapPoints(main, cancel, fromSource, std::array{offset}, after)[0];
    }
    ReadingSourceSpan MapReadingSpan(MainTextSnapshot const &main, notepads_markdown_core_cancel *cancel,
                                    bool fromSource, uint64_t start, uint64_t end, bool after)
    {
        if (start > end)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE};
        uint32_t footnoteFlow{};
        auto points = MapPoints(main, cancel, fromSource, std::array{start, end}, after, &footnoteFlow);
        bool discontinuous = points[1].sourceByteOffset < points[0].sourceByteOffset ||
                             points[1].previewUtf16Offset < points[0].previewUtf16Offset;
        if (!discontinuous && footnoteFlow && start < end)
            discontinuous = DiscontinuousRange(main, cancel, footnoteFlow, fromSource, start, end, after);
        return {points[0], points[1], discontinuous};
    }
}
