// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Resources/Images/NativeImagePresentation.h"
#include <algorithm>
#include <bit>
#include <cmath>

namespace Notepads::Markdown::Native
{
    namespace
    {
        std::wstring_view SvgFailureDescription(uint32_t detail, bool ready = false) noexcept
        {
            // Stable transport kinds from the controlled SVG budget contract.
            // Every message is static: no source, path or arbitrary error text.
            switch (detail >> 16)
            {
            case 1:
                return ready ? L"Image loaded: SVG resolution update failed: native resource error"
                             : L"SVG not loaded: native resource error";
            case 2:
                switch (detail & 0xffff)
                {
                case 2:
                    return ready ? L"Image loaded: SVG resolution update failed: filter scratch limit"
                                 : L"SVG not loaded: filter scratch limit";
                case 3:
                    return ready ? L"Image loaded: SVG resolution update failed: nested image count limit"
                                 : L"SVG not loaded: nested image count limit";
                case 4:
                    return ready ? L"Image loaded: SVG resolution update failed: nested SVG depth limit"
                                 : L"SVG not loaded: nested SVG depth limit";
                case 5:
                    return ready ? L"Image loaded: SVG resolution update failed: nested image source limit"
                                 : L"SVG not loaded: nested image source limit";
                case 6:
                    return ready ? L"Image loaded: SVG resolution update failed: XML node limit"
                                 : L"SVG not loaded: XML node limit";
                case 7:
                case 8:
                    return ready ? L"Image loaded: SVG resolution update failed: XML attribute limit"
                                 : L"SVG not loaded: XML attribute limit";
                case 9:
                    return ready ? L"Image loaded: SVG resolution update failed: XML depth limit"
                                 : L"SVG not loaded: XML depth limit";
                case 10:
                    return ready ? L"Image loaded: SVG resolution update failed: namespace limit"
                                 : L"SVG not loaded: namespace limit";
                case 11:
                    return ready ? L"Image loaded: SVG resolution update failed: work limit"
                                 : L"SVG not loaded: work limit";
                case 13:
                    return ready ? L"Image loaded: SVG resolution update failed: source size limit"
                                 : L"SVG not loaded: source size limit";
                default:
                    return ready ? L"Image loaded: SVG resolution update failed: resource capacity limit"
                                 : L"SVG not loaded: resource capacity limit";
                }
            case 3:
            case 22:
                return ready ? L"Image loaded: SVG resolution update failed: allocation failed"
                             : L"SVG not loaded: allocation failed";
            case 4:
                return ready ? L"Image loaded: SVG resolution update failed: canceled" : L"SVG not loaded: canceled";
            case 5:
                return ready ? L"Image loaded: SVG resolution update failed: invalid SVG data"
                             : L"SVG not loaded: invalid SVG data";
            case 6:
                return ready ? L"Image loaded: SVG resolution update failed: unsupported SVG content"
                             : L"SVG not loaded: unsupported SVG content";
            case 7:
                return ready ? L"Image loaded: SVG resolution update failed: resource state error"
                             : L"SVG not loaded: resource state error";
            case 16:
                return ready ? L"Image loaded: SVG resolution update failed: external resource blocked"
                             : L"SVG not loaded: external resource blocked";
            case 17:
                return ready ? L"Image loaded: SVG resolution update failed: unsupported embedded image"
                             : L"SVG not loaded: unsupported embedded image";
            case 18:
                return ready ? L"Image loaded: SVG resolution update failed: invalid embedded image header"
                             : L"SVG not loaded: invalid embedded image header";
            case 19:
                return ready ? L"Image loaded: SVG resolution update failed: embedded image decode failed"
                             : L"SVG not loaded: embedded image decode failed";
            case 20:
                return ready ? L"Image loaded: SVG resolution update failed: malformed embedded image data"
                             : L"SVG not loaded: malformed embedded image data";
            case 21:
                return ready ? L"Image loaded: SVG resolution update failed: nested SVG decode failed"
                             : L"SVG not loaded: nested SVG decode failed";
            case 23:
                return ready ? L"Image loaded: SVG resolution update failed: invalid filter region"
                             : L"SVG not loaded: invalid filter region";
            case 24:
                return ready ? L"Image loaded: SVG resolution update failed: font conversion failed"
                             : L"SVG not loaded: font conversion failed";
            default:
                return ready ? L"Image loaded: SVG resolution update failed: resource or content error"
                             : L"SVG not loaded: resource or content error";
            }
        }
    }
    bool ValidImageMetadata(notepads_markdown_image_metadata const &value, uint64_t capacity) noexcept
    {
        if (value.reserved || !value.decoded_width || !value.decoded_height ||
            value.decoded_width > ImageMaximumDecodeSide || value.decoded_height > ImageMaximumDecodeSide ||
            uint64_t{value.decoded_width} * value.decoded_height > ImageMaximumPixelCount ||
            uint64_t{value.stride} < uint64_t{value.decoded_width} * 4 ||
            uint64_t{value.stride} * value.decoded_height > capacity || !std::isfinite(value.natural_width_dip) ||
            !std::isfinite(value.natural_height_dip) || value.natural_width_dip <= 0 || value.natural_height_dip <= 0 ||
            value.natural_width_dip > ImageMaximumIntrinsicSide ||
            value.natural_height_dip > ImageMaximumIntrinsicSide ||
            value.natural_width_dip * value.natural_height_dip > ImageMaximumPixelCount ||
            !std::isfinite(value.source_width) || !std::isfinite(value.source_height) || value.source_width <= 0 ||
            value.source_height <= 0 || value.source_width > value.decoded_width ||
            value.source_height > value.decoded_height)
            return false;
        if (value.kind == NOTEPADS_MARKDOWN_IMAGE_RASTER)
        {
            RasterImageMetadata raster{value.intrinsic_width, value.intrinsic_height, value.decoded_width,
                                       value.decoded_height,  value.orientation,      value.stride};
            auto const width = value.orientation >= 5 ? value.intrinsic_height : value.intrinsic_width;
            auto const height = value.orientation >= 5 ? value.intrinsic_width : value.intrinsic_height;
            return width && height && width <= ImageMaximumIntrinsicSide && height <= ImageMaximumIntrinsicSide &&
                   uint64_t{width} * height <= ImageMaximumPixelCount && value.orientation >= 1 &&
                   value.orientation <= 8 && value.decoded_width <= width && value.decoded_height <= height &&
                   SameImageMetadata(value, MakeRasterImageMetadata(raster));
        }
        if (value.kind != NOTEPADS_MARKDOWN_IMAGE_SVG || value.intrinsic_width || value.intrinsic_height ||
            value.orientation != 1 || (value.flags & ~uint32_t{NOTEPADS_MARKDOWN_IMAGE_RESOLUTION_LIMITED}))
            return false;
        auto const width = std::bit_cast<float>(value.viewport_width_bits);
        auto const height = std::bit_cast<float>(value.viewport_height_bits);
        auto const scale = std::bit_cast<float>(value.scale_bits);
        if (!std::isfinite(width) || !std::isfinite(height) || !std::isfinite(scale) || width <= 0 || height <= 0 ||
            scale <= 0)
            return false;
        auto const extentWidth = static_cast<double>(width * scale);
        auto const extentHeight = static_cast<double>(height * scale);
        return static_cast<float>(value.natural_width_dip) == width &&
               static_cast<float>(value.natural_height_dip) == height && value.source_width == extentWidth &&
               value.source_height == extentHeight && std::ceil(extentWidth) == value.decoded_width &&
               std::ceil(extentHeight) == value.decoded_height;
    }

