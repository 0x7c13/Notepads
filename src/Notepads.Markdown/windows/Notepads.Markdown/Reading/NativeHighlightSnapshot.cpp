// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Reading/NativeHighlightSnapshot.h"
#include <algorithm>
#include <cstring>
#include <limits>
#include <utility>

namespace Notepads::Markdown::Native
{
    namespace
    {
        template <typename T> T CorePod() noexcept
        {
            T value{};
            value.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, static_cast<uint32_t>(sizeof(T))};
            return value;
        }

        notepads_markdown_highlight_summary SummaryPod() noexcept
        {
            notepads_markdown_highlight_summary value{};
            value.header = {NOTEPADS_MARKDOWN_HIGHLIGHT_ABI_VERSION, sizeof(value)};
            return value;
        }

        void Check(int32_t status)
        {
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
                throw HighlightFailure{status};
        }

        void Require(bool condition)
        {
            if (!condition)
                throw HighlightFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        }

        void MatchIdentity(notepads_markdown_core_identity const &expected, notepads_markdown_core_identity const &actual)
        {
            if (std::memcmp(expected.document, actual.document, sizeof(expected.document)) != 0)
                throw HighlightFailure{NOTEPADS_MARKDOWN_CORE_WRONG_DOCUMENT};
            if (expected.revision != actual.revision)
                throw HighlightFailure{NOTEPADS_MARKDOWN_CORE_WRONG_REVISION};
            if (expected.generation != actual.generation)
                throw HighlightFailure{NOTEPADS_MARKDOWN_CORE_WRONG_GENERATION};
            if (expected.profile != actual.profile || expected.extensions != actual.extensions)
                throw HighlightFailure{NOTEPADS_MARKDOWN_CORE_STALE_RESULT};
        }

        void ValidateSummary(notepads_markdown_highlight_summary const &summary, notepads_markdown_core_identity const &identity)
        {
            Require(summary.header.abi_version == NOTEPADS_MARKDOWN_HIGHLIGHT_ABI_VERSION &&
                    summary.header.struct_size == sizeof(summary));
            MatchIdentity(identity, summary.model);
            Require(
                summary.blocks <= NOTEPADS_MARKDOWN_HIGHLIGHT_MAX_BLOCKS && summary.spans <= NOTEPADS_MARKDOWN_HIGHLIGHT_MAX_SPANS &&
                uint64_t{summary.complete_blocks} + summary.skipped_blocks + summary.failed_blocks == summary.blocks &&
                summary.entry_limited <= 1 && (!summary.entry_limited || summary.blocks == NOTEPADS_MARKDOWN_HIGHLIGHT_MAX_BLOCKS) &&
                summary.participating_utf8 <= 1'048'576 && summary.external_events <= 1'000'000);
        }

        void MatchSummary(notepads_markdown_highlight_summary const &expected, notepads_markdown_highlight_summary const &actual)
        {
            ValidateSummary(actual, expected.model);
            Require(expected.blocks == actual.blocks && expected.spans == actual.spans &&
                    expected.complete_blocks == actual.complete_blocks &&
                    expected.skipped_blocks == actual.skipped_blocks &&
                    expected.failed_blocks == actual.failed_blocks && expected.entry_limited == actual.entry_limited &&
                    expected.participating_utf8 == actual.participating_utf8 &&
                    expected.external_events == actual.external_events);
        }

