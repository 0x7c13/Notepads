// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#ifndef DIRECTWRITERUNLAYOUT_H
#define DIRECTWRITERUNLAYOUT_H

#include <dwrite.h>
#include <memory>
#include <span>
#include <string_view>
#include <vector>

namespace Scintilla::Internal {

struct ParagraphOptions;

struct ParagraphFormatRange {
	UINT32 start;
	UINT32 length;
	IDWriteTextFormat *format;
};

struct ParagraphGlyphRun {
	DWRITE_GLYPH_RUN glyphs;
	DWRITE_GLYPH_RUN_DESCRIPTION text;
	float originX;
	float left;
	float width;
};

struct ParagraphGlyphCluster {
	UINT32 start;
	UINT32 length;
	UINT32 run;
	float width;
	float left = 0;
	float right = 0;
};

// Immutable single-row LTR-base geometry from full-paragraph Unicode analysis. Text is
// borrowed from the enclosing DirectWriteParagraph and must outlive this object.
// Shaping, geometry and drawing share the same retained glyph and cluster data.
class DirectWriteRunLayout {
	class Impl;
	std::unique_ptr<Impl> impl;
	DirectWriteRunLayout();
public:
	static constexpr size_t MinimumLength = 16 * 1024;
	~DirectWriteRunLayout();
	DirectWriteRunLayout(const DirectWriteRunLayout &) = delete;
	DirectWriteRunLayout &operator=(const DirectWriteRunLayout &) = delete;
	// S_FALSE keeps unsupported or inexpensive paragraphs on native layout.
	// Only dense, bounded full contexts are admitted; no geometry is published
	// until the complete plan, shaping proofs and cluster ownership are valid.
	// Allocation/COM errors remain failures, not requests for another allocation.
	[[nodiscard]] static HRESULT TryCreate(IDWriteFactory *factory, std::wstring_view text,
		IDWriteTextFormat *format, std::span<const ParagraphFormatRange> formats,
		const ParagraphOptions &options, std::unique_ptr<DirectWriteRunLayout> &result);
	[[nodiscard]] float Width() const noexcept;
	[[nodiscard]] UINT32 TrailingWhitespace() const noexcept;
	[[nodiscard]] std::span<const ParagraphGlyphRun> GlyphRuns() const noexcept;
	// Navigation reads the same immutable coordinates as caret/hit testing;
	// re-accumulating advances loses visual edge agreement at distant positions.
	[[nodiscard]] std::span<const ParagraphGlyphCluster> GlyphClusters() const noexcept;
	[[nodiscard]] float TextPosition(UINT32 position, bool trailing) const noexcept;
	[[nodiscard]] DWRITE_HIT_TEST_METRICS HitPoint(float x, bool &trailing) const noexcept;
	[[nodiscard]] std::vector<DWRITE_HIT_TEST_METRICS> Range(UINT32 start, UINT32 length) const;
};

}

#endif
