// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeImageTestSupport.h"
#include "NativeImageResourcesTests.h"
#include "Resources/Images/NativeImageResources.h"
#include "Resources/Images/NativeImageCoordinator.h"
#include "NativeImageFontControlTests.h"
#include "NativeSvgJobTests.h"
#include "NativeSvgRetainedTests.h"
#include <winrt/Windows.Storage.Streams.h>
#include <robuffer.h>
#include <algorithm>
#include <array>
#include <chrono>
#include <cstring>
#include <exception>
#include <source_location>
#include <string_view>
#include <thread>

namespace Notepads::Markdown::Native::ImageResourcesTests
{
    namespace
    {
        void Require(bool condition, std::wstring const &detail = {},
                     std::source_location location = std::source_location::current())
        {
            if (!condition)
                throw winrt::hresult_error{NativeStatusHresult(NOTEPADS_MARKDOWN_CORE_INVALID_STATE),
                                           winrt::hstring{L"Image diagnostic assertion at line " +
                                                          std::to_wstring(location.line()) +
                                                          (detail.empty() ? L"." : L": " + detail)}};
        }

        void Check(int32_t status, std::source_location location = std::source_location::current())
        {
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
                throw winrt::hresult_error{NativeStatusHresult(status),
                                           winrt::hstring{L"Image diagnostic core call at line " +
                                                          std::to_wstring(location.line()) + L": status=" +
                                                          std::to_wstring(status)}};
        }

        template <typename T> T Pod() noexcept
        {
            T value{};
            value.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, static_cast<uint32_t>(sizeof(T))};
            return value;
        }

        template <typename Predicate> bool Until(Predicate predicate)
        {
            auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(5);
            while (!predicate())
            {
                if (std::chrono::steady_clock::now() >= deadline)
                    return false;
                std::this_thread::sleep_for(std::chrono::milliseconds(1));
            }
            return true;
        }

        std::vector<uint8_t> Bmp()
        {
            // Original 2x2 24-bit BI_RGB fixture. Positive-height BMP is bottom
            // up; rows are BGR with 2 bytes padding. Top is red/green, bottom
            // blue/white. No tested decoder output generates this oracle.
            return {'B', 'M', 70, 0, 0,  0, 0,   0, 0, 0,   54,  0,   0, 0, 40, 0, 0,   0, 2,   0, 0, 0, 2, 0,
                    0,   0,   1,  0, 24, 0, 0,   0, 0, 0,   16,  0,   0, 0, 0,  0, 0,   0, 0,   0, 0, 0, 0, 0,
                    0,   0,   0,  0, 0,  0, 255, 0, 0, 255, 255, 255, 0, 0, 0,  0, 255, 0, 255, 0, 0, 0};
        }

        std::vector<uint8_t> Jpeg(uint32_t orientation)
        {
            // Original48x32 raster: two rows of three16px solid-color cells:
            // red/green/blue, yellow/magenta/cyan. Frozen JPEG generated using
            // Windows GDI+ JPEG encoder, quality100, from these independent RGB
            // constants; JPEG center samples permit12 channel levels of error.
            // Original encoded692B SHA256:
            // 21B9BC3834A502122B026D03E627A39D7E8D2BCE07DA77EFA0101139602991FF
            constexpr std::array<uint8_t, 692> encoded{
                0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x01, 0x00, 0x60, 0x00,
                0x60, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
                0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
                0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
                0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
                0x01, 0x01, 0x01, 0x01, 0xff, 0xdb, 0x00, 0x43, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
                0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
                0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
                0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
                0x01, 0x01, 0x01, 0x01, 0x01, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x20, 0x00, 0x30, 0x03, 0x01, 0x22,
                0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01, 0xff, 0xc4, 0x00, 0x1f, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01,
                0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05,
                0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0xff, 0xc4, 0x00, 0xb5, 0x10, 0x00, 0x02, 0x01, 0x03, 0x03, 0x02,
                0x04, 0x03, 0x05, 0x05, 0x04, 0x04, 0x00, 0x00, 0x01, 0x7d, 0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05,
                0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xa1, 0x08,
                0x23, 0x42, 0xb1, 0xc1, 0x15, 0x52, 0xd1, 0xf0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0a, 0x16, 0x17,
                0x18, 0x19, 0x1a, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43,
                0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64,
                0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x83, 0x84, 0x85,
                0x86, 0x87, 0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4,
                0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3,
                0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe1,
                0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8,
                0xf9, 0xfa, 0xff, 0xc4, 0x00, 0x1f, 0x01, 0x00, 0x03, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
                0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a,
                0x0b, 0xff, 0xc4, 0x00, 0xb5, 0x11, 0x00, 0x02, 0x01, 0x02, 0x04, 0x04, 0x03, 0x04, 0x07, 0x05, 0x04,
                0x04, 0x00, 0x01, 0x02, 0x77, 0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41,
                0x51, 0x07, 0x61, 0x71, 0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xa1, 0xb1, 0xc1, 0x09, 0x23,
                0x33, 0x52, 0xf0, 0x15, 0x62, 0x72, 0xd1, 0x0a, 0x16, 0x24, 0x34, 0xe1, 0x25, 0xf1, 0x17, 0x18, 0x19,
                0x1a, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47,
                0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68,
                0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88,
                0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7,
                0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6,
                0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe2, 0xe3, 0xe4, 0xe5,
                0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa, 0xff, 0xda, 0x00,
                0x0c, 0x03, 0x01, 0x00, 0x02, 0x11, 0x03, 0x11, 0x00, 0x3f, 0x00, 0xfc, 0x5f, 0xa2, 0x8a, 0x2b, 0xfc,
                0xa7, 0x3f, 0xef, 0xe0, 0xfe, 0x94, 0x28, 0xa2, 0x8a, 0xff, 0x00, 0x2d, 0xcf, 0xfc, 0x2b, 0xcf, 0xe0,
                0xde, 0x8a, 0x28, 0xaf, 0xfd, 0xa6, 0x0f, 0xfa, 0xb0, 0x3f, 0xd8, 0xa2, 0x8a, 0x28, 0xaf, 0xfc, 0x01,
                0xcf, 0xf5, 0x40, 0xfe, 0x0d, 0xe8, 0xa2, 0x8a, 0xff, 0x00, 0xda, 0x80, 0xff, 0x00, 0xab, 0x03, 0xfa,
                0x50, 0xa2, 0x8a, 0x2b, 0xfc, 0xb7, 0x3f, 0xf0, 0xa7, 0x3f, 0xff, 0xd9};
            Require(orientation == 5 || orientation == 7);
            std::array<uint8_t, 36> exif{0xff, 0xe1, 0x00, 0x22, 'E', 'x', 'i', 'f', 0,    0,    'I', 'I',
                                         0x2a, 0,    8,    0,    0,   0,   1,   0,   0x12, 0x01, 3,   0,
                                         1,    0,    0,    0,    0,   0,   0,   0,   0,    0,    0,   0};
            exif[28] = static_cast<uint8_t>(orientation);
            std::vector<uint8_t> result;
            result.reserve(encoded.size() + exif.size());
            result.insert(result.end(), encoded.begin(), encoded.begin() + 2);
            result.insert(result.end(), exif.begin(), exif.end());
            result.insert(result.end(), encoded.begin() + 2, encoded.end());
            return result;
        }

        std::vector<uint8_t> Gif(bool subrectangle)
        {
            // Original one-black-pixel GIF89a, global black/white palette.
            // LZW codes clear4, pixel0, end5 are packed as44 01. The second
            // case places that1x1 first frame at(1,1) in a2x2 logical canvas.
            auto side = static_cast<uint8_t>(subrectangle ? 2 : 1);
            auto offset = static_cast<uint8_t>(subrectangle ? 1 : 0);
            return {'G', 'I',  'F',    '8', '9',    'a', side, 0, side, 0, 0x80, 0, 0, 0,    0,    0, 255, 255,
                    255, 0x2c, offset, 0,   offset, 0,   1,    0, 1,    0, 0,    2, 2, 0x44, 0x01, 0, 0x3b};
        }

        std::vector<uint8_t> AnimatedGif(uint8_t repeats)
        {
            return {
                // 4x3 logical canvas, four global colors and loop count.
                0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x04, 0x00, 0x03, 0x00, 0x81, 0x00,
                0x00, 0x00, 0x00, 0x00, 0xd6, 0x4b, 0x33, 0x21, 0xa8, 0x7f, 0x26, 0x64,
                0xd7, 0x21, 0xff, 0x0b, 0x4e, 0x45, 0x54, 0x53, 0x43, 0x41, 0x50, 0x45,
                0x32, 0x2e, 0x30, 0x03, 0x01,
                repeats, 0x00, 0x00,
                // Full red frame, then a green and transparent patch at (1,1).
                0x21, 0xf9, 0x04, 0x04, 0x0a, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00,
                0x00, 0x04, 0x00, 0x03, 0x00, 0x00, 0x02, 0x0a, 0x0c, 0xc3, 0x30, 0x0c,
                0xc3, 0x30, 0x0c, 0xc3, 0x30, 0x05, 0x00, 0x21, 0xf9, 0x04, 0x09, 0x05,
                0x00, 0x00, 0x00, 0x2c, 0x01, 0x00, 0x01, 0x00, 0x02, 0x00, 0x01, 0x00,
                0x00, 0x02, 0x02, 0x14, 0x51, 0x00,
                // Blue patch at (0,1), then green at (3,2).
                0x21, 0xf9, 0x04, 0x0c, 0x03, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x01,
                0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x5c, 0x01, 0x00, 0x21,
                0xf9, 0x04, 0x04, 0x00, 0x00, 0x00, 0x00, 0x2c, 0x03, 0x00, 0x02, 0x00,
                0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x54, 0x01, 0x00, 0x3b,
            };
        }

