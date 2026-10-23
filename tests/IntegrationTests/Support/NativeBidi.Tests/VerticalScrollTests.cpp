// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include <cmath>
#include <limits>
#include <stdexcept>
#include "VerticalScroll.h"
#include "SmoothScrollAnimation.h"

size_t CheckVerticalScrollGeometry() {
	using namespace Scintilla::Internal;
	size_t checks = 0;
	const auto require = [&](bool condition) {
		if (!condition) throw std::runtime_error("Vertical viewport geometry contract failed.");
		++checks;
	};
	for (const double scale : {1.0, 1.25, 1.5, 2.0, 3.0}) {
		for (const int height : {17, 24, 37}) {
			for (const double viewport : {0.0, 5.0, 24.0, 301.0, 320.0}) {
				for (const Sci::Line rows : {0, 1, 12, 2000}) {
					VerticalScrollGeometry geometry(height, scale, viewport, rows, true);
					const auto maximum = geometry.Maximum();
					// Independent small-document oracle, in device pixels.
					const double end = std::max(0.0, static_cast<double>(rows) * height - viewport);
					require(std::abs((maximum.row * height + maximum.offset * scale) - end) < 1e-8);
					for (const double delta : {-100000.0, -0.1, 0.0, 0.1, 7.3, 17.0, 100000.0}) {
						const auto position = geometry.Clamp({0, delta});
						const double expected = std::clamp(delta * scale, 0.0, end);
						require(std::abs(position.row * height + position.offset * scale - expected) < 1e-8);
						require(geometry.Clamp(position) == position);
						require(position.offset >= 0 && position.offset < geometry.RowHeight());
						require(geometry.PixelOffset(position) == static_cast<int>(std::round(position.offset * scale)));
					}
				}
			}
		}
	}
	const Sci::Line large = std::numeric_limits<Sci::Line>::max() - 100;
	VerticalScrollGeometry huge(24, 1.5, 300, large, true);
	const auto start = VerticalScrollPosition{large - 1000, 0.125};
	const auto next = huge.Clamp({start.row, start.offset + 0.25});
	require(next.row == start.row && next.offset == 0.375);
	require(huge.Distance(start, next) == 0.25);
	require(huge.Clamp({0, std::numeric_limits<double>::max()}) == huge.Maximum());
	require(huge.Clamp({0, -std::numeric_limits<double>::max()}) == VerticalScrollPosition{});
	VerticalScrollGeometry beyondEnd(24, 1.25, 300, 100, false);
	require(beyondEnd.Maximum() == VerticalScrollPosition{99, 0});
	for (double scale : {1.25, 1.5, 1.75, 3.0}) {
		VerticalScrollGeometry exact(17, scale, 17 * 15, 2000, true);
		require(exact.Maximum() == VerticalScrollPosition{1985, 0});
	}

	VerticalScrollGeometry geometry(24, 1.5, 300, 2000, true);
	for (const double frequency : {30.0, 60.0, 144.0}) {
		SmoothScrollAnimation<VerticalScrollGeometry> animation;
		VerticalScrollPosition current{100, 0};
		animation.Retarget(geometry, current, 160, 0);
		for (int frame = 1; frame <= static_cast<int>(frequency * 0.12); ++frame) {
			const auto framePosition = animation.Advance(geometry, current, frame / frequency, 1.5);
			require(geometry.Distance(current, framePosition) > 0 && geometry.Distance(framePosition, animation.Target()) >= 0);
			current = framePosition;
		}
		// An independent analytic response at an exact clock time, irrespective
		// of how many intermediate rendering callbacks were delivered.
		current = animation.Advance(geometry, current, 0.15, 1.5);
		require(std::abs(geometry.Distance({100, 0}, current) - 160 * (1 - std::exp(-0.15 / 0.04))) < 1e-8);
		animation.Retarget(geometry, current, -3.25, 0.15);
		require(std::abs(geometry.Distance(current, animation.Target()) + 3.25) < 1e-8);
		const auto reversed = animation.Advance(geometry, current, 0.16, 1.5);
		require(geometry.Distance(current, reversed) < 0);
		current = animation.Advance(geometry, reversed, 0.6, 1.5);
		require(!animation.Active() && current == animation.Target());
	}
	SmoothScrollAnimation<VerticalScrollGeometry> animation;
	animation.Retarget(geometry, {100, 0}, 0.125, 0);
	const auto tiny = animation.Advance(geometry, {100, 0}, 0.02, 1.5);
	require(tiny.row == 100 && tiny.offset == 0.125 && !animation.Active());
	animation.Retarget(geometry, {100, 0}, 200, 0);
	const auto external = VerticalScrollPosition{400, 0.25};
	require(animation.Advance(geometry, external, 0.1, 1.5) == external && !animation.Active());
	animation.Retarget(geometry, {100, 0}, 200, 0);
	animation.Cancel();
	require(animation.Advance(geometry, {100, 0}, 0.1, 1.5) == VerticalScrollPosition{100, 0});
	animation.Retarget(geometry, {}, -20, 0);
	require(!animation.Active());

	for (const double scale : {1.0, 1.25, 1.5, 3.0}) {
		HorizontalScrollGeometry horizontal(20000, scale);
		for (const double frequency : {30.0, 60.0, 144.0}) {
			SmoothScrollAnimation<HorizontalScrollGeometry> pan;
			double current = 100 / scale;
			pan.Retarget(horizontal, current, 160, 0);
			for (int frame = 1; frame <= static_cast<int>(frequency * 0.12); ++frame) {
				const double framePosition = pan.Advance(horizontal, current, frame / frequency, scale);
				require(framePosition >= current && framePosition <= 100 / scale + 160);
				// Only projected whole pixels come back from the real editor.
				current = std::round(framePosition * scale) / scale;
			}
			const double exact = pan.Advance(horizontal, current, 0.15, scale);
			require(std::abs(exact - (100 / scale + 160 * (1 - std::exp(-0.15 / 0.04)))) < 1e-8);
			current = std::round(exact * scale) / scale;
			pan.Retarget(horizontal, current, -3.25, 0.15);
			require(std::abs(pan.Target() - exact + 3.25) < 1e-8);
			const double reversed = pan.Advance(horizontal, current, 0.16, scale);
			require(reversed < exact);
			current = std::round(reversed * scale) / scale;
			require(pan.Finish(horizontal, current) == pan.Target() && !pan.Active());
		}
		for (const bool animate : {false, true}) {
			SmoothScrollAnimation<HorizontalScrollGeometry> pan;
			double current = 0;
			for (int event = 0; event < 32; ++event) {
				pan.Retarget(horizontal, current, 0.125 / scale, event, animate);
				const auto framePosition = animate ? pan.Advance(horizontal, current, event + 0.3, scale) :
					pan.Finish(horizontal, current);
				current = std::round(framePosition * scale) / scale;
				require(std::abs(pan.Target() * scale - (event + 1) * 0.125) < 1e-8);
			}
			require(current * scale == 4 && !pan.Active());
			pan.Retarget(horizontal, current, 200, 40);
			require(pan.Advance(horizontal, 500 / scale, 40.1, scale) == 500 / scale && !pan.Active());
			pan.Retarget(horizontal, 0, -200, 41);
			require(!pan.Active());
			pan.Retarget(horizontal, 0, std::numeric_limits<double>::max(), 42);
			require(pan.Finish(horizontal, 0) == 20000 / scale);
			pan.Retarget(horizontal, 0, 100, 43);
			HorizontalScrollGeometry changedDpi(20000, scale * 2);
			require(pan.Advance(changedDpi, 0, 43.1, scale * 2) == 0 && !pan.Active());
			pan.Retarget(horizontal, 0, 0.1 / scale, 44);
			pan.Finish(horizontal, 0);
			pan.Cancel();
			pan.Retarget(horizontal, 0, 0.1 / scale, 45);
			require(pan.Target() == 0.1 / scale);
		}
		HorizontalScrollGeometry rtl(20000, scale, true, 20000);
		SmoothScrollAnimation<HorizontalScrollGeometry> pan;
		pan.Retarget(rtl, rtl.FromPixels(18000), 160, 0);
		const double firstFrame = pan.Advance(rtl, rtl.FromPixels(18000), 0.02, scale);
		const int firstPixels = rtl.PixelOffset(firstFrame);
		HorizontalScrollGeometry grown(24000, scale, true, 24000);
		const double afterGrowth = pan.Advance(grown, grown.FromPixels(firstPixels + 4000), 0.04, scale);
		require(std::abs(afterGrowth - (2000 / scale + 160 * (1 - std::exp(-0.04 / 0.04)))) < 1e-8);
		require(pan.Active() && grown.PixelOffset(afterGrowth) < firstPixels + 4000);
		pan.Retarget(grown, grown.FromPixels(grown.PixelOffset(afterGrowth)), -10, 0.04);
		require(std::abs(pan.Target() - afterGrowth + 10) < 1e-8);
		require(grown.PixelOffset(grown.Clamp(-std::numeric_limits<double>::max())) == 24000);
		require(grown.PixelOffset(grown.Clamp(std::numeric_limits<double>::max())) == 0);
	}
	return checks;
}
