// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Control/ViewportPresentation.h"
#include <algorithm>
#include <cstdint>
#include <optional>

namespace Notepads::Markdown::Native
{
    struct ViewportRasterBand
    {
        static constexpr uint64_t MaximumBytes = 16 * 1024 * 1024;
        int64_t firstRow{};
        int32_t pixelWidth{}, pixelHeight{};
        uint64_t bytes{};

        bool Contains(int64_t row, int32_t width, int32_t height) const noexcept
        {
            return row >= firstRow && width == pixelWidth && height > 0 && height <= pixelHeight &&
                   row - firstRow <= static_cast<int64_t>(pixelHeight - height);
        }
    };

    // Draw normalizes ordinary text to a binary DIP grid. A whole physical
    // row is not necessarily a whole grid step (notably at 144 DPI).
    inline bool ViewportRasterTranslationCompatible(int64_t rows, double scale) noexcept
    {
        const auto dpi = 96.0f * static_cast<float>(scale);
        if (!std::isfinite(scale) || scale <= 0 || !std::isfinite(dpi) || dpi <= 0 ||
            static_cast<double>(dpi) / 96.0 != scale)
            return false;
        const auto units = std::exp2(std::ceil(std::log2(static_cast<double>(dpi) * (64.0 / 96.0))));
        const auto steps = (static_cast<double>(rows) / scale) * units;
        return std::isfinite(steps) && steps == std::floor(steps);
    }

    inline bool ViewportRasterPaintIsStatic(bool selected, bool caretPainted,
        bool actionPainted, bool linkHovered) noexcept
    {
        return !selected && !caretPainted && !actionPainted && !linkHovered;
    }

    inline bool ViewportRasterOverlapsRange(ViewportRasterBand const &band, ViewportGeometry const &geometry,
        double padding, double top, double height) noexcept
    {
        if (!std::isfinite(top) || !std::isfinite(height) || height < 0 ||
            !std::isfinite(geometry.scale) || geometry.scale <= 0)
            return true;
        // Include the raster fringe of antialiased decorations and caret ink.
        const auto first = band.firstRow / geometry.scale - padding - geometry.phaseY - 2 / geometry.scale;
        const auto last = first + (band.pixelHeight + 4.0) / geometry.scale;
        return top < last && top + height > first;
    }

    inline bool SameViewportRaster(ViewportGeometry const &left, ViewportGeometry const &right) noexcept
    {
        return left.pixelExact && right.pixelExact && left.width == right.width && left.height == right.height &&
               left.scale == right.scale && left.phaseX == right.phaseX && left.phaseY == right.phaseY &&
               left.pixelWidth == right.pixelWidth && left.pixelHeight == right.pixelHeight;
    }

    // Physical bounds include every edge pixel of the visible SIS, not merely
    // the logical viewport height. The caller still validates Frame::Reposition.
    inline std::optional<ViewportRasterBand> PlanViewportRasterBand(ViewportGeometry const &geometry,
        int64_t row, double padding, double measuredTop, double measuredBottom, double totalHeight,
        bool expand) noexcept
    {
        if (!geometry.pixelExact || row < 0 || geometry.pixelWidth < 1 || geometry.pixelHeight < 1 ||
            geometry.pixelWidth > 16384 || geometry.pixelHeight > 16384 ||
            row > INT64_MAX - geometry.pixelHeight ||
            !std::isfinite(geometry.phaseX) || !std::isfinite(geometry.phaseY) ||
            !std::isfinite(geometry.scale) || geometry.scale <= 0 || !std::isfinite(padding) || padding < 0 ||
            !std::isfinite(measuredTop) || !std::isfinite(measuredBottom) || !std::isfinite(totalHeight) ||
            measuredTop < 0 || measuredBottom < measuredTop || totalHeight < 0)
            return std::nullopt;
        const auto rowBytes = static_cast<uint64_t>(geometry.pixelWidth) * 4;
        const auto heightLimit = std::min<uint64_t>(16384, ViewportRasterBand::MaximumBytes / rowBytes);
        if (static_cast<uint64_t>(geometry.pixelHeight) > heightLimit)
            return std::nullopt;
        ViewportRasterBand result{row, geometry.pixelWidth, geometry.pixelHeight,
                                 rowBytes * static_cast<uint64_t>(geometry.pixelHeight)};
        if (!expand)
            return result;
        const auto extra = static_cast<int64_t>(geometry.pixelHeight / 2);
        if (row > INT64_MAX - geometry.pixelHeight - extra)
            return result;
        // One inward physical row also keeps double -> DIP conversion away
        // from a measured boundary when the scale is not an integer.
        const auto first = measuredTop == 0 ? 0.0 :
            std::ceil((measuredTop + padding + geometry.phaseY) * geometry.scale) + 1;
        const auto last = measuredBottom >= totalHeight
            ? static_cast<double>(row + geometry.pixelHeight + extra)
            : std::floor((measuredBottom + padding) * geometry.scale) - 1;
        if (!std::isfinite(first) || !std::isfinite(last) || first < 0 || first >= 0x1p63 ||
            last < 0 || last >= 0x1p63)
            return result;
        auto begin = std::max<int64_t>(static_cast<int64_t>(first), std::max<int64_t>(0, row - extra));
        auto end = std::min<int64_t>(static_cast<int64_t>(last), row + geometry.pixelHeight + extra);
        // Never enlarge beyond two physical viewports or the byte/dimension cap.
        const auto maximum = std::min<uint64_t>(heightLimit, static_cast<uint64_t>(geometry.pixelHeight) * 2);
        if (end > begin && static_cast<uint64_t>(end - begin) > maximum)
        {
            begin = std::max<int64_t>(begin, row - static_cast<int64_t>((maximum - geometry.pixelHeight) / 2));
            end = std::min<int64_t>(end, begin + std::min<int64_t>(
                static_cast<int64_t>(maximum), INT64_MAX - begin));
        }
        // Preserve the current viewport's actual text-origin phase when the
        // band's origin changes. An unusual scale without a nearby compatible
        // row simply keeps the original single-viewport paint.
        unsigned alignmentRows{};
        while (begin <= row && !ViewportRasterTranslationCompatible(row - begin, geometry.scale))
        {
            if (++alignmentRows > 32)
                return result;
            ++begin;
        }
        if (begin <= row && end >= row + geometry.pixelHeight && end - begin > geometry.pixelHeight)
        {
            result.firstRow = begin;
            result.pixelHeight = static_cast<int32_t>(end - begin);
            result.bytes = rowBytes * static_cast<uint64_t>(result.pixelHeight);
        }
        return result;
    }
}
