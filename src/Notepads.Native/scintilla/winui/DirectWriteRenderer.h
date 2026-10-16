// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#ifndef DIRECTWRITERENDERER_H
#define DIRECTWRITERENDERER_H

#include <d2d1.h>
#include <dwrite.h>
#include <span>

namespace Scintilla::Internal {

struct ParagraphBrushRun {
	// Sorted, nonoverlapping UTF-16 ranges in the complete layout. Brushes are
	// borrowed until the synchronous draw returns; the layout never retains them.
	DWRITE_TEXT_RANGE text;
	ID2D1Brush *brush;
};

// Draw an already-shaped Scintilla paragraph through the caller's clip. A row
// limits drawing to complete glyph runs in that UTF-16 range of the layout;
// neighbouring rows never contribute overhanging ink. A null row draws all runs.
// origin and clip use the render target's pre-transform coordinates. Row draws
// require Direct2D Context4/DirectWrite Factory4 (Windows 10 14393+) and equal
// positive DPI in DIPs. They support colour, NO_SNAP and bitmap-snap options;
// unsupported contracts fail before drawing instead of losing the row filter.
// Calls without either a row or foreground overrides retain the system fallback
// for other targets/options. Nonempty overrides suppress callback effects;
// uncovered text uses the default brush. A complete shaped cluster takes the
// brush at its first logical code unit, preserving glyphs and geometry even
// when an override boundary lies inside a combining, ligature or emoji cluster.
// Scintilla owns decorations: this glyph layout must not set DirectWrite underline
// or strikethrough. No target resources survive this call.
// Direct2D drawing errors remain deferred to the caller's EndDraw/Flush; this
// result also reports DirectWrite and custom inline-object callback failures.
[[nodiscard]] HRESULT DrawParagraphTextClipped(IDWriteFactory *factory, ID2D1RenderTarget *target,
	IDWriteTextLayout *layout, D2D1_POINT_2F origin, D2D1_RECT_F clip, ID2D1Brush *brush,
	D2D1_DRAW_TEXT_OPTIONS options, const DWRITE_TEXT_RANGE *row = nullptr,
	std::span<const ParagraphBrushRun> foreground = {}) noexcept;

}

#endif