    ImagePresentationEntry const *ImagePresentationSnapshot::Find(uint32_t object) const noexcept
    {
        auto found = std::lower_bound(entries.begin(), entries.end(), object,
                                      [](auto const &entry, auto id) { return entry.state.object < id; });
        return found != entries.end() && found->state.object == object ? &*found : nullptr;
    }
    std::wstring_view ImageStateDescription(uint32_t state, uint32_t failure, uint32_t detail,
                                            uint32_t statusFlags) noexcept
    {
        switch (state)
        {
        case NOTEPADS_MARKDOWN_IMAGE_UNREQUESTED:
            return L"Image not loaded";
        case NOTEPADS_MARKDOWN_IMAGE_PENDING:
            return L"Image loading";
        case NOTEPADS_MARKDOWN_IMAGE_READY:
            if (failure != NOTEPADS_MARKDOWN_IMAGE_NO_FAILURE)
                return detail ? SvgFailureDescription(detail, true) : L"Image loaded: resolution update failed";
            if (statusFlags & NOTEPADS_MARKDOWN_IMAGE_TARGET_LIMITED)
                return L"Image loaded: resolution limited";
            return L"Image loaded";
        case NOTEPADS_MARKDOWN_IMAGE_UNAVAILABLE:
            if (failure == NOTEPADS_MARKDOWN_IMAGE_RELEASED)
                return L"Image not loaded: released";
            return L"Image not loaded: the host did not provide it";
        case NOTEPADS_MARKDOWN_IMAGE_LIMITED:
            if (detail)
                return SvgFailureDescription(detail);
            return L"Image not loaded: resource limit";
        case NOTEPADS_MARKDOWN_IMAGE_FAILED:
            if (detail)
                return SvgFailureDescription(detail);
            if (failure == NOTEPADS_MARKDOWN_IMAGE_UNSUPPORTED)
                return L"Image not loaded: unsupported format";
            if (failure == NOTEPADS_MARKDOWN_IMAGE_CANCELLED)
                return L"Image not loaded: canceled";
            if (failure == NOTEPADS_MARKDOWN_IMAGE_RELEASED)
                return L"Image not loaded: released";
            return L"Image not loaded: resource error";
        default:
            return L"Image not loaded";
        }
    }
    void ImageDrawCache::Begin(ID2D1DeviceContext *context)
    {
        if (!context)
            throw winrt::hresult_error(E_POINTER);
        winrt::com_ptr<ID2D1Device> device;
        context->GetDevice(device.put());
        if (_device.get() != device.get())
        {
            Clear();
            _device = std::move(device);
        }
        for (auto &entry : _entries)
            entry.seen = false;
    }
    ID2D1Bitmap1 *ImageDrawCache::Bitmap(ID2D1DeviceContext *context, std::shared_ptr<ImagePixels const> const &pixels,
                                       uint32_t frameIndex)
    {
        if (!context || !pixels || pixels->pixels.empty())
            throw winrt::hresult_error(E_INVALIDARG);
        auto bytes = pixels->FramePixels(frameIndex);
        if (bytes.empty())
            throw winrt::hresult_error(E_INVALIDARG);
        Entry *available{};
        Entry *previousFrame{};
        for (auto &entry : _entries)
        {
            auto existing = entry.pixels.lock();
            if (existing && existing == pixels && entry.frameIndex == frameIndex)
            {
                entry.seen = true;
                return entry.bitmap.get();
            }
            // Prefer an empty/dead mirror slot over a still-live image that
            // may be encountered later in this same Frame. This is not an LRU.
            if (!entry.seen && (!available || !existing))
                available = &entry;
            if (!entry.seen && existing == pixels)
                previousFrame = &entry;
        }
        // Animation advances reuse their own unseen mirror before occupying
        // another slot or evicting an unrelated still-live image.
        if (previousFrame)
            available = previousFrame;
        if (!available)
            throw winrt::hresult_error(E_UNEXPECTED);
        auto const &metadata = pixels->metadata;
        auto maximumSide = context->GetMaximumBitmapSize();
        if (!metadata.decoded_width || !metadata.decoded_height || metadata.decoded_width > maximumSide ||
            metadata.decoded_height > maximumSide || uint64_t{metadata.stride} < uint64_t{metadata.decoded_width} * 4 ||
            uint64_t{metadata.stride} * metadata.decoded_height > bytes.size())
            throw winrt::hresult_error(E_UNEXPECTED);
        auto properties = D2D1::BitmapProperties1(
            D2D1_BITMAP_OPTIONS_NONE, D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), 96,
            96);
        winrt::com_ptr<ID2D1Bitmap1> bitmap;
        winrt::check_hresult(context->CreateBitmap(D2D1::SizeU(metadata.decoded_width, metadata.decoded_height),
                                                   bytes.data(), metadata.stride, properties, bitmap.put()));
        available->pixels = pixels;
        available->frameIndex = frameIndex;
        available->bitmap = std::move(bitmap);
        available->seen = true;
        return available->bitmap.get();
    }
    void ImageDrawCache::Draw(ID2D1DeviceContext *context, std::shared_ptr<ImagePixels const> const &pixels,
                              D2D1_RECT_F const &destination, uint32_t frameIndex)
    {
        if (!context || !pixels || !ValidImageMetadata(pixels->metadata, pixels->pixels.size()) ||
            !std::isfinite(destination.left) || !std::isfinite(destination.top) || !std::isfinite(destination.right) ||
            !std::isfinite(destination.bottom) || !std::isfinite(destination.right - destination.left) ||
            !std::isfinite(destination.bottom - destination.top) || destination.right <= destination.left ||
            destination.bottom <= destination.top)
            throw winrt::hresult_error(E_INVALIDARG);
        auto const &metadata = pixels->metadata;
        auto const source =
            D2D1::RectF(0, 0, static_cast<float>(metadata.source_width), static_cast<float>(metadata.source_height));
        context->DrawBitmap(Bitmap(context, pixels, frameIndex), destination, 1.0f, D2D1_INTERPOLATION_MODE_LINEAR, &source,
                            nullptr);
    }
    void ImageDrawCache::End() noexcept
    {
        for (auto &entry : _entries)
            // Scrolling away does not retire live CPU pixels. Keep their GPU
            // mirror until the real pixel owner expires or the device changes.
            if (entry.pixels.expired())
            {
                entry.bitmap = nullptr;
                entry.pixels.reset();
            }
    }
    void ImageDrawCache::Clear() noexcept
    {
        for (auto &entry : _entries)
        {
            entry.bitmap = nullptr;
            entry.pixels.reset();
            entry.seen = false;
        }
        _device = nullptr;
    }
}
