// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include <cmath>
#include <cstdint>
#include <limits>
#include <initializer_list>
#include <optional>

namespace Notepads::Markdown::Native
{
    // Platform-private values. All positions are DIPs; the backing dimensions
    // are physical pixels. The control owns exactly one presented value.
    struct ViewportGeometry
    {
        double x{}, y{}, width{}, height{}, scale{1};
        double overlayX{}, overlayY{}, surfaceX{}, surfaceY{};
        double surfaceWidth{}, surfaceHeight{}, phaseX{}, phaseY{};
        int32_t pixelWidth{}, pixelHeight{};
        bool pixelExact{};
        uint64_t epoch{};

        bool Same(ViewportGeometry const &other) const noexcept
        {
            return x == other.x && y == other.y && width == other.width && height == other.height &&
                   scale == other.scale && overlayX == other.overlayX && overlayY == other.overlayY &&
                   surfaceX == other.surfaceX && surfaceY == other.surfaceY && surfaceWidth == other.surfaceWidth &&
                   surfaceHeight == other.surfaceHeight && phaseX == other.phaseX && phaseY == other.phaseY &&
                   pixelWidth == other.pixelWidth && pixelHeight == other.pixelHeight && pixelExact == other.pixelExact;
        }
    };

    inline bool MakeViewportGeometry(double x, double y, double width, double height, double scale, double rootX,
                                     double rootY, double overlayX, double overlayY, bool unitAxes,
                                     ViewportGeometry &result) noexcept
    {
        for (double value : {x, y, width, height, scale, rootX, rootY, overlayX, overlayY})
            if (!std::isfinite(value))
                return false;
        if (width <= 0 || height <= 0 || scale <= 0)
            return false;
        auto left = std::floor(rootX * scale);
        auto top = std::floor(rootY * scale);
        auto right = std::ceil((rootX + width) * scale);
        auto bottom = std::ceil((rootY + height) * scale);
        auto pixelsX = unitAxes ? right - left : std::ceil(width * scale);
        auto pixelsY = unitAxes ? bottom - top : std::ceil(height * scale);
        if (!std::isfinite(pixelsX) || !std::isfinite(pixelsY) || pixelsX < 1 || pixelsY < 1 || pixelsX > 16384 ||
            pixelsY > 16384)
            return false;
        ViewportGeometry next{};
        next.x = x;
        next.y = y;
        next.width = width;
        next.height = height;
        next.scale = scale;
        next.overlayX = overlayX;
        next.overlayY = overlayY;
        next.pixelWidth = static_cast<int32_t>(pixelsX);
        next.pixelHeight = static_cast<int32_t>(pixelsY);
        next.pixelExact = unitAxes;
        next.phaseX = unitAxes ? rootX - left / scale : 0;
        next.phaseY = unitAxes ? rootY - top / scale : 0;
        next.surfaceX = overlayX - next.phaseX;
        next.surfaceY = overlayY - next.phaseY;
        next.surfaceWidth = unitAxes ? pixelsX / scale : width;
        next.surfaceHeight = unitAxes ? pixelsY / scale : height;
        result = next;
        return true;
    }

    inline bool QuantizeViewportY(double rawY, double scale, int64_t &pixelRow, double &presentY) noexcept
    {
        if (!std::isfinite(rawY) || !std::isfinite(scale) || rawY < 0 || scale <= 0)
            return false;
        auto row = std::floor(rawY * scale + 0.5);
        // Avoid a rounded double representation of INT64_MAX admitting 2^63.
        if (!std::isfinite(row) || row < 0 || row >= 0x1p63)
            return false;
        pixelRow = static_cast<int64_t>(row);
        presentY = row / scale;
        return std::isfinite(presentY);
    }

    struct ViewportTarget
    {
        ViewportGeometry geometry;
        int64_t pixelRow{};
        double y{};
        uint64_t serial{};

        bool SameTarget(ViewportTarget const &other) const noexcept
        {
            return geometry.epoch == other.geometry.epoch && pixelRow == other.pixelRow;
        }
    };

    // The only pending viewport is overwritten; an already dispatched value
    // stays immutable. A presentation-only redraw may advance the serial floor.
    inline bool CoalesceViewportTarget(std::optional<ViewportTarget> &latest, ViewportGeometry const &geometry,
                                       double rawY, uint64_t &serial, uint64_t presentedFloor = 0) noexcept
    {
        ViewportTarget next;
        next.geometry = geometry;
        if (!QuantizeViewportY(rawY, geometry.scale, next.pixelRow, next.y))
            return false;
        if (latest && latest->SameTarget(next) && latest->serial > presentedFloor)
            return true;
        if (serial == UINT64_MAX)
            return false;
        next.serial = ++serial;
        latest = next;
        return true;
    }
}