        std::vector<uint8_t> ThinGif(uint8_t row)
        {
            std::vector<uint8_t> result{'G', 'I', 'F', '8', '9', 'a', 0, 1, 2, 0, 0x81, 0, 0,
                0, 0, 0, 0xd6, 0x4b, 0x33, 0x21, 0xa8, 0x7f, 0x26, 0x64, 0xd7};
            auto image = [&](uint8_t top, uint8_t height, std::array<uint8_t, 3> const &codes)
            {
                result.insert(result.end(), {0x21, 0xf9, 4, 4, 10, 0, 0, 0,
                                             0x2c, 0, 0, top, 0, 0, 1, height, 0, 0, 2});
                // Each 3-byte group encodes four pixels with a clear code before each index.
                std::vector<uint8_t> stream;
                for (uint32_t pixels = uint32_t{height} * 256; pixels; pixels -= 4)
                    stream.insert(stream.end(), codes.begin(), codes.end());
                stream.push_back(5);
                for (size_t at = 0; at < stream.size();)
                {
                    const auto count = std::min<size_t>(255, stream.size() - at);
                    result.push_back(static_cast<uint8_t>(count));
                    result.insert(result.end(), stream.begin() + at, stream.begin() + at + count);
                    at += count;
                }
                result.push_back(0);
            };
            image(0, 2, {0x0c, 0xc3, 0x30});
            image(row, 1, {0x14, 0x45, 0x51});
            result.push_back(0x3b);
            return result;
        }

        std::vector<uint8_t> LargePartialGif()
        {
            // A valid 3000x3000 canvas with two 1x1 updates needs separate
            // 36 MB canvas, patch and restore-previous scratch blocks when
            // scaled. Every block is below the 64 MiB per-pixel-block cap.
            std::vector<uint8_t> result{'G', 'I', 'F', '8', '9', 'a', 0xb8, 0x0b, 0xb8, 0x0b,
                0x80, 0, 0, 0, 0, 0, 255, 255, 255};
            for (auto disposal : {uint8_t{3}, uint8_t{1}})
                result.insert(result.end(), {0x21, 0xf9, 4, static_cast<uint8_t>(disposal << 2), 10, 0, 0, 0,
                    0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x44, 0x01, 0});
            result.push_back(0x3b);
            return result;
        }

        std::wstring Hex(uint32_t value)
        {
            constexpr wchar_t digits[] = L"0123456789ABCDEF";
            std::wstring result(10, L'0');
            result[1] = L'x';
            for (size_t index = 0; index < 8; ++index)
                result[9 - index] = digits[(value >> (index * 4)) & 15];
            return result;
        }

        std::wstring Describe(ImagePixels const &pixels)
        {
            auto const &metadata = pixels.metadata;
            std::wstring detail =
                L"intrinsic=" + std::to_wstring(metadata.intrinsic_width) + L"x" +
                std::to_wstring(metadata.intrinsic_height) + L", decoded=" + std::to_wstring(metadata.decoded_width) +
                L"x" + std::to_wstring(metadata.decoded_height) + L", orientation=" +
                std::to_wstring(metadata.orientation) + L", stride=" + std::to_wstring(metadata.stride) + L", size=" +
                std::to_wstring(pixels.pixels.size()) + L", capacity=" + std::to_wstring(pixels.pixels.capacity()) +
                L", first32B=";
            constexpr wchar_t digits[] = L"0123456789ABCDEF";
            for (size_t index = 0; index < std::min<size_t>(32, pixels.pixels.size()); ++index)
            {
                auto value = pixels.pixels[index];
                detail += digits[value >> 4];
                detail += digits[value & 15];
            }
            return detail;
        }

        std::wstring Describe(NativeImageDecodeResult const &result)
        {
            return L"failure=" + std::to_wstring(result.failure) + L", HRESULT=" +
                   Hex(static_cast<uint32_t>(result.error)) + L", nativeStatus=" +
                   std::to_wstring(result.nativeStatus) + L", closeHRESULT=" +
                   Hex(static_cast<uint32_t>(result.closeError)) + L", pixels=" +
                   (result.pixels ? Describe(*result.pixels) : L"null");
        }

        std::wstring Describe(NativeImageSnapshot const &snapshot)
        {
            auto const &header = snapshot.header;
            std::wstring detail =
                L"count=" + std::to_wstring(header.count) + L", active=" + std::to_wstring(header.active_requests) +
                L", reservations=" + std::to_wstring(header.reservations) + L", encoded=" +
                std::to_wstring(header.encoded_bytes) + L", decoded=" + std::to_wstring(header.decoded_bytes) +
                L", stateGeneration=" + std::to_wstring(header.state_generation);
            for (size_t index = 0; index < std::min<size_t>(header.count, 8); ++index)
            {
                auto const &entry = snapshot.entries[index];
                detail += L"; entry[" + std::to_wstring(index) + L"] object=" + std::to_wstring(entry.object) +
                          L", state=" + std::to_wstring(entry.state) + L", failure=" + std::to_wstring(entry.failure) +
                          L", HRESULT=" + Hex(static_cast<uint32_t>(entry.error));
            }
            return detail;
        }

        void CompleteReady(std::shared_ptr<NativeImageRequest> const &request, NativeImageDecodeResult const &result)
        {
            try
            {
                request->CompleteReady(result.pixels->metadata, result.pixels->charge);
            }
            catch (NativeFailure const &error)
            {
                throw winrt::hresult_error{error.code(),
                                           winrt::hstring{L"Image CompleteReady core status=" +
                                                          std::to_wstring(error.status) + L"; " + Describe(result)}};
            }
        }

        void CheckJpegPixels(ImagePixels const &pixels, uint32_t orientation)
        {
            Require(pixels.metadata.intrinsic_width == 48 && pixels.metadata.intrinsic_height == 32 &&
                        pixels.metadata.orientation == orientation && pixels.metadata.decoded_width == 32 &&
                        pixels.metadata.decoded_height == 48 && pixels.metadata.stride == 128 &&
                        pixels.pixels.size() == 32 * 48 * 4,
                    Describe(pixels));
            constexpr std::array<std::array<uint8_t, 3>, 6> rgb{
                {{255, 0, 0}, {0, 255, 0}, {0, 0, 255}, {255, 255, 0}, {255, 0, 255}, {0, 255, 255}}};
            auto order =
                orientation == 5 ? std::array<size_t, 6>{0, 3, 1, 4, 2, 5} : std::array<size_t, 6>{5, 2, 4, 1, 3, 0};
            auto close = [](uint8_t actual, uint8_t expected)
            { return (actual > expected ? actual - expected : expected - actual) <= 12; };
            for (size_t cell = 0; cell < order.size(); ++cell)
            {
                auto x = (cell % 2) * 16 + 8;
                auto y = (cell / 2) * 16 + 8;
                auto position = y * pixels.metadata.stride + x * 4;
                auto const &color = rgb[order[cell]];
                Require(close(pixels.pixels[position], color[2]) && close(pixels.pixels[position + 1], color[1]) &&
                            close(pixels.pixels[position + 2], color[0]) && pixels.pixels[position + 3] == 255,
                        L"EXIF=" + std::to_wstring(orientation) + L", cell=" + std::to_wstring(cell) +
                            L", actualBGRA=" +
                            Hex(pixels.pixels[position] | (uint32_t{pixels.pixels[position + 1]} << 8) |
                                (uint32_t{pixels.pixels[position + 2]} << 16) |
                                (uint32_t{pixels.pixels[position + 3]} << 24)) +
                            L"; " + Describe(pixels));
            }
        }

        constexpr std::array<uint8_t, 16> Expected{0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255};

        bool Matches(ImagePixels const &pixels)
        {
            return pixels.metadata.intrinsic_width == 2 && pixels.metadata.intrinsic_height == 2 &&
                   pixels.metadata.decoded_width == 2 && pixels.metadata.decoded_height == 2 &&
                   pixels.metadata.orientation == 1 && pixels.metadata.stride == 8 &&
                   pixels.pixels.size() == Expected.size() &&
                   std::equal(Expected.begin(), Expected.end(), pixels.pixels.begin());
        }

        winrt::Windows::Foundation::IAsyncOperation<winrt::Windows::Storage::Streams::IRandomAccessStream> Stream(
            std::vector<uint8_t> bytes)
        {
            using namespace winrt::Windows::Storage::Streams;
            InMemoryRandomAccessStream stream;
            DataWriter writer{stream};
            writer.WriteBytes(bytes);
            co_await writer.StoreAsync();
            (void)writer.DetachStream();
            writer.Close();
            stream.Seek(0);
            co_return stream;
        }

        CoreValidation::CheckResult Result(wchar_t const *name, std::exception_ptr failure, wchar_t const *detail)
        {
            CoreValidation::CheckResult result{name, !failure, NOTEPADS_MARKDOWN_CORE_OK, detail};
            if (!failure)
                return result;
            try
            {
                std::rethrow_exception(failure);
            }
            catch (NativeFailure const &error)
            {
                result.status = error.status;
                result.detail =
                    L"Image fixed fixture or ownership assertion failed (" + std::to_wstring(error.status) + L").";
            }
            catch (winrt::hresult_error const &error)
            {
                result.status = error.code().value;
                result.detail = error.message().c_str();
            }
            catch (...)
            {
                result.status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                result.detail = L"Unexpected image diagnostic exception.";
            }
            return result;
        }

        void Record(std::shared_ptr<CoreControllerTests::Results> const &results,
                    std::shared_ptr<CoreValidation::Context> const &context, CoreValidation::CheckResult result,
                    wchar_t const *name)
        {
            if (!result.passed)
                context->RecordPrimary(CoreValidation::FaultKind::Assertion, result.status, name);
            results->checks.push_back(std::move(result));
        }

