// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#ifndef VERTICALSCROLL_H
#define VERTICALSCROLL_H

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <limits>
#include "Position.h"

namespace Scintilla::Internal {

struct VerticalScrollPosition {
	Sci::Line row = 0;
	double offset = 0; // DIPs within the first display row, before pixel projection.
	bool operator==(const VerticalScrollPosition &) const noexcept = default;
};

// Operate on a row and a local distance: even very large documents retain exact
// row identity. All consumers project the same rounded offset, never independently
// round a document-sized coordinate or accumulate rounded input deltas.
class VerticalScrollGeometry {
	double height;
	double scale;
	VerticalScrollPosition maximum;
public:
	using Position = VerticalScrollPosition;
	VerticalScrollGeometry(int lineHeight, double scale_, double viewportPixels,
		Sci::Line rows, bool endAtLastLine) noexcept :
		height(lineHeight / scale_), scale(scale_) {
		if (rows <= 0) return;
		if (!endAtLastLine) { maximum.row = rows - 1; return; }
		const double viewport = std::max(0.0, viewportPixels);
		const double wholeRows = std::floor(viewport / lineHeight);
		if (wholeRows >= static_cast<double>(rows)) return;
		const double remainder = viewport - wholeRows * lineHeight;
		maximum.row = rows - static_cast<Sci::Line>(wholeRows);
		if (remainder > 0) {
			--maximum.row;
			maximum.offset = (lineHeight - remainder) / scale;
		}
	}

	double RowHeight() const noexcept { return height; }
	double UnitSize() const noexcept { return height; }
	bool Matches(Position left, Position right) const noexcept { return left == right; }
	Position Add(Position position, double delta) const noexcept { return Clamp({position.row, position.offset + delta}); }
	VerticalScrollPosition Maximum() const noexcept { return maximum; }
	VerticalScrollPosition Clamp(VerticalScrollPosition position) const noexcept {
		if (position.row < 0) return {};
		if (position.row > maximum.row) return maximum;
		const double rows = std::floor(position.offset / height);
		// Compare before narrowing or adding, including at the signed line limit.
		if (rows < -static_cast<double>(position.row)) return {};
		if (rows > static_cast<double>(maximum.row - position.row)) return maximum;
		if (rows >= static_cast<double>(std::numeric_limits<Sci::Line>::max())) return maximum;
		position.row += static_cast<Sci::Line>(rows);
		position.offset -= rows * height;
		if (position.row < 0) return {};
		if (position.row > maximum.row ||
			(position.row == maximum.row && position.offset > maximum.offset)) return maximum;
		return position;
	}
	int PixelOffset(VerticalScrollPosition position) const noexcept {
		return static_cast<int>(std::round(position.offset * scale));
	}
	double Distance(VerticalScrollPosition from, VerticalScrollPosition to) const noexcept {
		return static_cast<double>(to.row - from.row) * height + to.offset - from.offset;
	}
};

}

#endif