        void ValidateRows(notepads_markdown_core_model_summary_result const &model, notepads_markdown_highlight_summary const &summary,
                          std::span<notepads_markdown_highlight_block const> blocks, std::span<notepads_markdown_highlight_span const> spans)
        {
            uint32_t complete{}, skipped{}, failed{}, consumed{};
            for (size_t index = 0; index < blocks.size(); ++index)
            {
                auto const &block = blocks[index];
                Require(block.node != 0 && block.node <= model.nodes &&
                        (!index || blocks[index - 1].node < block.node) && block.logical_start <= block.logical_end &&
                        block.logical_end <= model.main_utf16 && block.language <= NOTEPADS_MARKDOWN_HIGHLIGHT_MAX_LANGUAGE &&
                        block.spans_start == consumed &&
                        block.spans_count <= spans.size() - consumed);
                auto length = block.logical_end - block.logical_start;
                switch (block.state)
                {
                case NOTEPADS_MARKDOWN_HIGHLIGHT_COMPLETE:
                    ++complete;
                    Require(block.reason == 0 && block.language != 0 && length <= UINT32_MAX);
                    break;
                case NOTEPADS_MARKDOWN_HIGHLIGHT_SKIPPED:
                    ++skipped;
                    Require(block.reason >= 1 && block.reason <= 6 && block.spans_count == 0 &&
                            ((block.reason == 1) == (block.language == 0)));
                    break;
                case NOTEPADS_MARKDOWN_HIGHLIGHT_FAILED:
                    ++failed;
                    Require(block.reason >= 1 && block.reason <= 4 && block.language != 0 && block.spans_count == 0);
                    break;
                default:
                    // The private ABI rejects aborted builds instead of
                    // exporting partial/Pending rows as a usable result.
                    throw HighlightFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                }
                uint64_t position{};
                for (uint32_t offset = 0; offset < block.spans_count; ++offset)
                {
                    auto const &span = spans[consumed + offset];
                    Require(span.start_utf16 == position && span.length_utf16 != 0 &&
                            span.role < NOTEPADS_MARKDOWN_HIGHLIGHT_ROLE_COUNT && span.reserved == 0 &&
                            span.length_utf16 <= length - position);
                    position += span.length_utf16;
                }
                Require(block.state != NOTEPADS_MARKDOWN_HIGHLIGHT_COMPLETE || position == length);
                consumed += block.spans_count;
            }
            Require(consumed == spans.size() && complete == summary.complete_blocks &&
                    skipped == summary.skipped_blocks && failed == summary.failed_blocks);
        }
    }

    void NativeHighlightSnapshot::ResultDeleter::operator()(notepads_markdown_highlight_snapshot *value) const noexcept
    {
        if (value)
            (void)notepads_markdown_highlight_destroy(value);
    }

    void NativeHighlightCache::Deleter::operator()(notepads_markdown_highlight_cache *value) const noexcept
    {
        if (value)
            (void)notepads_markdown_highlight_cache_destroy(value);
    }

    std::shared_ptr<NativeHighlightSnapshot const> NativeHighlightSnapshot::Build(
        std::shared_ptr<ReadingMemory> memory, notepads_markdown_core_cancel *sourceFlag, std::span<uint32_t const> nodes,
        NativeHighlightCache *cache, uint32_t *reusedBlocks)
    {
        ModuleLifetime module;
        if (!memory || !sourceFlag || (cache == nullptr) != nodes.empty() ||
            nodes.size() > NOTEPADS_MARKDOWN_HIGHLIGHT_MAX_BLOCKS)
            throw HighlightFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        if (notepads_markdown_highlight_abi_version() != NOTEPADS_MARKDOWN_HIGHLIGHT_ABI_VERSION)
            throw HighlightFailure{NOTEPADS_MARKDOWN_CORE_ABI_MISMATCH};
        auto const model = memory->ModelOwner().Get();
        auto modelSummary = CorePod<notepads_markdown_core_model_summary_result>();
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_core_model_summary(model, &modelSummary, &error));
        MatchIdentity(memory->Identity(), modelSummary.identity);

        notepads_markdown_highlight_snapshot *raw{};
        uint32_t reused{};
        if (cache)
        {
            auto buildSelected = [&]
            {
                if (!cache->_cache)
                {
                    notepads_markdown_highlight_cache *handle{};
                    Check(notepads_markdown_highlight_cache_create(model, sourceFlag, &handle, &error));
                    cache->_cache.reset(handle);
                    Require(handle != nullptr);
                }
                return notepads_markdown_highlight_build_selected(model, sourceFlag, nodes.data(),
                    static_cast<uint32_t>(nodes.size()), cache->_cache.get(), &raw, &reused, &error);
            };
            auto status = buildSelected();
            if (status == NOTEPADS_MARKDOWN_CORE_WRONG_BUDGET)
            {
                // An accepted revision can install a fresh root. Retire all
                // old reservations before attempting admission under the new one.
                cache->_cache.reset();
                status = buildSelected();
            }
            // FFI poisons a cache after a panic. A later viewport can start
            // with a fresh cache after this request falls back to plain text.
            if (status == NOTEPADS_MARKDOWN_CORE_PANIC)
                cache->_cache.reset();
            Check(status);
        }
        else
            Check(notepads_markdown_highlight_build(model, sourceFlag, &raw, &error));
        ResultOwner result{raw};
        Require(static_cast<bool>(result));
        auto summary = SummaryPod();
        Check(notepads_markdown_highlight_get_summary(result.get(), &summary, &error));
        ValidateSummary(summary, memory->Identity());

        // ReadingAllocator reserves before HeapAlloc and admits actual HeapSize;
        // both complete native capacities remain paid alongside the Rust cache.
        ReadingVector<notepads_markdown_highlight_block> blocks{ReadingAllocator<notepads_markdown_highlight_block>{memory}};
        ReadingVector<notepads_markdown_highlight_span> spans{ReadingAllocator<notepads_markdown_highlight_span>{memory}};
        blocks.resize(summary.blocks);
        spans.resize(summary.spans);
        auto copied = SummaryPod();
        Check(notepads_markdown_highlight_copy(result.get(), blocks.data(), summary.blocks, sizeof(notepads_markdown_highlight_block), spans.data(),
                                summary.spans, sizeof(notepads_markdown_highlight_span), &copied, &error));
        // On any nonzero FFI result no partially written row is ever exposed.
        MatchSummary(summary, copied);
        ValidateRows(modelSummary, copied, blocks, spans);
        if (cache)
        {
            Require(nodes.size() == blocks.size() && reused <= copied.complete_blocks);
            for (size_t index = 0; index < nodes.size(); ++index)
                Require(nodes[index] == blocks[index].node);
        }
        if (reusedBlocks)
            *reusedBlocks = reused;
        auto allocator = ReadingAllocator<NativeHighlightSnapshot>{memory};
        return std::allocate_shared<NativeHighlightSnapshot>(allocator, ConstructionKey{}, std::move(memory),
                                                             std::move(result), copied, std::move(blocks),
                                                             std::move(spans));
    }

    NativeHighlightSnapshot::NativeHighlightSnapshot(ConstructionKey, std::shared_ptr<ReadingMemory> memory,
                                                     ResultOwner result, notepads_markdown_highlight_summary const &summary,
                                                     ReadingVector<notepads_markdown_highlight_block> blocks,
                                                     ReadingVector<notepads_markdown_highlight_span> spans) noexcept
        : _memory(std::move(memory)), _result(std::move(result)), _summary(summary), _blocks(std::move(blocks)),
          _spans(std::move(spans))
    {
    }

    notepads_markdown_highlight_summary const &NativeHighlightSnapshot::Summary() const noexcept
    {
        return _summary;
    }

    std::span<notepads_markdown_highlight_block const> NativeHighlightSnapshot::Blocks() const noexcept
    {
        return {_blocks.data(), _blocks.size()};
    }

    notepads_markdown_highlight_block const *NativeHighlightSnapshot::Find(uint32_t node) const noexcept
    {
        auto found =
            std::lower_bound(_blocks.begin(), _blocks.end(), node,
                             [](notepads_markdown_highlight_block const &block, uint32_t value) { return block.node < value; });
        return found == _blocks.end() || found->node != node ? nullptr : &*found;
    }

    std::span<notepads_markdown_highlight_span const> NativeHighlightSnapshot::Spans(notepads_markdown_highlight_block const &block) const
    {
        if (Find(block.node) != &block)
            throw HighlightFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        return std::span<notepads_markdown_highlight_span const>{_spans.data(), _spans.size()}.subspan(block.spans_start,
                                                                                        block.spans_count);
    }

    std::shared_ptr<ReadingMemory> const &NativeHighlightSnapshot::Memory() const noexcept
    {
        return _memory;
    }
}