        notepads_markdown_image_epoch Epoch(std::shared_ptr<ReadingMemory> const &memory, uint64_t generation)
        {
            notepads_markdown_image_epoch epoch{};
            auto const &identity = memory->Identity();
            std::copy(std::begin(identity.document), std::end(identity.document), std::begin(epoch.source.document));
            epoch.source.revision = identity.revision;
            epoch.source_generation = generation;
            epoch.provider_generation = 1;
            return epoch;
        }

        void RawEncodedFailureKeepsProcessCapacity(std::shared_ptr<ReadingMemory> const &memory)
        {
            auto policy = NativeImagePolicy::Create();
            policy->Begin(Epoch(memory, 1));
            constexpr uint64_t bytes = 16ull * 1024 * 1024;
            std::array<std::shared_ptr<NativeImagePolicy>, 3> paddingPolicies;
            std::array<NativeEncodedImageHandle, 6> padding;
            constexpr uint64_t paddingBytes = 6 * bytes;
            static_assert(paddingBytes + 2 * bytes == ImageProcessMaximumEncodedBytes);
            for (size_t index = 0; index < padding.size(); ++index)
            {
                auto &peer = paddingPolicies[index / 2];
                if (!peer)
                {
                    peer = NativeImagePolicy::Create();
                    peer->Begin(Epoch(memory, 1));
                }
                auto object = static_cast<uint32_t>(index % 2);
                peer->Desire(object, {1, 1, 512, 1});
                auto request = peer->Start(object, NOTEPADS_MARKDOWN_IMAGE_AUTO);
                padding[index] = NativeEncodedImage::Create(memory,
                    request->Reserve(NOTEPADS_MARKDOWN_IMAGE_ENCODED, bytes), bytes);
                constexpr std::array<uint8_t, 4096> page{};
                for (uint64_t filled = 0; filled < bytes; filled += page.size())
                    padding[index]->Append(page);
                padding[index]->Freeze();
                Require(padding[index]->Bytes().size() == bytes &&
                    ReadingMemory::InspectAllocation(padding[index].Get()).requestedTotal >= bytes);
            }
            auto make = [&](uint32_t object)
            {
                policy->Desire(object, {1, 1, 512, 1});
                auto request = policy->Start(object, NOTEPADS_MARKDOWN_IMAGE_AUTO);
                auto charge = request->Reserve(NOTEPADS_MARKDOWN_IMAGE_ENCODED, bytes);
                return NativeEncodedImage::Create(memory, charge, bytes);
            };
            auto first = make(0);
            auto second = make(1);
            auto global = Pod<notepads_markdown_image_process_capacity>();
            global.header = {NOTEPADS_MARKDOWN_IMAGE_ABI_VERSION, sizeof(global)};
            auto error = Pod<notepads_markdown_core_error>();
            Check(notepads_markdown_image_get_process_capacity(&global, &error));
            Require(global.encoded_bytes == ImageProcessMaximumEncodedBytes);
            auto generation = global.generation;
            auto physical = first.Get();
            auto original = ReadingMemory::InspectAllocation(physical);
            auto held = first;
            first.Reset();
            Require(policy->Snapshot().header.encoded_bytes == 2 * bytes);
            ReadingMemoryTestHooks hooks{};
            hooks.target = physical;
            hooks.failNextFree = true;
            auto previous = ReadingMemory::SetTestHooks(&hooks);
            held.Reset();
            ReadingMemory::SetTestHooks(previous);
            Check(notepads_markdown_image_get_process_capacity(&global, &error));
            Require(global.encoded_bytes == paddingBytes + 2 * bytes && policy->Snapshot().header.encoded_bytes == 2 * bytes,
                    L"Failed raw encoded HeapFree retains the full 128 MiB process and 32 MiB per-control charges.");
            auto failed = ReadingMemory::InspectAllocation(physical);
            Require(global.generation == generation && failed.hasAction && failed.lease == original.lease &&
                        failed.action.context == original.action.context && failed.requestedTotal == original.requestedTotal);
            auto other = NativeImagePolicy::Create();
            other->Begin(Epoch(memory, 1));
            other->Desire(2, {1, 1, 512, 1});
            auto denied = other->Start(2, NOTEPADS_MARKDOWN_IMAGE_AUTO);
            bool refused{};
            try { (void)denied->Reserve(NOTEPADS_MARKDOWN_IMAGE_ENCODED, 1); }
            catch (NativeImageFailure const &failure) { refused = failure.status == NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT; }
            Require(refused);
            denied->Fail(NOTEPADS_MARKDOWN_IMAGE_QUOTA, HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA));
            denied.reset();
            Require(ReadingMemory::TryFree(physical), L"Retry the original still-live prefix, never a reconstructed encoded object.");
            Check(notepads_markdown_image_get_process_capacity(&global, &error));
            Require(global.encoded_bytes == paddingBytes + bytes && global.generation > generation && policy->Snapshot().header.encoded_bytes == bytes);
            auto retry = other->Start(2, NOTEPADS_MARKDOWN_IMAGE_AUTO);
            auto recovered = NativeEncodedImage::Create(memory, retry->Reserve(NOTEPADS_MARKDOWN_IMAGE_ENCODED, 1), 1);
            recovered.Reset();
            retry.reset();
            second.Reset();
            for (auto &owner : padding) owner.Reset();
            for (auto &peer : paddingPolicies) peer.reset();
            Check(notepads_markdown_image_get_process_capacity(&global, &error));
            Require(global.encoded_bytes == 0 && policy->Snapshot().header.encoded_bytes == 0);
        }

        winrt::Windows::Foundation::IAsyncAction MetadataSurvivesRowReAdmission(std::shared_ptr<ReadingMemory> memory)
        {
            auto coordinator = std::make_shared<ImageCoordinator>();
            coordinator->policy = NativeImagePolicy::Create();
            coordinator->policy->Begin(Epoch(memory, 1));
            coordinator->bound = true;
            coordinator->textGeneration = 77;
            coordinator->intrinsics = std::allocate_shared<ImageCoordinator::Intrinsics>(
                ReadingAllocator<ImageCoordinator::Intrinsics>{memory}, memory);
            coordinator->intrinsics->textGeneration = coordinator->textGeneration;
            coordinator->intrinsics->entries.push_back({0});
            coordinator->policy->Desire(0, {1, 1, 512, 1});
            auto first = coordinator->policy->Start(0, NOTEPADS_MARKDOWN_IMAGE_AUTO);
            auto stream = co_await Stream(Bmp());
            auto decoded = std::make_shared<NativeImageDecodeResult>();
            auto action = DecodeImageAsync(first, memory, stream, 2048, decoded);
            co_await action;
            action = nullptr;
            Require(decoded->Ready());
            auto expected = decoded->pixels->metadata;
            first->CompleteReady(expected, decoded->pixels->charge);
            first.reset();
            coordinator->RememberMetadata(coordinator->policy->Snapshot());
            Require(coordinator->intrinsics->entries[0].valid);
            coordinator->policy->Evict(0);
            decoded->pixels.reset();
            Require(coordinator->policy->TryForget(0));
            for (uint32_t object = 1; object <= NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES; ++object)
                coordinator->policy->Desire(object, {1, 1, 512, 1});
            Require(coordinator->policy->TryForget(1));
            coordinator->policy->Desire(0, {1, 1, 512, 1});
            auto current = coordinator->policy->Start(0, NOTEPADS_MARKDOWN_IMAGE_AUTO);
            auto validate = [&](std::shared_ptr<ImagePresentationSnapshot const> const &presentation, uint32_t state)
            {
                auto row = presentation->Find(0);
                Require(row && row->state.state == state && !row->pixels && !row->state.published_request &&
                            (row->state.status_flags & NOTEPADS_MARKDOWN_IMAGE_HAS_METADATA) &&
                            !(row->state.status_flags & NOTEPADS_MARKDOWN_IMAGE_TARGET_SATISFIED) &&
                            SameImageMetadata(row->state.metadata, expected),
                        L"Re-admitted/failing row keeps current-source intrinsic geometry without Ready/pixel/publication authority.");
            };
            validate(coordinator->Freeze(memory), NOTEPADS_MARKDOWN_IMAGE_PENDING);
            stream = co_await Stream(std::vector<uint8_t>{'B', 'M', 0, 0});
            auto failed = std::make_shared<NativeImageDecodeResult>();
            action = DecodeImageAsync(current, memory, stream, 2048, failed);
            co_await action;
            action = nullptr;
            Require(!failed->Ready());
            current->Fail(failed->failure, failed->error, failed->detail);
            validate(coordinator->Freeze(memory), NOTEPADS_MARKDOWN_IMAGE_FAILED);
            current.reset();
            ++coordinator->textGeneration;
            // Keep the snapshot alive while inspecting its borrowed row.
            auto staleSnapshot = coordinator->Freeze(memory);
            auto stale = staleSnapshot->Find(0);
            Require(stale && !(stale->state.status_flags & NOTEPADS_MARKDOWN_IMAGE_HAS_METADATA));
        }

        winrt::Windows::Foundation::IAsyncAction Normal(std::shared_ptr<NativeImagePolicy> policy,
                                                        std::shared_ptr<ReadingMemory> memory,
                                                        std::shared_ptr<NativeImageDecodeResult> preserved)
        {
            policy->Begin(Epoch(memory, 1));
            auto request = StartImageTestRequest(policy, 0);
            auto result = std::make_shared<NativeImageDecodeResult>();
            auto stream = co_await Stream(Bmp());
            auto action = DecodeImageAsync(request, memory, stream, ImageMaximumDecodeSide, result);
            co_await action;
            action = nullptr;
            Require(result->Ready() && Matches(*result->pixels), L"BMP independent 16B oracle; " + Describe(*result));
            auto pending = policy->Snapshot();
            Require(pending.header.count == 1 && pending.header.active_requests == 1 &&
                        pending.header.encoded_bytes == 0 && pending.header.decoded_bytes == Expected.size() &&
                        pending.entries[0].state == NOTEPADS_MARKDOWN_IMAGE_PENDING,
                    L"BMP after decode; " + Describe(pending));
            CompleteReady(request, *result);
            request.reset();
            auto ready = policy->Snapshot();
            Require(ready.header.active_requests == 0 && ready.header.reservations == 1 &&
                        ready.entries[0].state == NOTEPADS_MARKDOWN_IMAGE_READY && ready.header.decoded_bytes == Expected.size(),
                    L"BMP after explicit request release; " + Describe(ready));
            // Retiring source rows does not release a still-owned native buffer.
            policy->Begin(Epoch(memory, 2));
            auto old = policy->Snapshot();
            Require(old.header.count == 0 && old.header.decoded_bytes == Expected.size() && Matches(*result->pixels),
                    L"BMP retained across source epoch; " + Describe(old) + L"; " + Describe(*result));
            preserved->pixels = result->pixels;
            memory.reset();
        }

        winrt::Windows::Foundation::IAsyncAction FailedInputs(std::shared_ptr<NativeImagePolicy> policy,
                                                              std::shared_ptr<ReadingMemory> memory)
        {
            struct Input
            {
                std::vector<uint8_t> bytes;
                uint32_t failure;
            };
            std::array<Input, 2> inputs{Input{{'B', 'M'}, NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR},
                                        Input{{'<', 's', 'v', 'g', '/', '>'}, NOTEPADS_MARKDOWN_IMAGE_UNSUPPORTED}};
            for (size_t index = 0; index < inputs.size(); ++index)
            {
                auto request = StartImageTestRequest(policy, static_cast<uint32_t>(index + 1));
                auto result = std::make_shared<NativeImageDecodeResult>();
                auto stream = co_await Stream(inputs[index].bytes);
                auto action = DecodeImageAsync(request, memory, stream, ImageMaximumDecodeSide, result);
                co_await action;
                action = nullptr;
                Require(!result->Ready() && !result->pixels && result->failure == inputs[index].failure,
                        L"bad/unsupported input=" + std::to_wstring(index) + L"; " + Describe(*result));
                request->Fail(result->failure, result->error);
                request.reset();
            }
            auto snapshot = policy->Snapshot();
            Require(snapshot.header.active_requests == 0 && snapshot.header.encoded_bytes == 0 &&
                        snapshot.header.decoded_bytes == Expected.size() && snapshot.header.reservations == 1,
                    L"bad/unsupported cleanup; " + Describe(snapshot));
            memory.reset();
        }

        winrt::Windows::Foundation::IAsyncAction XmlDispatch(std::shared_ptr<ReadingMemory> memory)
        {
            constexpr std::string_view svg = "<svg xmlns='http://www.w3.org/2000/svg' width='2' height='2'>"
                                             "<rect width='2' height='2' fill='#1256ab'/></svg>";
            auto bytes = [](std::string const &text) { return std::vector<uint8_t>(text.begin(), text.end()); };
            struct Input
            {
                std::vector<uint8_t> bytes;
                bool xml;
                bool cancelled{};
            };
            // Exact fixed upstream red-3x3 WebP, SHA256
            // 4537D5A20BAFCF14F462F79E79EE873A46EF41CB59F8C516D3278E24A50CE583.
            std::vector<uint8_t> webp{0x52, 0x49, 0x46, 0x46, 0x3c, 0x00, 0x00, 0x00, 0x57, 0x45, 0x42, 0x50, 0x56, 0x50, 0x38, 0x20, 0x30, 0x00, 0x00, 0x00, 0xd0, 0x01, 0x00, 0x9d, 0x01, 0x2a, 0x03, 0x00, 0x03, 0x00, 0x02, 0x00, 0x34, 0x25, 0xa0, 0x02, 0x74, 0xba, 0x01, 0xf8, 0x00, 0x03, 0xb0, 0x00, 0xfe, 0xf0, 0xc4, 0x0b, 0xff, 0x20, 0xb9, 0x61, 0x75, 0xc8, 0xd7, 0xff, 0x20, 0x3f, 0xe4, 0x07, 0xfc, 0x80, 0xff, 0xf8, 0xf2, 0x00, 0x00, 0x00};
            std::array<Input, 8> inputs{
                Input{bytes(std::string{svg}), true},
                Input{bytes(std::string{"\xef\xbb\xbf \t\r\n"} + std::string{svg}), true},
                Input{bytes(std::string{"<?xml version='1.0' encoding='UTF-8'?><!-- UTF8 \xe4\xb8\xad -->"} + std::string{svg}), true},
                Input{bytes(std::string(8193, ' ') + std::string{svg}), true},
                Input{std::move(webp), false}, Input{{}, false},
                Input{bytes(std::string(8193, ' ')), false},
                Input{bytes(std::string(8193, ' ') + std::string{svg}), false, true}};
            for (size_t index = 0; index < inputs.size(); ++index)
            {
                auto policy = NativeImagePolicy::Create();
                policy->Begin(Epoch(memory, index + 10));
                auto task = ImageLoadTask::Create(memory);
                struct FinishTask
                {
                    std::shared_ptr<ImageLoadTask> task;
                    ~FinishTask() { if (task) task->Finish(); }
                } finish{task};
                task->request = StartImageTestRequest(policy, 1, {1, 17, 512, 1});
                if (inputs[index].cancelled)
                    task->request->Cancel();
                auto result = std::shared_ptr<NativeImageDecodeResult>{task, &task->decodeResult};
                auto stream = co_await Stream(std::move(inputs[index].bytes));
                auto action = DecodeImageAsync(task->request, memory, stream, ImageMaximumDecodeSide, result, task,
                                               NativeSvgImageTarget{512, 1, 17, true});
                co_await action;
                action = nullptr;
                auto detail = L"XML dispatch input=" + std::to_wstring(index) + L"; " + Describe(*result);
                Require(SUCCEEDED(result->closeError), detail);
                if (inputs[index].xml)
                    Require(result->Ready() && result->svg && result->encoded && task->svgStarted && task->svgRetired &&
                                !task->svgQuarantined && result->svgPlan.natural_width_dip == 2 &&
                                result->svgPlan.natural_height_dip == 2 &&
                                std::any_of(result->pixels->pixels.begin(), result->pixels->pixels.end(),
                                            [](uint8_t value) { return value != 0; }), detail);
                else
                    Require(!result->Ready() && !result->svg && !result->encoded && !task->svgStarted &&
                                result->failure == static_cast<uint32_t>(inputs[index].cancelled ? NOTEPADS_MARKDOWN_IMAGE_CANCELLED : NOTEPADS_MARKDOWN_IMAGE_UNSUPPORTED) &&
                                result->detail == 0, detail);
                task->Finish();
                Require(task->finished.load(std::memory_order_acquire) && !task->request, detail);
                result.reset();
                task.reset();
                finish.task.reset();
                auto released = policy->Snapshot();
                Require(released.header.active_requests == 0 && released.header.encoded_bytes == 0 &&
                            released.header.decoded_bytes == 0 && released.header.reservations == 0, Describe(released));
            }
        }

        winrt::Windows::Foundation::IAsyncAction HugeDimensions(std::shared_ptr<NativeImagePolicy> policy,
                                                                std::shared_ptr<ReadingMemory> memory)
        {
            auto bytes = Bmp();
            bytes[18] = 1;
            bytes[19] = 32;
            // Preserve the original malformed counterexample: the header now
            // says 8193x2, but bfSize 70/biSizeImage 16 and pixels still describe 2x2.
            // WIC rejects that truncated payload before exposing dimensions.
            auto request = StartImageTestRequest(policy, 3);
            auto result = std::make_shared<NativeImageDecodeResult>();
            auto stream = co_await Stream(std::move(bytes));
            auto action = DecodeImageAsync(request, memory, stream, ImageMaximumDecodeSide, result);
            co_await action;
            action = nullptr;
            Require(!result->Ready() && !result->pixels && result->failure == NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR &&
                        result->error == static_cast<HRESULT>(0x88982F60u), // WINCODEC_ERR_BADIMAGE.
                    L"original truncated oversize BMP format error; " + Describe(*result));
            request->Fail(result->failure, result->error);
            request.reset();
            auto malformed = policy->Snapshot();
            Require(malformed.header.active_requests == 0 && malformed.header.encoded_bytes == 0 &&
                        malformed.header.decoded_bytes == Expected.size(),
                    L"truncated oversize BMP cleanup; " + Describe(malformed));

            // Independent, complete 24-bit BI_RGB fixture. Every byte of both
            // black rows and their padding exists; this tests the dimension
            // guard rather than malformed-file rejection. The quota expectation
            // remains strict and decoding must not allocate output pixels.
            constexpr uint32_t width = 8193, height = 2;
            constexpr uint32_t stride = (width * 3 + 3) & ~uint32_t{3};
            constexpr uint32_t imageBytes = stride * height, fileBytes = 54 + imageBytes;
            static_assert(stride == 24580 && imageBytes == 49160 && fileBytes == 49214);
            bytes.assign(fileBytes, 0);
            auto put32 = [&](size_t offset, uint32_t value)
            {
                for (size_t index = 0; index < 4; ++index)
                    bytes[offset + index] = static_cast<uint8_t>(value >> (index * 8));
            };
            bytes[0] = 'B';
            bytes[1] = 'M';
            put32(2, fileBytes);
            put32(10, 54);
            put32(14, 40);
            put32(18, width);
            put32(22, height);
            bytes[26] = 1;  // Planes.
            bytes[28] = 24; // Bits per pixel; compression remains BI_RGB 0.
            put32(34, imageBytes);
            request = StartImageTestRequest(policy, 6);
            stream = co_await Stream(std::move(bytes));
            action = DecodeImageAsync(request, memory, stream, ImageMaximumDecodeSide, result);
            co_await action;
            action = nullptr;
            Require(!result->Ready() && !result->pixels && result->failure == NOTEPADS_MARKDOWN_IMAGE_QUOTA,
                    L"complete legal 8193x2 BMP dimension guard; " + Describe(*result));
            request->Fail(result->failure, result->error);
            request.reset();
            auto snapshot = policy->Snapshot();
            Require(snapshot.header.active_requests == 0 && snapshot.header.encoded_bytes == 0 &&
                        snapshot.header.decoded_bytes == Expected.size(),
                    L"oversize BMP cleanup; " + Describe(snapshot));
            memory.reset();
        }

        struct ReadGate
        {
            using Operation = winrt::Windows::Foundation::IAsyncOperationWithProgress<winrt::Windows::Storage::Streams::IBuffer, uint32_t>;
            ReadRetirementCase scenario;
            winrt::handle entered, returnRead, finishRead, registering, returnRegistration, cancelling, returnCancel, decoderDone, physicalDone;
            std::atomic<uint32_t> active{}, closes{}, closeActive{}, cancelCalls{}, cancelActive{}, calls{};
            std::mutex mutex;
            Operation read{nullptr};
            winrt::Windows::Foundation::IAsyncAction decoder{nullptr};
            std::vector<uint8_t> bytes{Bmp()};
            size_t position{};
            static winrt::handle Event(bool signaled = false)
            {
                winrt::handle event{CreateEventExW(nullptr, nullptr,
                    CREATE_EVENT_MANUAL_RESET | (signaled ? CREATE_EVENT_INITIAL_SET : 0), EVENT_ALL_ACCESS)};
                winrt::check_bool(static_cast<bool>(event));
                return event;
            }
            explicit ReadGate(ReadRetirementCase value)
                : scenario(value), entered(Event()), returnRead(Event(value != ReadRetirementCase::CancelDuringCreate &&
                    value != ReadRetirementCase::LateCancel && value != ReadRetirementCase::CancelBeforeDecoderObserver)),
                  finishRead(Event(value == ReadRetirementCase::Synchronous || value == ReadRetirementCase::LateCancel ||
                                   value == ReadRetirementCase::CloseFailure || value == ReadRetirementCase::ReadAndCloseFailure)),
                  registering(Event()), returnRegistration(Event(value != ReadRetirementCase::CancelDuringRegistration)),
                  cancelling(Event()), returnCancel(Event(value != ReadRetirementCase::CancelTail)), decoderDone(Event()), physicalDone(Event())
            {
            }
            static void Wait(HANDLE event)
            {
                Require(WaitForSingleObjectEx(event, 5000, FALSE) == WAIT_OBJECT_0, L"bounded real image-read fixture gate");
            }
            bool Registered()
            {
                Operation operation{nullptr};
                {
                    std::lock_guard lock(mutex);
                    operation = read;
                }
                return operation && static_cast<bool>(operation.Completed());
            }
        };

        ReadGate::Operation PhysicalRead(std::shared_ptr<ReadGate> gate, winrt::Windows::Storage::Streams::IBuffer buffer,
                                         uint32_t count, bool first)
        {
            ++gate->active;
            struct PhysicalTail
            {
                ReadGate *gate;
                ~PhysicalTail()
                {
                    --gate->active;
                    SetEvent(gate->physicalDone.get());
                }
            } tail{gate.get()};
            if (first)
                co_await winrt::resume_on_signal(gate->finishRead.get());
            if (gate->scenario == ReadRetirementCase::ReadAndCloseFailure)
                throw winrt::hresult_error(E_INVALIDARG);
            auto length = static_cast<uint32_t>(std::min<size_t>(count, gate->bytes.size() - gate->position));
            if (gate->scenario == ReadRetirementCase::PartialRead)
                length = std::min(length, 13u);
            auto access = buffer.as<::Windows::Storage::Streams::IBufferByteAccess>();
            BYTE *destination{};
            winrt::check_hresult(access->Buffer(&destination));
            if (length)
                std::memcpy(destination, gate->bytes.data() + gate->position, length);
            gate->position += length;
            buffer.Length(length);
            // A completed coroutine frame must not create gate->read->gate.
            gate.reset();
            co_return buffer;
        }

        struct ReadOperation : winrt::implements<ReadOperation, ReadGate::Operation, winrt::Windows::Foundation::IAsyncInfo>
        {
            using Handler = winrt::Windows::Foundation::AsyncOperationWithProgressCompletedHandler<winrt::Windows::Storage::Streams::IBuffer, uint32_t>;
            using ProgressHandler = winrt::Windows::Foundation::AsyncOperationProgressHandler<winrt::Windows::Storage::Streams::IBuffer, uint32_t>;
            std::shared_ptr<ReadGate> gate;
            ReadGate::Operation operation;
            bool first;
            ReadOperation(std::shared_ptr<ReadGate> value, ReadGate::Operation read, bool initial)
                : gate(std::move(value)), operation(std::move(read)), first(initial)
            {
            }
            uint32_t Id() const
            {
                return operation.Id();
            }
            winrt::Windows::Foundation::AsyncStatus Status() const
            {
                return operation.Status();
            }
            winrt::hresult ErrorCode() const
            {
                return operation.ErrorCode();
            }
            void Close() const { operation.Close(); }
            void Cancel() const
            {
                ++gate->cancelCalls;
                ++gate->cancelActive;
                operation.Cancel();
                SetEvent(gate->cancelling.get());
                ReadGate::Wait(gate->returnCancel.get());
                --gate->cancelActive;
            }
            void Completed(Handler const &handler) const
            {
                if (first)
                {
                    SetEvent(gate->registering.get());
                    ReadGate::Wait(gate->returnRegistration.get());
                    if (gate->scenario == ReadRetirementCase::RegistrationFailure)
                        throw winrt::hresult_error(E_ACCESSDENIED);
                }
                operation.Completed(handler);
            }
            Handler Completed() const
            {
                return operation.Completed();
            }
            void Progress(ProgressHandler const &handler) const { operation.Progress(handler); }
            ProgressHandler Progress() const
            {
                return operation.Progress();
            }
            winrt::Windows::Storage::Streams::IBuffer GetResults() const
            {
                return operation.GetResults();
            }
        };

        struct ReadStream : winrt::implements<ReadStream, winrt::Windows::Storage::Streams::IRandomAccessStream,
            winrt::Windows::Storage::Streams::IInputStream, winrt::Windows::Storage::Streams::IOutputStream,
            winrt::Windows::Foundation::IClosable>
        {
            std::shared_ptr<ReadGate> gate;
            explicit ReadStream(std::shared_ptr<ReadGate> value) : gate(std::move(value))
            {
            }
            uint64_t Size() const
            {
                return gate->bytes.size();
            }
            void Size(uint64_t) const
            {
                throw winrt::hresult_not_implemented();
            }
            uint64_t Position() const
            {
                return gate->position;
            }
            bool CanRead() const
            {
                return true;
            }
            bool CanWrite() const
            {
                return false;
            }
            void Seek(uint64_t) const
            {
                throw winrt::hresult_not_implemented();
            }
            winrt::Windows::Storage::Streams::IInputStream GetInputStreamAt(uint64_t) const
            {
                throw winrt::hresult_not_implemented();
            }
            winrt::Windows::Storage::Streams::IOutputStream GetOutputStreamAt(uint64_t) const
            {
                throw winrt::hresult_not_implemented();
            }
            winrt::Windows::Storage::Streams::IRandomAccessStream CloneStream() const
            {
                throw winrt::hresult_not_implemented();
            }
            ReadGate::Operation ReadAsync(winrt::Windows::Storage::Streams::IBuffer const &buffer, uint32_t count,
                                          winrt::Windows::Storage::Streams::InputStreamOptions) const
            {
                auto first = gate->calls.fetch_add(1) == 0;
                auto operation = PhysicalRead(gate, buffer, count, first);
                if (first)
                {
                    {
                        std::lock_guard lock(gate->mutex);
                        gate->read = operation;
                    }
                    SetEvent(gate->entered.get());
                    ReadGate::Wait(gate->returnRead.get());
                    if (gate->scenario == ReadRetirementCase::PreCancelledRead)
                        operation.Cancel(); // Real pinned-runtime premature setter callback.
                }
                return winrt::make<ReadOperation>(gate, std::move(operation), first);
            }
            void Close() const
            {
                ++gate->closes;
                if (gate->active.load() || gate->cancelActive.load())
                    ++gate->closeActive;
                if (gate->scenario == ReadRetirementCase::LateCancel)
                {
                    winrt::Windows::Foundation::IAsyncAction decoder{nullptr};
                    {
                        std::lock_guard lock(gate->mutex);
                        decoder = gate->decoder;
                    }
                    decoder.Cancel();
                }
                if (gate->scenario == ReadRetirementCase::CloseFailure || gate->scenario == ReadRetirementCase::ReadAndCloseFailure)
                    throw winrt::hresult_error(E_ACCESSDENIED);
            }
            winrt::Windows::Foundation::IAsyncOperationWithProgress<uint32_t, uint32_t> WriteAsync(winrt::Windows::Storage::Streams::IBuffer const &) const
            {
                throw winrt::hresult_not_implemented();
            }
            winrt::Windows::Foundation::IAsyncOperation<bool> FlushAsync() const
            {
                throw winrt::hresult_not_implemented();
            }
        };

        winrt::Windows::Foundation::IAsyncAction CancelledAndStale(std::shared_ptr<NativeImagePolicy> policy,
                                                                   std::shared_ptr<ReadingMemory> memory)
        {
            auto request = StartImageTestRequest(policy, 4);
            request->Cancel();
            auto result = std::make_shared<NativeImageDecodeResult>();
            auto stream = co_await Stream(Bmp());
            auto action = DecodeImageAsync(request, memory, stream, ImageMaximumDecodeSide, result);
            co_await action;
            action = nullptr;
            Require(!result->pixels && result->failure == NOTEPADS_MARKDOWN_IMAGE_CANCELLED,
                    L"cancel before decode; " + Describe(*result));
            request->Fail(result->failure, result->error);
            request.reset();
            request = StartImageTestRequest(policy, 5);
            policy->Begin(Epoch(memory, 3));
            stream = co_await Stream(Bmp());
            action = DecodeImageAsync(request, memory, stream, ImageMaximumDecodeSide, result);
            co_await action;
            action = nullptr;
            Require(!result->pixels && result->failure == NOTEPADS_MARKDOWN_IMAGE_CANCELLED,
                    L"stale before decode; " + Describe(*result));
            auto pending = policy->Snapshot();
            Require(pending.header.count == 0 && pending.header.active_requests == 1,
                    L"stale caller still owns request; " + Describe(pending));
            request.reset();
            auto done = policy->Snapshot();
            Require(done.header.active_requests == 0 && done.header.encoded_bytes == 0 &&
                        done.header.decoded_bytes == Expected.size(),
                    L"cancel/stale cleanup; " + Describe(done));
            // Keep the existing diagnostic case count. These use the actual
            // decoder, real C++/WinRT child operations and the original policy.
            for (uint32_t scenario = 0; scenario <= static_cast<uint32_t>(ReadRetirementCase::PartialRead); ++scenario)
                Require(RunReadRetirementCase(memory, static_cast<ReadRetirementCase>(scenario)).passed,
                        L"actual decoder read-retirement scenario " + std::to_wstring(scenario));
            memory.reset();
        }

        winrt::Windows::Foundation::IAsyncAction ExifMirrors(std::shared_ptr<NativeImagePolicy> policy,
                                                             std::shared_ptr<ReadingMemory> memory)
        {
            for (auto orientation : {5u, 7u})
            {
                auto request = StartImageTestRequest(policy, 10 + orientation);
                auto result = std::make_shared<NativeImageDecodeResult>();
                auto stream = co_await Stream(Jpeg(orientation));
                auto action = DecodeImageAsync(request, memory, stream, ImageMaximumDecodeSide, result);
                co_await action;
                action = nullptr;
                Require(result->Ready(), L"EXIF=" + std::to_wstring(orientation) + L"; " + Describe(*result));
                CheckJpegPixels(*result->pixels, orientation);
                CompleteReady(request, *result);
                request.reset();
                auto ready = policy->Snapshot();
                Require(ready.header.active_requests == 0 && ready.header.encoded_bytes == 0 &&
                            ready.header.decoded_bytes == Expected.size() + 32 * 48 * 4,
                        L"EXIF retained pixels; " + Describe(ready));
                result->pixels.reset();
                auto released = policy->Snapshot();
                Require(released.header.decoded_bytes == Expected.size(), L"EXIF cleanup; " + Describe(released));
            }
            memory.reset();
        }

        winrt::Windows::Foundation::IAsyncAction GifCanvas(std::shared_ptr<NativeImagePolicy> policy,
                                                           std::shared_ptr<ReadingMemory> memory)
        {
            for (auto subrectangle : {false, true})
            {
                auto request = StartImageTestRequest(policy, subrectangle ? 31 : 30);
                auto result = std::make_shared<NativeImageDecodeResult>();
                auto stream = co_await Stream(Gif(subrectangle));
                auto action = DecodeImageAsync(request, memory, stream, ImageMaximumDecodeSide, result);
                co_await action;
                action = nullptr;
                const auto side = subrectangle ? 2u : 1u;
                Require(result->Ready() && !result->pixels->IsAnimated() &&
                            result->pixels->metadata.intrinsic_width == side &&
                            result->pixels->metadata.intrinsic_height == side &&
                            result->pixels->metadata.decoded_width == side &&
                            result->pixels->metadata.decoded_height == side && result->pixels->metadata.stride == side * 4 &&
                            result->pixels->pixels.size() == side * side * 4,
                        L"GIF static canvas; " + Describe(*result));
                auto pixels = result->pixels->FramePixels(0);
                for (size_t offset = 0; offset < pixels.size(); offset += 4)
                    Require(pixels[offset] == 0 && pixels[offset + 1] == 0 &&
                                pixels[offset + 2] == 0 && pixels[offset + 3] == 255,
                            L"GIF partial canvas is filled from its authored global background");
                CompleteReady(request, *result);
                request.reset();
                result->pixels.reset();
                auto done = policy->Snapshot();
                Require(done.header.active_requests == 0 && done.header.encoded_bytes == 0 &&
                            done.header.decoded_bytes == Expected.size(),
                        L"GIF cleanup; " + Describe(done));
            }
            for (auto repeats : {uint8_t{2}, uint8_t{0}})
            {
                auto request = StartImageTestRequest(policy, repeats ? 32 : 33);
                auto result = std::make_shared<NativeImageDecodeResult>();
                auto stream = co_await Stream(AnimatedGif(repeats));
                auto action = DecodeImageAsync(request, memory, stream, ImageMaximumDecodeSide, result);
                co_await action;
                action = nullptr;
                Require(result->Ready() && result->pixels->IsAnimated() && result->pixels->IsFrozen(),
                        L"GIF animated initialization; " + Describe(*result));
                auto const &pixels = *result->pixels;
                constexpr uint32_t red = 0xffd64b33, green = 0xff21a87f, blue = 0xff2664d7;
                constexpr std::array<std::array<uint32_t, 12>, 4> ExpectedFrames{{
                    {red, red, red, red, red, red, red, red, red, red, red, red},
                    {red, red, red, red, red, green, red, red, red, red, red, red},
                    {red, red, red, red, blue, 0, 0, red, red, red, red, red},
                    {red, red, red, red, red, 0, 0, red, red, red, red, green},
                }};
                for (uint32_t index = 0; index < ExpectedFrames.size(); ++index)
                {
                    auto expected = std::as_bytes(std::span{ExpectedFrames[index]});
                    auto actual = pixels.FramePixels(index);
                    Require(actual.size() == expected.size() && std::memcmp(actual.data(), expected.data(), expected.size()) == 0,
                            L"GIF partial update, transparency and disposal compose the exact canvas of frame " +
                                std::to_wstring(index));
                }
                Require(pixels.FramePixels(4).empty() && pixels.FrameAt(0).nextDelayMilliseconds == 100 &&
                            pixels.FrameAt(99).index == 0 && pixels.FrameAt(100).index == 1 &&
                            pixels.FrameAt(150).index == 2 && pixels.FrameAt(180).index == 3 &&
                            pixels.FrameAt(180).nextDelayMilliseconds == 100 && pixels.FrameAt(280).index == 0 &&
                            pixels.FrameAt(840).index == (repeats ? 3 : 0) &&
                            pixels.FrameAt(840).nextDelayMilliseconds == (repeats ? 0 : 100),
                        L"GIF respects all frame delays and finite/infinite playback boundaries");
                CompleteReady(request, *result);
                request.reset();
                Require(policy->Snapshot().header.decoded_bytes == Expected.size() + pixels.pixels.capacity(),
                        L"GIF retains only the immutable frames and schedule after worker cleanup");
                result->pixels.reset();
                auto done = policy->Snapshot();
                Require(done.header.active_requests == 0 && done.header.encoded_bytes == 0 &&
                            done.header.decoded_bytes == Expected.size(),
                        L"GIF animated cleanup; " + Describe(done));
            }
            for (auto row : {uint8_t{0}, uint8_t{1}})
            {
                auto request = StartImageTestRequest(policy, 34 + row, {1, 1, 128, 1});
                auto result = std::make_shared<NativeImageDecodeResult>();
                auto stream = co_await Stream(ThinGif(row));
                auto action = DecodeImageAsync(request, memory, stream, ImageMaximumDecodeSide, result);
                co_await action;
                action = nullptr;
                Require(result->Ready() && result->pixels->IsAnimated() &&
                            result->pixels->metadata.decoded_width == 128 && result->pixels->metadata.decoded_height == 1,
                        L"GIF thin-patch test uses the real half-resolution decode bucket; " + Describe(*result));
                auto pixels = result->pixels->FramePixels(1);
                Require(pixels.size() == 128 * 4, L"GIF scaled canvas has exactly one output row");
                constexpr std::array<uint8_t, 4> mixed{89, 122, 124, 255};
                for (size_t offset = 0; offset < pixels.size(); offset += 4)
                    Require(std::equal(mixed.begin(), mixed.end(), pixels.begin() + offset),
                            L"Both authored stripe rows contribute the same half coverage after composition and scaling");
                CompleteReady(request, *result);
                request.reset();
                result->pixels.reset();
                Require(policy->Snapshot().header.decoded_bytes == Expected.size(),
                        L"GIF intrinsic compositor workspace is retired with all scaling buffers");
            }
            {
                auto request = StartImageTestRequest(policy, 36, {1, 1, 64, 1});
                auto result = std::make_shared<NativeImageDecodeResult>();
                auto stream = co_await Stream(LargePartialGif());
                auto action = DecodeImageAsync(request, memory, stream, ImageMaximumDecodeSide, result);
                co_await action;
                action = nullptr;
                Require(result->Ready() && result->pixels->IsAnimated() &&
                            result->pixels->metadata.intrinsic_width == 3000 &&
                            result->pixels->metadata.intrinsic_height == 3000 &&
                            result->pixels->metadata.decoded_width == 128 &&
                            result->pixels->metadata.decoded_height == 128 &&
                            result->pixels->FramePixels(2).empty(),
                        L"GIF composition admits separate full-canvas scratch blocks; " + Describe(*result));
                for (uint32_t index = 0; index < 2; ++index)
                {
                    auto frame = result->pixels->FramePixels(index);
                    Require(frame.size() == 128 * 128 * 4, L"GIF animation exposes one complete BGRA canvas per frame");
                    for (size_t offset = 0; offset < frame.size(); offset += 4)
                        Require(frame[offset] == 0 && frame[offset + 1] == 0 && frame[offset + 2] == 0 &&
                                    frame[offset + 3] == 255,
                                L"GIF large partial updates and restore-previous preserve the authored opaque background");
                }
                CompleteReady(request, *result);
                request.reset();
                result->pixels.reset();
                Require(policy->Snapshot().header.decoded_bytes == Expected.size(),
                        L"GIF separate canvas scratch returns all physical capacity");
            }
            memory.reset();
        }
    }

    ReadRetirementResult RunReadRetirementCase(std::shared_ptr<ReadingMemory> const &memory, ReadRetirementCase scenario)
    {
        Require(static_cast<bool>(memory), L"read retirement uses an actual accepted model-root allocator");
        auto gate = std::allocate_shared<ReadGate>(ReadingAllocator<ReadGate>{memory}, scenario);
        auto policy = NativeImagePolicy::Create();
        policy->Begin(Epoch(memory, 700 + static_cast<uint32_t>(scenario)));
        auto task = ImageLoadTask::Create(memory);
        task->request = StartImageTestRequest(policy, 0);
        auto result = std::shared_ptr<NativeImageDecodeResult>{task, &task->decodeResult};
        auto stream = winrt::make<ReadStream>(gate);
        auto action = DecodeImageAsync(task->request, memory, stream, ImageMaximumDecodeSide, result, task);
        {
            std::lock_guard lock(task->mutex);
            task->decode = action;
        }
        {
            std::lock_guard lock(gate->mutex);
            gate->decoder = action;
        }
        if (scenario != ReadRetirementCase::CancelBeforeDecoderObserver)
            action.Completed([gate](auto const &, auto) { SetEvent(gate->decoderDone.get()); });
        struct Rescue
        {
            std::shared_ptr<ReadGate> gate;
            ~Rescue()
            {
                SetEvent(gate->returnRead.get());
                SetEvent(gate->returnRegistration.get());
                SetEvent(gate->returnCancel.get());
                SetEvent(gate->finishRead.get());
                (void)WaitForSingleObjectEx(gate->decoderDone.get(), 5000, FALSE);
                if (gate->active.load())
                    (void)WaitForSingleObjectEx(gate->physicalDone.get(), 5000, FALSE);
                std::lock_guard lock(gate->mutex);
                gate->read = nullptr;
                gate->decoder = nullptr;
            }
        } rescue{gate};
        ReadGate::Wait(gate->entered.get());
        const auto cancelled = scenario == ReadRetirementCase::CancelPending || scenario == ReadRetirementCase::CancelDuringCreate ||
                               scenario == ReadRetirementCase::CancelDuringRegistration || scenario == ReadRetirementCase::CancelTail ||
                               scenario == ReadRetirementCase::CancelBeforeDecoderObserver;
        const auto quarantine = scenario == ReadRetirementCase::RegistrationFailure || scenario == ReadRetirementCase::PreCancelledRead ||
                                scenario == ReadRetirementCase::InvalidDecodeTail;
        if (scenario == ReadRetirementCase::InvalidDecodeTail)
        {
            Require(Until([&] { return gate->Registered(); }), L"invalid outer wait retains a genuinely pending read");
            {
                std::lock_guard lock(task->mutex);
                task->decodeTail = winrt::handle{}; // No wait is registered on this task event in this direct native fixture.
            }
            Require(!task->WaitForDecodeComplete(), L"invalid decode-tail wait is a false observation, never completion");
            task->Finish();
            Require(task->svgQuarantined && !task->finished.load() && gate->active.load() == 1 && gate->closes.load() == 0 &&
                        policy->Snapshot().header.active_requests == 1,
                    L"failed tail observation preserves the original task and actual physical permit");
        }
        if (scenario == ReadRetirementCase::LateCancel)
            SetEvent(gate->returnRead.get()); // The stream's late-Cancel target is now published.
        if (scenario == ReadRetirementCase::Pending)
            Require(Until([&] { return gate->Registered(); }) && gate->active.load() == 1 && gate->closes.load() == 0,
                    L"ordinary pending read has an actual registered completion observer");
        if (scenario == ReadRetirementCase::CancelBeforeDecoderObserver)
        {
            task->Cancel();
            action.Completed([gate](auto const &, auto) { SetEvent(gate->decoderDone.get()); });
            ReadGate::Wait(gate->decoderDone.get()); // Deliberately premature C++/WinRT outer notification.
            task->Finish();
            Require(!task->finished.load() && gate->active.load() == 1 && gate->closes.load() == 0 &&
                        policy->Snapshot().header.active_requests == 1,
                    L"an early outer cancellation observer cannot retire the actual decoder task/IO slot");
            SetEvent(gate->returnRead.get());
        }
        else if (scenario == ReadRetirementCase::CancelDuringCreate)
        {
            task->Cancel();
            SetEvent(gate->returnRead.get());
        }
        else if (scenario == ReadRetirementCase::CancelDuringRegistration)
        {
            ReadGate::Wait(gate->registering.get());
            task->Cancel();
            SetEvent(gate->returnRegistration.get());
        }
        else if (cancelled)
        {
            Require(Until([&] { return gate->Registered(); }), L"cancel only after the actual child completion observer exists");
            task->Cancel();
        }
        if (cancelled)
        {
            Require(Until([&]
                          {
                              return gate->cancelCalls.load() != 0 ||
                                     (scenario != ReadRetirementCase::CancelBeforeDecoderObserver &&
                                      WaitForSingleObjectEx(gate->decoderDone.get(), 0, FALSE) == WAIT_OBJECT_0);
                          }), L"child cancellation or decoder notification arrives");
            Require(gate->cancelCalls.load() == 1,
                    L"child read must receive cancellation before decoder retirement; closes=" +
                        std::to_wstring(gate->closes.load()) + L", closedWhileActive=" +
                        std::to_wstring(gate->closeActive.load()) + L", activeReads=" + std::to_wstring(gate->active.load()));
            ReadGate::Wait(gate->cancelling.get());
            Require(gate->cancelCalls.load() == 1 && gate->active.load() == 1 && gate->closes.load() == 0 &&
                        (scenario == ReadRetirementCase::CancelBeforeDecoderObserver ||
                         WaitForSingleObjectEx(gate->decoderDone.get(), 0, FALSE) == WAIT_TIMEOUT) &&
                        policy->Snapshot().header.active_requests == 1 && !task->finished.load(),
                    L"cancellation retains the live read, original physical slot and Close authority");
        }
        if (scenario == ReadRetirementCase::CancelTail)
        {
            SetEvent(gate->finishRead.get());
            ReadGate::Wait(gate->physicalDone.get());
            Require(gate->cancelActive.load() == 1 && gate->closes.load() == 0 &&
                        WaitForSingleObjectEx(gate->decoderDone.get(), 0, FALSE) == WAIT_TIMEOUT,
                    L"actual read completion cannot outrun the in-progress child Cancel tail");
            SetEvent(gate->returnCancel.get());
        }
        if (quarantine && scenario != ReadRetirementCase::InvalidDecodeTail)
        {
            ReadGate::Wait(gate->decoderDone.get());
            task->Finish();
            Require(task->svgQuarantined && !task->finished.load() && gate->active.load() == 1 &&
                        gate->closes.load() == 0 && policy->Snapshot().header.active_requests == 1 &&
                        FAILED(result->closeError) && !result->Ready(),
                    L"unobservable read quarantines the original permit and unique Close authority");
        }
        SetEvent(gate->finishRead.get());
        ReadGate::Wait(gate->decoderDone.get());
        Require(task->WaitForDecodeComplete() == !quarantine,
                L"actual decoder tail, not its eager cancelled IAsyncInfo, owns completion observation");
        Require(Until([&] { return gate->active.load() == 0; }), L"real read coroutine tail has retired");
        try
        {
            action.GetResults();
        }
        catch (winrt::hresult_canceled const &)
        {
            Require(cancelled || scenario == ReadRetirementCase::LateCancel);
        }
        if (!quarantine)
        {
            Require(gate->closes.load() == 1 && gate->closeActive.load() == 0, L"exactly-once Close follows read and cancel tails");
            if (cancelled || scenario == ReadRetirementCase::LateCancel)
                Require(!result->Ready() && result->failure == NOTEPADS_MARKDOWN_IMAGE_CANCELLED, L"cancel never publishes Ready");
            else if (scenario == ReadRetirementCase::CloseFailure)
                Require(!result->Ready() && result->error == E_ACCESSDENIED && result->closeError == E_ACCESSDENIED,
                        L"Close failure remains visible and prevents Ready");
            else if (scenario == ReadRetirementCase::ReadAndCloseFailure)
                Require(!result->Ready() && result->error == E_INVALIDARG && result->closeError == E_ACCESSDENIED,
                        L"first read failure is preserved alongside independent Close failure");
            else
                Require(result->Ready() && Matches(*result->pixels), L"synchronous and pending reads retain the independent BMP oracle");
            if (scenario == ReadRetirementCase::PartialRead)
                Require(gate->calls.load() >= 7, L"partial input reuses the same completed event/handler through six nonempty reads and EOF");
            result->pixels.reset();
            task->Finish();
            // The decoder-tail signal can precede a pending Finish's final release,
            // and the child completion callback can still hold its original request.
            // Observe both real retirements; outer IAsyncInfo completion is insufficient.
            const auto retired = Until([&]
            {
                return task->finished.load(std::memory_order_acquire) &&
                       policy->Snapshot().header.active_requests == 0;
            });
            Require(retired && !task->request,
                    L"real read completion reopens the original physical slot; scenario=" +
                        std::to_wstring(static_cast<uint32_t>(scenario)));
            auto successor = StartImageTestRequest(policy, 1);
            Require(policy->Snapshot().header.active_requests == 1, L"successor admission after actual retirement");
            successor.reset();
        }
        auto snapshot = policy->Snapshot();
        ReadRetirementResult observed{true, quarantine, gate->closes.load(), gate->closeActive.load(),
                                     gate->cancelCalls.load(), gate->active.load(), snapshot.header.active_requests, gate->calls.load()};
        return observed;
    }

    void RunRawEncodedFailureRegression(std::shared_ptr<ReadingMemory> const &memory)
    {
        RawEncodedFailureKeepsProcessCapacity(memory);
    }

    winrt::Windows::Foundation::IAsyncAction RunMetadataReAdmissionRegression(std::shared_ptr<ReadingMemory> memory)
    {
        return MetadataSurvivesRowReAdmission(std::move(memory));
    }

    winrt::Windows::Foundation::IAsyncAction RunAsync(winrt::Windows::UI::Core::CoreDispatcher dispatcher,
                                                      std::shared_ptr<CoreControllerTests::Results> results,
                                                      std::shared_ptr<CoreValidation::Context> context)
    {
        if (!dispatcher || !dispatcher.HasThreadAccess() || !results || !context)
            throw winrt::hresult_invalid_argument();
        results->checks.reserve(results->checks.size() + CheckCount());
        auto controller = CoreController::CreatePreview(dispatcher);
        auto lifetime = controller->Lifetime();
        CoreCompletionTicket ticket;
        CoreModelPin pin;
        notepads_markdown_core_cancel *sourceFlag{};
        bool registered{}, sourceRegistered{};
        std::shared_ptr<ReadingMemory> memory;
        std::shared_ptr<NativeImagePolicy> policy;
        std::shared_ptr<NativeImageDecodeResult> preserved;
        std::exception_ptr setup;
        try
        {
            context->PublishController(controller.get());
            registered = true;
            policy = NativeImagePolicy::Create();
            preserved = std::make_shared<NativeImageDecodeResult>();
            auto descriptor = Pod<notepads_markdown_core_snapshot>();
            descriptor.source.document[0] = 0x49;
            descriptor.source.document[15] = 0x30;
            descriptor.source.revision = 1;
            descriptor.profile = NOTEPADS_MARKDOWN_CORE_G3;
            constexpr std::string_view source = "![tile](tile.bmp)";
            Check(
                controller
                    ->SubmitUtf8({reinterpret_cast<uint8_t const *>(source.data()), source.size()}, descriptor, &ticket)
                    .status);
            co_await winrt::resume_background();
            Require(Until(
                [&]
                {
                    context->Arbitrate();
                    return ticket.Done();
                }));
            Check(ticket.Status());
            co_await winrt::resume_foreground(dispatcher);
            pin = controller->PinInstalled();
            co_await winrt::resume_background();
            Require(static_cast<bool>(pin));
            auto summary = Pod<notepads_markdown_core_model_summary_result>();
            auto error = Pod<notepads_markdown_core_error>();
            Check(notepads_markdown_core_model_summary(pin.Get(), &summary, &error));
            Check(notepads_markdown_core_model_cancel_create(pin.Get(), &sourceFlag, &error));
            context->Publish(sourceFlag);
            sourceRegistered = true;
            memory = ReadingMemory::Create(std::move(pin), sourceFlag, summary.identity);
        }
        catch (...)
        {
            setup = std::current_exception();
        }
        if (!setup)
        {
            constexpr std::array names{L"image_decode_bmp_independent_pixels",
                                       L"image_decode_bad_and_unsupported",
                                       L"image_decode_dimension_guard",
                                       L"image_decode_cancel_and_stale",
                                       L"image_decode_exif_transpose_antitranspose",
                                       L"image_decode_gif_canvas_contract",
                                       L"image_decode_xml_dispatch",
                                       L"image_font_watch_stop_reload",
                                       L"image_font_watch_idle_destroy",
                                       L"image_font_watch_notification_owners",
                                       L"image_raw_encoded_failed_free_process_budget",
                                       L"image_metadata_forget_readmit_pending_failed_wic",
                                       L"image_svg_native_worker_and_finish",
                                       L"image_svg_retained_owners_and_release"};
            for (size_t index = 0; index < names.size(); ++index)
            {
                std::exception_ptr failure;
                try
                {
                    context->Arbitrate();
                    switch (index)
                    {
                    case 0:
                        co_await Normal(policy, memory, preserved);
                        break;
                    case 1:
                        co_await FailedInputs(policy, memory);
                        break;
                    case 2:
                        co_await HugeDimensions(policy, memory);
                        break;
                    case 3:
                        co_await CancelledAndStale(policy, memory);
                        break;
                    case 4:
                        co_await ExifMirrors(policy, memory);
                        break;
                    case 5:
                        co_await GifCanvas(policy, memory);
                        break;
                    case 6:
                        co_await XmlDispatch(memory);
                        break;
                    case 7:
                    case 8:
                    case 9:
                    {
                        co_await winrt::resume_background();
                        auto observation = RunImageFontWatchDiagnostic(memory, static_cast<uint32_t>(index - 7));
                        Require(observation.passed,
                                L"actual TP watcher: HRESULT=" + std::to_wstring(observation.error) + L", waits=" +
                                    std::to_wstring(observation.waitsCreated) + L"/" +
                                    std::to_wstring(observation.waitsClosed) + L", workClose=" +
                                    std::to_wstring(observation.workClosed) + L", notification/Ack=" +
                                    std::to_wstring(observation.notifications) + L"/" +
                                    std::to_wstring(observation.acknowledgements) + L", physicalFree=" +
                                    std::to_wstring(observation.frees));
                        break;
                    }
                    case 10:
                        co_await winrt::resume_background();
                        RawEncodedFailureKeepsProcessCapacity(memory);
                        break;
                    case 11:
                        co_await MetadataSurvivesRowReAdmission(memory);
                        break;
                    case 12:
                        co_await winrt::resume_background();
                        Require(SvgJobTests::Run(memory).checks == 6);
                        break;
                    case 13:
                        co_await winrt::resume_background();
                        Require(SvgRetainedTests::Run(memory->ModelOwner().Get(), sourceFlag).checks == 14);
                        break;
                    }
                }
                catch (...)
                {
                    failure = std::current_exception();
                }
                auto detail = L"Real model-root funded image decode using fixed source fixtures.";
                if (index >= 7 && index <= 9)
                    detail = L"Paid font observer with a private event and real TP_WORK/TP_WAIT ownership and retirement.";
                else if (index == 12)
                    detail = L"Real SVG raster worker, resource and thread retirement, pre-cancellation, and outer finish gate.";
                else if (index == 13)
                    detail = L"Retained SVG owners share the model ledger and recover from controlled release failures.";
                Record(results, context, Result(names[index], failure, detail), names[index]);
            }
        }
        else
            Record(results, context,
                   Result(L"image_decode_fixture_setup", setup, L"Real G3 model and ReadingMemory fixture."),
                   L"image_decode_fixture_setup");

        if (sourceRegistered)
            context->Withdraw(sourceFlag);
        if (registered)
            context->WithdrawController(controller.get());
        controller->Stop();
        co_await winrt::resume_background();
        auto cancelCleanup = notepads_markdown_core_cancel_destroy(sourceFlag);
        context->RecordCleanup(cancelCleanup, L"image_decode_fixture_cleanup");
        pin.Reset();
        std::weak_ptr<ReadingMemory> oldImageMemory = memory;
        memory.reset();
        auto drain = controller->Drain();
        controller.reset();
        ticket = {};
        std::exception_ptr ownership;
        try
        {
            Require(!setup && preserved && preserved->Ready() && Matches(*preserved->pixels));
            Require(lifetime.Snapshot().modelPins == 0 && oldImageMemory.expired());
            auto stillOwned = policy->Snapshot();
            Require(stillOwned.header.decoded_bytes == Expected.size());
            preserved->pixels.reset();
            auto released = policy->Snapshot();
            Require(released.header.active_requests == 0 && released.header.reservations == 0 &&
                    released.header.encoded_bytes == 0 && released.header.decoded_bytes == 0);
        }
        catch (...)
        {
            ownership = std::current_exception();
        }
        preserved.reset();
        policy.reset();
        auto released = Until([&] { return lifetime.Snapshot().fullyReleased; });
        auto final = lifetime.Snapshot();
        results->lastDrain = drain;
        results->lastLifetime = final;
        auto check = Result(L"image_decode_last_native_owner", ownership,
                            L"CPU pixels survive source/controller/caller flags after the genuine model owner retires, "
                            L"then native buffer free precedes image subquota return and final allocation-root release. "
                            L"No codec cancellation latency, driver, GPU, or process RSS claim.");
        check.passed = check.passed && released && drain.coreDrained && drain.driverExited &&
                       drain.primaryStatus == NOTEPADS_MARKDOWN_CORE_OK && drain.cleanupStatus == NOTEPADS_MARKDOWN_CORE_OK &&
                       drain.primaryHresult == 0 && drain.cleanupHresult == 0 && final.cleanupStatus == NOTEPADS_MARKDOWN_CORE_OK &&
                       final.cleanupHresult == 0;
        if (!check.passed && check.status == NOTEPADS_MARKDOWN_CORE_OK)
            check.status = NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
        Record(results, context, std::move(check), L"image_decode_last_native_owner");
        context->RecordCleanup(drain.cleanupStatus, L"image_decode_fixture_cleanup");
        context->RecordCleanup(drain.cleanupHresult, L"image_decode_fixture_cleanup");
        context->RecordCleanup(final.cleanupStatus, L"image_decode_fixture_cleanup");
        context->RecordCleanup(final.cleanupHresult, L"image_decode_fixture_cleanup");
        if (!released)
            context->RecordCleanupInvariant(L"image_decode_fixture_cleanup");
        context->Arbitrate();
        co_await winrt::resume_foreground(dispatcher);
        co_await ImageFontControlTests::RunAsync(dispatcher, results, context);
    }
}
