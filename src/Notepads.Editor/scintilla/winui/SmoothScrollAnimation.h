// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once

#include "VerticalScroll.h"

namespace Scintilla::Internal {

// Retain distance from the reading-order leading edge. A growing RTL canvas
// changes its physical origin without changing the user's scrolling intent.
// Rendering uses whole pixels; subpixel input stays in the animation channel.
class HorizontalScrollGeometry {
	int maximumPixels;
	double scale;
	bool fromRight;
	double origin;
public:
	using Position = double;
	HorizontalScrollGeometry(int maximumPixels_, double scale_, bool fromRight_ = false, double origin_ = 0) noexcept :
		maximumPixels(std::max(0, maximumPixels_)), scale(scale_), fromRight(fromRight_), origin(origin_) {}
	double UnitSize() const noexcept { return 1 / scale; }
	double FromPixels(int position) const noexcept { return (fromRight ? origin - position : position) / scale; }
	int PixelOffset(double position) const noexcept {
		return static_cast<int>(std::clamp(std::round(fromRight ? origin - position * scale : position * scale),
			0.0, static_cast<double>(maximumPixels)));
	}
	double Clamp(double position) const noexcept {
		return fromRight ? std::clamp(position, (origin - maximumPixels) / scale, origin / scale) :
			std::clamp(position, 0.0, maximumPixels / scale);
	}
	double Add(double position, double delta) const noexcept { return Clamp(position + delta); }
	double Distance(double from, double to) const noexcept { return to - from; }
	bool Matches(double left, double right) const noexcept {
		return PixelOffset(left) == PixelOffset(right);
	}
};

// A short target-following filter, not an additional source of kinetic inertia.
// The caller supplies a monotonic clock and owns frame subscription/cancellation.
// There is no document-sized floating point coordinate or allocation per frame.
template <typename Geometry>
class SmoothScrollAnimation {
	using Position = typename Geometry::Position;
	Position target{};
	Position expected{};
	double previousTime = 0;
	double unitSize = 0;
	bool anchored = false;
	bool active = false;
public:
	bool Active() const noexcept { return active; }
	Position Target() const noexcept { return target; }
	void Cancel() noexcept { active = anchored = false; }
	void Retarget(const Geometry &geometry, Position current,
		double delta, double now, bool accumulate = true) noexcept {
		const bool matching = anchored && geometry.Matches(current, expected) && unitSize == geometry.UnitSize();
		// Preserve horizontal fractions across completed animations, including
		// reduced-motion input, until another operation takes the viewport.
		if (matching) current = geometry.Clamp(expected);
		const bool continuing = active && matching && accumulate;
		const double remaining = continuing ? geometry.Distance(current, target) : 0;
		// A reversal starts from what is actually visible, not the old target.
		if (!continuing || (remaining != 0 && delta != 0 && std::signbit(remaining) != std::signbit(delta)))
			target = current;
		target = geometry.Add(target, delta);
		if (!continuing) previousTime = now;
		expected = current;
		unitSize = geometry.UnitSize();
		anchored = true;
		active = target != current;
	}
	Position Finish(const Geometry &geometry, Position current) noexcept {
		if (!active) return current;
		if (!geometry.Matches(expected, current) || unitSize != geometry.UnitSize()) {
			Cancel();
			return current;
		}
		active = false;
		expected = target = geometry.Clamp(target);
		return expected;
	}
	Position Advance(const Geometry &geometry,
		Position current, double now, double pixelScale) noexcept {
		// An external viewport/metric change owns the view from this point on.
		if (!active) return current;
		if (!geometry.Matches(expected, current) || unitSize != geometry.UnitSize()) {
			Cancel();
			return current;
		}
		current = geometry.Clamp(expected);
		if (now <= previousTime) return current;
		target = geometry.Clamp(target);
		const double distance = geometry.Distance(current, target);
		const double elapsed = now - previousTime;
		previousTime = now;
		if (elapsed >= 0.25 || std::abs(distance) * pixelScale <= 0.25) {
			active = false;
			expected = target;
		} else {
			constexpr double responseSeconds = 0.04;
			expected = geometry.Add(current, distance * -std::expm1(-elapsed / responseSeconds));
		}
		return expected;
	}
};

}
