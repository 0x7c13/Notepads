// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <limits>
#include <optional>

namespace Notepads::Markdown::Native
{
    // Input intent only. ScrollViewer owns interpolation and the actual offset;
    // ViewportPresentation owns the sole physical-pixel rasterization boundary.
    class NativeWheelScroll
    {
      public:
        uint64_t Serial() const noexcept { return _serial; }
        bool Active() const noexcept { return _target.has_value(); }
        std::optional<double> Target() const noexcept { return _target; }

        uint64_t Cancel() noexcept
        {
            _target.reset();
            _direction = 0;
            return ++_serial;
        }

        std::optional<double> Plan(int32_t wheelDelta, double current, double maximum, double viewportHeight,
                                   double lineHeight, uint32_t lines) const noexcept
        {
            if (!wheelDelta || !lines || !std::isfinite(current) || current < 0 || !std::isfinite(maximum) ||
                maximum <= 0 || !std::isfinite(viewportHeight) || viewportHeight <= 0 ||
                !std::isfinite(lineHeight) || lineHeight <= 0)
                return {};
            current = std::clamp(current, 0.0, maximum);
            const int direction = wheelDelta < 0 ? 1 : -1;
            const double distance = lines == std::numeric_limits<uint32_t>::max()
                                        ? viewportHeight
                                        : static_cast<double>(lines) * lineHeight;
            const double delta = -static_cast<double>(wheelDelta) / 120.0 * distance;
            if (!std::isfinite(delta))
                return {};
            const double origin = _target && _direction == direction ? std::clamp(*_target, 0.0, maximum) : current;
            return std::clamp(origin + delta, 0.0, maximum);
        }

        uint64_t Publish(double target, int32_t wheelDelta) noexcept
        {
            _target = target;
            _direction = wheelDelta < 0 ? 1 : -1;
            return ++_serial;
        }

        void Reject(uint64_t serial) noexcept
        {
            if (serial == _serial)
                _target.reset();
        }

        void Observe(double current, double scale) noexcept
        {
            // A late final event for an older animation must not retire the
            // newer target. A quarter physical pixel only covers offset precision.
            if (_target && std::isfinite(current) && std::isfinite(scale) && scale > 0 &&
                std::abs(current - *_target) * scale <= 0.25)
                _target.reset();
        }

      private:
        std::optional<double> _target;
        uint64_t _serial{};
        int _direction{};
    };
}
