// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Resources/Images/NativeImageResources.h"
#include <d2d1_1.h>
#include <string_view>

namespace Notepads::Markdown::Native
{
    struct ImagePresentationEntry
    {
        notepads_markdown_image_entry state{};
        std::shared_ptr<ImagePixels const> pixels;
        uint32_t frameIndex{};
        uint32_t VisibleState() const noexcept
        {
            return state.state == NOTEPADS_MARKDOWN_IMAGE_READY && !pixels ? NOTEPADS_MARKDOWN_IMAGE_FAILED : state.state;
        }
    };
    struct ImagePresentationSnapshot
    {
        uint64_t textGeneration{};
        uint64_t geometryGeneration{};
        NativeImageSnapshot policy;
        // Stable object order, immutable for this text/policy snapshot.
        ReadingVector<ImagePresentationEntry> entries;
        explicit ImagePresentationSnapshot(std::shared_ptr<ReadingMemory> memory)
            : entries(ReadingAllocator<ImagePresentationEntry>{std::move(memory)})
        {
        }
        ImagePresentationEntry const *Find(uint32_t object) const noexcept;
    };
    std::wstring_view ImageStateDescription(uint32_t state, uint32_t failure, uint32_t detail = 0,
                                            uint32_t statusFlags = 0) noexcept;
    bool ValidImageMetadata(notepads_markdown_image_metadata const &metadata, uint64_t capacity) noexcept;

    // UI/device objects only: bounded mirrors of still-live CPU pixels, retained
    // across scroll frames. No strong CPU/model/request pins or admission policy;
    // unused slots may be replaced when this device's fixed table is full.
    class ImageDrawCache
    {
      public:
        void Begin(ID2D1DeviceContext *context);
        void Draw(ID2D1DeviceContext *context, std::shared_ptr<ImagePixels const> const &pixels,
                  D2D1_RECT_F const &destination, uint32_t frameIndex = 0);
        ID2D1Bitmap1 *Bitmap(ID2D1DeviceContext *context, std::shared_ptr<ImagePixels const> const &pixels,
                             uint32_t frameIndex = 0);
        void End() noexcept;
        void Clear() noexcept;

      private:
        struct Entry
        {
            std::weak_ptr<ImagePixels const> pixels;
            winrt::com_ptr<ID2D1Bitmap1> bitmap;
            uint32_t frameIndex{};
            bool seen{};
        };
        winrt::com_ptr<ID2D1Device> _device;
        std::array<Entry, NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES> _entries;
    };
}
