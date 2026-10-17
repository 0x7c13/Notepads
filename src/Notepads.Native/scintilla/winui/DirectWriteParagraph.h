// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#ifndef DIRECTWRITEPARAGRAPH_H
#define DIRECTWRITEPARAGRAPH_H

#include <cstddef>
#include <cstdint>
#include <memory>
#include <optional>
#include <span>
#include <string_view>
#include <vector>

#include "ParagraphLayout.h"

struct IDWriteFactory;
struct IDWriteTextFormat;
struct IDWriteTextLayout;

namespace Scintilla::Internal {

struct ParagraphGlyphRun;

enum class ParagraphWrap { None, Word, Character };

struct ParagraphOptions {
	float width = 1;
	float lineHeight = 1;
	float baseline = 1;
	float tabWidth = 4;
	bool rtl = false;
	ParagraphWrap wrap = ParagraphWrap::None;
	// Logical leading/trailing space inside the outer document canvas. The
	// trailing space holds EOL markers without covering reordered text.
	float leadingPadding = 0;
	float trailingPadding = 0;
};

struct ParagraphFontRun {
	size_t start = 0;
	size_t length = 0;
	IDWriteTextFormat *format = nullptr;
};

struct ParagraphRepresentation {
	size_t start = 0;
	size_t length = 0;
	float width = 0;
};

// One physical document line, with its complete paragraph context. The immutable
// layout owns no rendering-target resources and can be tested without a window.
// All public offsets are UTF-8 bytes, including font and representation spans.
class DirectWriteParagraph final : public IParagraphLayout {
	class Impl;
	std::unique_ptr<Impl> impl;
public:
	DirectWriteParagraph(IDWriteFactory *factory, std::string_view text,
		IDWriteTextFormat *defaultFormat, std::span<const ParagraphFontRun> fonts,
		std::span<const ParagraphRepresentation> representations, ParagraphOptions options);
	~DirectWriteParagraph() override;
	DirectWriteParagraph(const DirectWriteParagraph &) = delete;
	DirectWriteParagraph &operator=(const DirectWriteParagraph &) = delete;
	[[nodiscard]] static bool NeedsLayout(std::string_view text);

	[[nodiscard]] int32_t Status() const noexcept;
	[[nodiscard]] bool IsValid() const noexcept;
	// Rendering uses either the native layout or prepared glyph runs. A valid
	// analyzed paragraph has no IDWriteTextLayout; both views remain immutable.
	[[nodiscard]] IDWriteTextLayout *Layout() const noexcept;
	[[nodiscard]] std::span<const ParagraphGlyphRun> GlyphRuns() const noexcept;
	[[nodiscard]] float DrawOriginX() const noexcept;
	[[nodiscard]] std::span<const ParagraphLine> Lines() const noexcept override;
	[[nodiscard]] float WidthIncludingTrailingWhitespace() const noexcept override;
	[[nodiscard]] float WidthIncludingPadding() const noexcept override;
	[[nodiscard]] float LayoutWidth() const noexcept override;
	[[nodiscard]] size_t Length() const noexcept override;
	[[nodiscard]] size_t CaretLine(ParagraphCaret caret) const noexcept override;
	[[nodiscard]] uint32_t Utf16Position(size_t bytePosition) const noexcept;
	[[nodiscard]] size_t BytePosition(uint32_t utf16Position) const noexcept;

	[[nodiscard]] ParagraphCaret HitTest(float x, float y, bool charPosition = false) const override;
	[[nodiscard]] ParagraphPoint CaretPoint(ParagraphCaret caret) const override;
	[[nodiscard]] std::vector<ParagraphRect> RangeRectangles(size_t start, size_t end) const override;
	[[nodiscard]] std::vector<ParagraphRange> VisualRanges(size_t line, float left, float right) const override;
	// Visual moves stay within this paragraph and return nullopt at its outer
	// edge. The editor owns transitions to adjacent document paragraphs.
	[[nodiscard]] std::optional<ParagraphCaret> VisualNeighbor(ParagraphCaret caret, int direction) const override;
	[[nodiscard]] ParagraphCaret VisualEdge(size_t line, bool right) const override;
};

}

#endif
