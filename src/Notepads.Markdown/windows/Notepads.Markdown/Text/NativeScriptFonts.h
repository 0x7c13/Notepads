// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include <dwrite.h>
#include <memory>
#include <span>
#include <cstdint>

struct notepads_markdown_preview_layout;
struct notepads_markdown_preview_run;

namespace Notepads::Markdown::Native
{
    struct NativeScriptFontState;
    struct NativeScriptFontBindingState;

    class NativeScriptFontBinding final
    {
        struct ConstructionKey {};
        friend class NativeScriptFonts;

      public:
        NativeScriptFontBinding(ConstructionKey, std::shared_ptr<NativeScriptFontBindingState> state);
        ~NativeScriptFontBinding();
        // Call after the final object replacements, metric queries and glyph
        // capture. Afterwards fallback may only replay this binding's mappings.
        void Freeze();
        bool Frozen() const noexcept;

      private:
        std::shared_ptr<NativeScriptFontBindingState> _state;
    };

    class NativeScriptFonts final
    {
        struct ConstructionKey {};

      public:
        // The factory must be frame-scoped and DWRITE_FACTORY_TYPE_ISOLATED.
        // Bindings retain it; never register these owner-funded loaders on the
        // process-wide shared factory. No shared state owns a binding/layout.
        static std::shared_ptr<NativeScriptFonts> Create(notepads_markdown_preview_layout *owner, IDWriteFactory *factory);
        NativeScriptFonts(ConstructionKey, notepads_markdown_preview_layout *owner, IDWriteFactory *factory);
        ~NativeScriptFonts();
        // The supplied factory and its original collections form one immutable
        // font epoch. Start a new provider when the collection expires; retained
        // frozen bindings replay saved mappings without consulting it again.
        // Apply family/weight/style/size first. This call preserves the text and
        // original collection semantics; subsequent object replacements are OK
        // until Resolve/capture/Freeze. Runs cover the complete UTF-16 text.
        // Additional barriers are proved real nonempty container boundaries,
        // strictly ascending, unique, inside (0,length), at Unicode scalar edges.
        // Arbitrary run/atom endpoints must not be supplied as barriers.
        std::shared_ptr<NativeScriptFontBinding> Bind(IDWriteTextLayout *layout,
            std::span<notepads_markdown_preview_run const> runs, uint32_t utf16Length,
            std::span<uint32_t const> barriers = {});

      private:
        std::shared_ptr<NativeScriptFontState> _state;
    };
}
