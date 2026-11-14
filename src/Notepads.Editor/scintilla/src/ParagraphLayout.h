// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#ifndef PARAGRAPHLAYOUT_H
#define PARAGRAPHLAYOUT_H

#include <cstddef>
#include <optional>
#include <span>
#include <vector>

namespace Scintilla::Internal {

struct ParagraphCaret {
	size_t position = 0;
	// The preceding cluster's trailing edge and the following cluster's leading
	// edge can differ at a bidirectional or wrapped-line boundary.
	bool trailing = false;
	bool operator==(const ParagraphCaret &) const noexcept = default;
};

struct ParagraphPoint {
	float x = 0;
	float y = 0;
	float height = 0;
};

struct ParagraphRect {
	float left = 0;
	float top = 0;
	float width = 0;
	float height = 0;
	size_t start = 0;
	size_t length = 0;
};

struct ParagraphLine {
	size_t start = 0;
	size_t length = 0;
	float top = 0;
	float height = 0;
	float baseline = 0;
	size_t trailingWhitespaceLength = 0;
};

struct ParagraphRange {
	size_t start = 0;
	size_t length = 0;
};

// Geometry for one complete physical document line, excluding its EOL bytes.
// All offsets are UTF-8 bytes. Vertical coordinates describe uniform display
// rows, not font-dependent glyph bounds. The platform owns shaping and drawing;
// the editor consumes this interface without depending on a graphics backend.
class IParagraphLayout {
public:
	virtual ~IParagraphLayout() = default;
	[[nodiscard]] virtual std::span<const ParagraphLine> Lines() const noexcept = 0;
	[[nodiscard]] virtual float WidthIncludingTrailingWhitespace() const noexcept = 0;
	[[nodiscard]] virtual float WidthIncludingPadding() const noexcept = 0;
	[[nodiscard]] virtual float LayoutWidth() const noexcept = 0;
	[[nodiscard]] virtual size_t Length() const noexcept = 0;
	[[nodiscard]] virtual size_t CaretLine(ParagraphCaret caret) const noexcept = 0;
	[[nodiscard]] virtual ParagraphCaret HitTest(float x, float y, bool charPosition = false) const = 0;
	[[nodiscard]] virtual ParagraphPoint CaretPoint(ParagraphCaret caret) const = 0;
	[[nodiscard]] virtual std::vector<ParagraphRect> RangeRectangles(size_t start, size_t end) const = 0;
	[[nodiscard]] virtual std::vector<ParagraphRange> VisualRanges(size_t line, float left, float right) const = 0;
	// Return nullopt at the paragraph's outer edge; the editor owns transitions
	// to the preceding or following physical document line.
	[[nodiscard]] virtual std::optional<ParagraphCaret> VisualNeighbor(ParagraphCaret caret, int direction) const = 0;
	[[nodiscard]] virtual ParagraphCaret VisualEdge(size_t line, bool right) const = 0;
};

}

#endif
