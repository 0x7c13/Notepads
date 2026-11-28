// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Reading/NativeReadingSnapshot.h"
#include "Core/ModuleLifetime.h"
#include "notepads/markdown/highlight.h"
#include <memory>
#include <span>

namespace Notepads::Markdown::Native
{
    struct HighlightFailure : NativeFailure
    {
        using NativeFailure::NativeFailure;
    };

    // A view owns this cache across editing revisions. Only its serialized
    // highlight worker accesses the Rust handle. The handle owns no model.
    class NativeHighlightCache final
    {
        friend class NativeHighlightSnapshot;
        struct Deleter
        {
            void operator()(notepads_markdown_highlight_cache *value) const noexcept;
        };
        ModuleLifetime _module;
        std::unique_ptr<notepads_markdown_highlight_cache, Deleter> _cache;
    };

    // Immutable native tokens, with no graphics/XAML objects or retained work
    // flag. A successful build does not authorize obsolete source publication.
    // Build and Rust handle access are serialized; native reads after Build may
    // run concurrently while callers retain a shared snapshot owner.
    class NativeHighlightSnapshot final
    {
        struct ConstructionKey
        {
        };
        struct ResultDeleter
        {
            void operator()(notepads_markdown_highlight_snapshot *value) const noexcept;
        };
        using ResultOwner = std::unique_ptr<notepads_markdown_highlight_snapshot, ResultDeleter>;

      public:
        static std::shared_ptr<NativeHighlightSnapshot const> Build(std::shared_ptr<ReadingMemory> memory,
                                                                    notepads_markdown_core_cancel *sourceFlag,
                                                                    std::span<uint32_t const> nodes = {},
                                                                    NativeHighlightCache *cache = nullptr,
                                                                    uint32_t *reusedBlocks = nullptr);
        NativeHighlightSnapshot(ConstructionKey, std::shared_ptr<ReadingMemory> memory, ResultOwner result,
                                notepads_markdown_highlight_summary const &summary, ReadingVector<notepads_markdown_highlight_block> blocks,
                                ReadingVector<notepads_markdown_highlight_span> spans) noexcept;
        NativeHighlightSnapshot(NativeHighlightSnapshot const &) = delete;
        NativeHighlightSnapshot &operator=(NativeHighlightSnapshot const &) = delete;

        notepads_markdown_highlight_summary const &Summary() const noexcept;
        std::span<notepads_markdown_highlight_block const> Blocks() const noexcept;
        notepads_markdown_highlight_block const *Find(uint32_t node) const noexcept;
        // Pass an actual row from this snapshot's Find/Blocks, not a row copy.
        // Views borrow this snapshot and must not outlive its shared owner.
        std::span<notepads_markdown_highlight_span const> Spans(notepads_markdown_highlight_block const &block) const;
        std::shared_ptr<ReadingMemory> const &Memory() const noexcept;

      private:
        // Reverse destruction frees native capacities before the Rust result,
        // then the model-root owner, with the module held throughout cleanup.
        ModuleLifetime _module;
        std::shared_ptr<ReadingMemory> _memory;
        ResultOwner _result;
        notepads_markdown_highlight_summary _summary{};
        ReadingVector<notepads_markdown_highlight_block> _blocks;
        ReadingVector<notepads_markdown_highlight_span> _spans;
    };
}
