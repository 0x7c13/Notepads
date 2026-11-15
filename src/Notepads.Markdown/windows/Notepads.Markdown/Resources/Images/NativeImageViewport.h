// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include <algorithm>
#include <cmath>
#include <optional>

namespace Notepads::Markdown::Native
{
    // Navigation lines use document coordinates; a frame's Canvas viewport also
    // contains its top padding. Convert once before visible/near IO admission.
    struct ImageViewportWindow
    {
        double top{}, bottom{}, height{};
        static std::optional<ImageViewportWindow> FromCanvas(double y, double height, double padding) noexcept
        {
            auto top = y - padding;
            auto bottom = top + height;
            if (!std::isfinite(y) || !std::isfinite(height) || height <= 0 ||
                !std::isfinite(top) || !std::isfinite(bottom) ||
                !std::isfinite(top - height) || !std::isfinite(bottom + height))
                return {};
            return ImageViewportWindow{top, bottom, height};
        }
        bool Near(double first, double last) const noexcept
        {
            return last >= top - height && first <= bottom + height;
        }
        bool Visible(double first, double last) const noexcept
        {
            return last >= top && first <= bottom;
        }
        double Distance(double first, double last) const noexcept
        {
            return std::max({0.0, top - last, first - bottom});
        }
    };
}
