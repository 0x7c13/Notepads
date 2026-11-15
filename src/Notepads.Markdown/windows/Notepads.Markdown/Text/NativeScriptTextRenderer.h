// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Rendering/NativeDocumentRenderer.h"
#include <optional>
#include <span>

namespace Notepads::Markdown::Native
{
    class NativeScriptLayout;
    class ScriptDrawingEffect;


    struct ScriptPaint
    {
        ID2D1Brush *foreground{};
        ID2D1Brush *link{};
        ID2D1Brush *selectionForeground{};
        std::optional<DWRITE_TEXT_RANGE> selection;
        std::optional<DWRITE_TEXT_RANGE> hover;
        std::shared_ptr<ReadingMemory> memory;
    };

    // Prepare merges geometry-equivalent IR runs and installs permanent paint
    // properties before final metric queries. Links are canonical action spans,
    // not IR fragments. Seal warms the layout; Draw never changes it.
    class NativeScriptDrawing final
    {
        struct ConstructionKey
        {
        };

      public:
        static std::shared_ptr<NativeScriptDrawing> Create(
            notepads_markdown_preview_layout *owner, std::shared_ptr<NativeScriptLayout const> geometry,
            IDWriteFactory *factory, std::span<notepads_markdown_preview_run const> runs);
        NativeScriptDrawing(ConstructionKey, notepads_markdown_preview_layout *owner,
                            std::shared_ptr<NativeScriptLayout const> geometry,
                            IDWriteFactory *factory, std::span<notepads_markdown_preview_run const> runs);
        ~NativeScriptDrawing();
        void Prepare(IDWriteTextLayout *layout, std::span<PreviewLinkVisual const> links,
                     std::span<PreviewImageVisual const> images);
        void Seal(IDWriteTextLayout *layout);
        void Draw(ID2D1DeviceContext *context, IDWriteTextLayout *layout,
                  D2D1_POINT_2F origin, ScriptPaint const &paint) const;
        void Draw(ID2D1DeviceContext *context, IDWriteTextLayout *layout,
                  D2D1_POINT_2F origin, ID2D1Brush *foreground) const;

      private:
        notepads_markdown_preview_layout *_owner{};
        uint32_t _length{};
        bool _sealed{};
        std::shared_ptr<NativeScriptLayout const> _geometry;
        winrt::com_ptr<IDWriteFactory> _factory;
        winrt::com_ptr<IDWriteTextLayout> _layout;
        PreviewVector<notepads_markdown_preview_run> _runs;
        PreviewVector<winrt::com_ptr<ScriptDrawingEffect>> _effects;
    };
}
