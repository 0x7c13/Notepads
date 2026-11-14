// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#ifndef NOMINMAX
#define NOMINMAX
#endif
#include "DirectWriteRenderer.h"
#include "DirectWriteRunLayout.h"

#include <d2d1_3.h>
#include <dwrite_3.h>
#include <wrl/client.h>

#include <array>
#include <algorithm>
#include <atomic>
#include <cmath>
#include <cstdint>
#include <map>
#include <new>
#include <utility>
#include <vector>

namespace Scintilla::Internal {
namespace {

using Microsoft::WRL::ComPtr;

bool Finite(D2D1_RECT_F rectangle) noexcept {
	return std::isfinite(rectangle.left) && std::isfinite(rectangle.top) &&
		std::isfinite(rectangle.right) && std::isfinite(rectangle.bottom);
}

bool Intersects(D2D1_RECT_F a, D2D1_RECT_F b) noexcept {
	return a.right > b.left && a.left < b.right && a.bottom > b.top && a.top < b.bottom;
}

bool Translation(D2D1_MATRIX_3X2_F transform) noexcept {
	return transform._11 == 1 && transform._12 == 0 && transform._21 == 0 && transform._22 == 1 &&
		std::isfinite(transform._31) && std::isfinite(transform._32);
}

bool Finite(D2D1_MATRIX_3X2_F transform) noexcept {
	return std::isfinite(transform._11) && std::isfinite(transform._12) &&
		std::isfinite(transform._21) && std::isfinite(transform._22) &&
		std::isfinite(transform._31) && std::isfinite(transform._32);
}

// Retained runs bypass IDWriteTextLayout::Draw, which normally snaps the
// common baseline. Match its paint-only adjustment without rounding natural X
// positions or changing immutable caret geometry. DirectWrite applies DPI before
// the renderer transform; translation is therefore not scaled a second time.
float SnapBaseline(float y, D2D1_MATRIX_3X2_F transform, float pixelsPerDip) noexcept {
	float scale, translation;
	if (transform._12 == 0 && transform._21 == 0) {
		scale = transform._22;
		translation = transform._32;
	} else if (transform._11 == 0 && transform._22 == 0) {
		scale = transform._21;
		translation = transform._31;
	} else {
		// A non-axis-aligned baseline is not snapped by DirectWrite.
		return y;
	}
	if (scale == 0) return y;
	const float snapped = std::floor((y * pixelsPerDip) * scale + translation + 0.5f);
	D2D1_MATRIX_3X2_F inverse = transform;
	if (!D2D1InvertMatrix(&inverse)) return y;
	return (transform._12 == 0 && transform._21 == 0 ?
		snapped * inverse._22 + inverse._32 : snapped * inverse._12 + inverse._32) / pixelsPerDip;
}

class ClippedTextRenderer final : public IDWriteTextRenderer {
	struct FontBounds {
		// Keep the identity used as the map key alive even when a custom inline
		// object creates temporary font faces during its Draw callback.
		ComPtr<IDWriteFontFace> face;
		ComPtr<IDWritePaintReader> reader;
		bool outlineOnly = false;
	};

	std::atomic<ULONG> references{1};
	ComPtr<IDWriteFactory4> factory;
	ComPtr<ID2D1DeviceContext4> target;
	ComPtr<ID2D1DeviceContext7> modernTarget;
	ComPtr<ID2D1Brush> defaultBrush;
	ComPtr<ID2D1SolidColorBrush> paletteBrush;
	DWRITE_MATRIX transform;
	DWRITE_MATRIX deviceTransform;
	D2D1_RECT_F worldClip;
	float pixelsPerDip;
	BOOL disableSnapping;
	bool colourEnabled;
	bool checkBounds;
	bool filterRow;
	DWRITE_TEXT_RANGE row;
	D2D1_COLOR_BITMAP_GLYPH_SNAP_OPTION bitmapSnap;
	HRESULT failure = S_OK;
	// These are per-draw metadata/scratch buffers, not paragraph or device
	// caches. Most paragraphs use only one or two fallback faces.
	std::map<IDWriteFontFace *, FontBounds> fonts;
	std::vector<D2D1_POINT_2F> largeOrigins;
	std::span<const ParagraphBrushRun> foreground;
	struct Cluster {
		UINT32 glyphStart;
		UINT32 textStart;
		UINT32 textLength;
		ID2D1Brush *brush;
	};
	std::vector<Cluster> clusters;
	std::vector<UINT16> sliceMap;
	std::vector<D2D1_POINT_2F> penOrigins;

	ID2D1Brush *BrushAt(UINT32 position) const noexcept {
		const auto it = std::lower_bound(foreground.begin(), foreground.end(), position,
			[](const ParagraphBrushRun &range, UINT32 offset) {
				return uint64_t{range.text.startPosition} + range.text.length <= offset;
			});
		return it != foreground.end() && it->text.startPosition <= position ? it->brush : defaultBrush.Get();
	}

	HRESULT DrawWithForeground(D2D1_POINT_2F origin, const DWRITE_GLYPH_RUN *run,
		const DWRITE_GLYPH_RUN_DESCRIPTION *description, DWRITE_MEASURING_MODE mode) {
		if (!run->glyphCount) return S_OK;
		if (!description || !description->stringLength || !run->glyphIndices ||
			uint64_t{description->textPosition} + description->stringLength > UINT32_MAX) return E_INVALIDARG;
		const UINT32 runEnd = description->textPosition + description->stringLength;
		ID2D1Brush *firstBrush = BrushAt(description->textPosition);
		bool uniform = true;
		// Entirely covered single-colour runs need neither cluster metadata nor
		// reconstructed origins, including callbacks with optional arrays absent.
		const auto firstRange = std::lower_bound(foreground.begin(), foreground.end(), description->textPosition,
			[](const ParagraphBrushRun &range, UINT32 position) {
				return uint64_t{range.text.startPosition} + range.text.length <= position;
			});
		for (auto it = firstRange; it != foreground.end(); ++it) {
			const auto &range = *it;
			if (range.text.startPosition >= runEnd) break;
			if (range.text.startPosition > description->textPosition && BrushAt(range.text.startPosition) != firstBrush) uniform = false;
			const auto end = uint64_t{range.text.startPosition} + range.text.length;
			if (end > description->textPosition && end < runEnd && BrushAt(static_cast<UINT32>(end)) != firstBrush) uniform = false;
		}
		if (uniform) return DrawAcceptedRun(origin, run, description, firstBrush, mode);
		if (!description->clusterMap || !description->string) return E_INVALIDARG;
		clusters.clear();
		for (UINT32 text = 0; text < description->stringLength;) {
			const UINT32 glyph = description->clusterMap[text];
			if (glyph >= run->glyphCount) return E_INVALIDARG;
			const UINT32 first = text;
			while (text < description->stringLength && description->clusterMap[text] == glyph) ++text;
			clusters.push_back({glyph, first, text - first, BrushAt(description->textPosition + first)});
		}
		// Sorting cluster ownership never sorts glyphs: the slices still follow
		// the original glyph array and preserve overlapping ink composition.
		if (!std::is_sorted(clusters.begin(), clusters.end(), [](const Cluster &a, const Cluster &b) { return a.glyphStart < b.glyphStart; }))
			std::sort(clusters.begin(), clusters.end(), [](const Cluster &a, const Cluster &b) { return a.glyphStart < b.glyphStart; });
		if (clusters.empty() || clusters.front().glyphStart != 0) return E_INVALIDARG;
		for (size_t i = 1; i < clusters.size(); ++i)
			if (clusters[i - 1].glyphStart == clusters[i].glyphStart) return E_INVALIDARG;
		if (std::all_of(clusters.begin(), clusters.end(), [&](const Cluster &cluster) { return cluster.brush == clusters.front().brush; }))
			return DrawAcceptedRun(origin, run, description, clusters.front().brush, mode);

		// Remove offsets only for this placement query. The actual slices retain
		// their complete original offsets and receive them exactly once.
		DWRITE_GLYPH_RUN unshifted = *run;
		unshifted.glyphOffsets = nullptr;
		penOrigins.resize(run->glyphCount);
		HRESULT result = factory->ComputeGlyphOrigins(&unshifted, mode, origin, &deviceTransform, penOrigins.data());
		if (FAILED(result)) return result;
		for (size_t first = 0; first < clusters.size();) {
			size_t end = first + 1;
			UINT32 textFirst = clusters[first].textStart;
			UINT32 textEnd = textFirst + clusters[first].textLength;
			while (end < clusters.size() && clusters[end].brush == clusters[first].brush) {
				const auto &next = clusters[end];
				if (next.textStart == textEnd) textEnd += next.textLength;
				else if (next.textStart + next.textLength == textFirst) textFirst = next.textStart;
				else break;
				++end;
			}
			const UINT32 glyphFirst = clusters[first].glyphStart;
			const UINT32 glyphEnd = end < clusters.size() ? clusters[end].glyphStart : run->glyphCount;
			DWRITE_GLYPH_RUN slice = *run;
			slice.glyphCount = glyphEnd - glyphFirst;
			slice.glyphIndices += glyphFirst;
			if (slice.glyphAdvances) slice.glyphAdvances += glyphFirst;
			if (slice.glyphOffsets) slice.glyphOffsets += glyphFirst;
			DWRITE_GLYPH_RUN anchor = slice;
			anchor.glyphCount = 1;
			anchor.glyphOffsets = nullptr;
			D2D1_POINT_2F localAnchor{};
			result = factory->ComputeGlyphOrigins(&anchor, mode, {0, 0}, &deviceTransform, &localAnchor);
			if (FAILED(result)) return result;
			const D2D1_POINT_2F sliceOrigin{penOrigins[glyphFirst].x - localAnchor.x, penOrigins[glyphFirst].y - localAnchor.y};
			sliceMap.resize(textEnd - textFirst);
			for (UINT32 text = textFirst; text < textEnd; ++text) {
				const UINT32 glyph = description->clusterMap[text];
				if (glyph < glyphFirst || glyph >= glyphEnd) return E_INVALIDARG;
				sliceMap[text - textFirst] = static_cast<UINT16>(glyph - glyphFirst);
			}
			DWRITE_GLYPH_RUN_DESCRIPTION sliceDescription = *description;
			sliceDescription.string += textFirst;
			sliceDescription.textPosition += textFirst;
			sliceDescription.stringLength = textEnd - textFirst;
			sliceDescription.clusterMap = sliceMap.data();
			if (MayBeVisible(sliceOrigin, slice, mode)) {
				result = DrawAcceptedRun(sliceOrigin, &slice, &sliceDescription, clusters[first].brush, mode);
				if (FAILED(result)) return result;
			}
			first = end;
		}
		return S_OK;
	}

	HRESULT Record(HRESULT result) noexcept {
		if (FAILED(result) && SUCCEEDED(failure)) failure = result;
		return failure;
	}

	template <typename Action>
	HRESULT Invoke(Action &&action) noexcept {
		if (FAILED(failure)) return failure;
		try {
			return Record(action());
		} catch (const std::bad_alloc &) {
			return Record(E_OUTOFMEMORY);
		} catch (...) {
			return Record(E_FAIL);
		}
	}

	HRESULT BrushForEffect(IUnknown *effect, ComPtr<ID2D1Brush> &brush) const noexcept {
		if (effect) return effect->QueryInterface(IID_PPV_ARGS(&brush));
		brush = defaultBrush;
		return S_OK;
	}

	FontBounds &BoundsForFont(IDWriteFontFace *face) {
		if (const auto found = fonts.find(face); found != fonts.end()) return found->second;
		FontBounds bounds;
		bounds.face = face;
		ComPtr<IDWriteFontFace4> face4;
		if (SUCCEEDED(face->QueryInterface(IID_PPV_ARGS(&face4)))) {
			const auto formats = face4->GetGlyphImageFormats();
			constexpr unsigned outlineFormats = DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE | DWRITE_GLYPH_IMAGE_FORMATS_CFF;
			constexpr unsigned paintFormats = outlineFormats | DWRITE_GLYPH_IMAGE_FORMATS_COLR |
				DWRITE_GLYPH_IMAGE_FORMATS_COLR_PAINT_TREE;
			bounds.outlineOnly = (formats & outlineFormats) && !(formats & ~outlineFormats);
			// An outline's black box does not bound bitmap/SVG or COLRv1 paint.
			// Only use the paint reader when it describes the representation the
			// system renderer will choose, and simulated outlines cannot alter it.
			if (colourEnabled && modernTarget && (formats & DWRITE_GLYPH_IMAGE_FORMATS_COLR_PAINT_TREE) && !(formats & ~paintFormats) &&
				face->GetSimulations() == DWRITE_FONT_SIMULATIONS_NONE) {
				ComPtr<IDWriteFontFace7> face7;
				if (SUCCEEDED(face->QueryInterface(IID_PPV_ARGS(&face7)))) {
					// Failure only disables this optional visibility optimisation.
					if (FAILED(face7->CreatePaintReader(DWRITE_GLYPH_IMAGE_FORMATS_COLR_PAINT_TREE,
						modernTarget->GetPaintFeatureLevel(), &bounds.reader))) bounds.reader.Reset();
				}
			}
		}
		return fonts.emplace(face, std::move(bounds)).first->second;
	}

	bool MayBeVisible(D2D1_POINT_2F origin, const DWRITE_GLYPH_RUN &run,
		DWRITE_MEASURING_MODE measuringMode) {
		if (!checkBounds || run.isSideways) return true;
		auto &bounds = BoundsForFont(run.fontFace);
		if (bounds.outlineOnly) {
			D2D1_RECT_F box{};
			// This API includes the target's transform/DPI. Compare with the
			// translated clip, never with the caller's untransformed rectangle.
			if (FAILED(target->GetGlyphRunWorldBounds(origin, &run, measuringMode, &box)) || !Finite(box)) return true;
			return Intersects(box, worldClip);
		}
		if (!bounds.reader) return true;

		std::array<D2D1_POINT_2F, 8> localOrigins;
		auto *origins = localOrigins.data();
		if (run.glyphCount > localOrigins.size()) {
			largeOrigins.resize(run.glyphCount);
			origins = largeOrigins.data();
		}
		// DirectWrite owns RTL advances, glyph offsets and GDI-compatible
		// rounding. Do not reimplement those placement rules here.
		if (FAILED(factory->ComputeGlyphOrigins(&run, measuringMode, origin, &deviceTransform, origins))) return true;
		for (UINT32 glyph = 0; glyph < run.glyphCount; ++glyph) {
			DWRITE_PAINT_ELEMENT element{};
			D2D_RECT_F box{};
			if (FAILED(bounds.reader->SetCurrentGlyph(run.glyphIndices[glyph], &element, &box)) ||
				element.paintType == DWRITE_PAINT_TYPE_NONE || !Finite(box) ||
				box.right <= box.left || box.bottom <= box.top) return true;
			// Empty means no guaranteed clip box, not an invisible glyph. A
			// nonempty box is a mandatory bound of this glyph's paint tree.
			box.left = box.left * run.fontEmSize + origins[glyph].x + transform.dx;
			box.right = box.right * run.fontEmSize + origins[glyph].x + transform.dx;
			box.top = box.top * run.fontEmSize + origins[glyph].y + transform.dy;
			box.bottom = box.bottom * run.fontEmSize + origins[glyph].y + transform.dy;
			if (!Finite(box) || Intersects(box, worldClip)) return true;
		}
		return false;
	}

	HRESULT DrawAcceptedRun(D2D1_POINT_2F origin, const DWRITE_GLYPH_RUN *run,
		const DWRITE_GLYPH_RUN_DESCRIPTION *description, ID2D1Brush *brush,
		DWRITE_MEASURING_MODE measuringMode) {
		if (!colourEnabled) {
			target->DrawGlyphRun(origin, run, description, brush, measuringMode);
			return S_OK;
		}
		if (modernTarget) {
			// Prefer the system's current format selection, including COLRv1.
			modernTarget->DrawGlyphRunWithColorSupport(origin, run, description, brush,
				nullptr, 0, measuringMode, bitmapSnap);
			return S_OK;
		}
		// Context4 and Factory4 support every colour format available on our
		// Windows 10 minimum. Factory2's COLR-only translation loses SVG/bitmaps.
		constexpr DWRITE_GLYPH_IMAGE_FORMATS formats =
			DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE | DWRITE_GLYPH_IMAGE_FORMATS_CFF |
			DWRITE_GLYPH_IMAGE_FORMATS_COLR | DWRITE_GLYPH_IMAGE_FORMATS_SVG |
			DWRITE_GLYPH_IMAGE_FORMATS_PNG | DWRITE_GLYPH_IMAGE_FORMATS_JPEG |
			DWRITE_GLYPH_IMAGE_FORMATS_TIFF | DWRITE_GLYPH_IMAGE_FORMATS_PREMULTIPLIED_B8G8R8A8;
		ComPtr<IDWriteColorGlyphRunEnumerator1> layers;
		HRESULT result = factory->TranslateColorGlyphRun(origin, run, description, formats,
			measuringMode, &deviceTransform, 0, &layers);
		if (result == DWRITE_E_NOCOLOR) {
			target->DrawGlyphRun(origin, run, description, brush, measuringMode);
			return S_OK;
		}
		if (FAILED(result)) return result;
		if (!layers) return E_UNEXPECTED;
		for (;;) {
			BOOL hasLayer = FALSE;
			result = layers->MoveNext(&hasLayer);
			if (FAILED(result) || !hasLayer) return result;
			const DWRITE_COLOR_GLYPH_RUN1 *layer = nullptr;
			result = layers->GetCurrentRun(&layer);
			if (FAILED(result)) return result;
			if (!layer) return E_UNEXPECTED;
			const D2D1_POINT_2F layerOrigin{layer->baselineOriginX, layer->baselineOriginY};
			switch (static_cast<unsigned>(layer->glyphImageFormat)) {
			case DWRITE_GLYPH_IMAGE_FORMATS_NONE:
				break;
			case DWRITE_GLYPH_IMAGE_FORMATS_PNG:
			case DWRITE_GLYPH_IMAGE_FORMATS_JPEG:
			case DWRITE_GLYPH_IMAGE_FORMATS_TIFF:
			case DWRITE_GLYPH_IMAGE_FORMATS_PREMULTIPLIED_B8G8R8A8:
				target->DrawColorBitmapGlyphRun(layer->glyphImageFormat, layerOrigin,
					&layer->glyphRun, layer->measuringMode, bitmapSnap);
				break;
			case DWRITE_GLYPH_IMAGE_FORMATS_SVG:
				target->DrawSvgGlyphRun(layerOrigin, &layer->glyphRun, brush, nullptr, 0, layer->measuringMode);
				break;
			case DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE:
			case DWRITE_GLYPH_IMAGE_FORMATS_CFF:
			case DWRITE_GLYPH_IMAGE_FORMATS_COLR:
			// Factory4 can retain COLR alongside the layer's outline format.
			// These are already translated palette layers, not paint trees.
			case DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE | DWRITE_GLYPH_IMAGE_FORMATS_COLR:
			case DWRITE_GLYPH_IMAGE_FORMATS_CFF | DWRITE_GLYPH_IMAGE_FORMATS_COLR: {
				ID2D1Brush *layerBrush = brush;
				if (layer->paletteIndex != DWRITE_NO_PALETTE_INDEX) {
					const D2D1_COLOR_F colour{layer->runColor.r, layer->runColor.g, layer->runColor.b, layer->runColor.a};
					if (!paletteBrush) {
						result = target->CreateSolidColorBrush(colour, &paletteBrush);
						if (FAILED(result)) return result;
					} else {
						paletteBrush->SetColor(colour);
					}
					// Fixed palette layers use their own alpha. Foreground layers
					// retain the original brush, including its opacity/transform.
					layerBrush = paletteBrush.Get();
				}
				target->DrawGlyphRun(layerOrigin, &layer->glyphRun,
					layer->glyphRunDescription, layerBrush, layer->measuringMode);
				break;
			}
			default:
				return E_NOTIMPL;
			}
		}
	}

public:
	ClippedTextRenderer(IDWriteFactory4 *factory_, ID2D1DeviceContext4 *target_, ID2D1DeviceContext7 *modern,
		ID2D1Brush *brush, D2D1_MATRIX_3X2_F matrix, D2D1_RECT_F clip, float dpi,
		D2D1_DRAW_TEXT_OPTIONS options, const DWRITE_TEXT_RANGE *row_, std::span<const ParagraphBrushRun> foreground_) noexcept :
		factory(factory_), target(target_), modernTarget(modern), defaultBrush(brush),
		transform{matrix._11, matrix._12, matrix._21, matrix._22, matrix._31, matrix._32},
		deviceTransform{matrix._11 * dpi / 96, matrix._12 * dpi / 96, matrix._21 * dpi / 96,
			matrix._22 * dpi / 96, matrix._31 * dpi / 96, matrix._32 * dpi / 96},
		worldClip{clip.left + matrix._31 - 96 / dpi, clip.top + matrix._32 - 96 / dpi,
			clip.right + matrix._31 + 96 / dpi, clip.bottom + matrix._32 + 96 / dpi},
		pixelsPerDip(dpi / 96), disableSnapping((options & D2D1_DRAW_TEXT_OPTIONS_NO_SNAP) != 0),
		colourEnabled((options & D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT) != 0),
		checkBounds(Translation(matrix)), filterRow(row_ != nullptr), row(row_ ? *row_ : DWRITE_TEXT_RANGE{}),
		bitmapSnap((options & D2D1_DRAW_TEXT_OPTIONS_DISABLE_COLOR_BITMAP_SNAPPING) ?
			D2D1_COLOR_BITMAP_GLYPH_SNAP_OPTION_DISABLE : D2D1_COLOR_BITMAP_GLYPH_SNAP_OPTION_DEFAULT), foreground(foreground_) {
		// One device pixel of outward slack covers antialiasing and aliased
		// clip rounding; it is not an assumed font overhang allowance.
	}

	[[nodiscard]] HRESULT Status() const noexcept { return failure; }

	HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **result) override {
		if (!result) return E_POINTER;
		*result = nullptr;
		if (iid == __uuidof(IUnknown) || iid == __uuidof(IDWritePixelSnapping) || iid == __uuidof(IDWriteTextRenderer)) {
			*result = static_cast<IDWriteTextRenderer *>(this);
			AddRef();
			return S_OK;
		}
		return E_NOINTERFACE;
	}
	ULONG STDMETHODCALLTYPE AddRef() override { return ++references; }
	ULONG STDMETHODCALLTYPE Release() override {
		const ULONG count = --references;
		if (!count) delete this;
		return count;
	}
	HRESULT STDMETHODCALLTYPE IsPixelSnappingDisabled(void *, BOOL *disabled) override {
		if (!disabled) return Record(E_POINTER);
		*disabled = disableSnapping;
		return failure;
	}
	HRESULT STDMETHODCALLTYPE GetCurrentTransform(void *, DWRITE_MATRIX *matrix) override {
		if (!matrix) return Record(E_POINTER);
		*matrix = transform;
		return failure;
	}
	HRESULT STDMETHODCALLTYPE GetPixelsPerDip(void *, FLOAT *value) override {
		if (!value) return Record(E_POINTER);
		*value = pixelsPerDip;
		return failure;
	}
	HRESULT STDMETHODCALLTYPE DrawGlyphRun(void *, FLOAT x, FLOAT y, DWRITE_MEASURING_MODE measuringMode,
		const DWRITE_GLYPH_RUN *run, const DWRITE_GLYPH_RUN_DESCRIPTION *description, IUnknown *effect) override {
		return Invoke([&]() -> HRESULT {
			if (!run || !run->fontFace) return E_INVALIDARG;
			if (filterRow) {
				if (!description) return E_INVALIDARG;
				const uint64_t start = description->textPosition;
				const uint64_t end = start + description->stringLength;
				const uint64_t rowEnd = uint64_t{row.startPosition} + row.length;
				if (start >= rowEnd || end <= row.startPosition) return S_OK;
				// DirectWrite line breaks divide complete glyph callbacks. Never
				// split a shaped run or let another row's overhang into this row.
				if (start < row.startPosition || end > rowEnd) return E_INVALIDARG;
			}
			const D2D1_POINT_2F origin{x, y};
			if (!MayBeVisible(origin, *run, measuringMode)) return S_OK;
			if (!foreground.empty()) return DrawWithForeground(origin, run, description, measuringMode);
			ComPtr<ID2D1Brush> brush;
			const HRESULT result = BrushForEffect(effect, brush);
			if (FAILED(result)) return result;
			return DrawAcceptedRun(origin, run, description, brush.Get(), measuringMode);
		});
	}
	HRESULT STDMETHODCALLTYPE DrawUnderline(void *, FLOAT, FLOAT,
		const DWRITE_UNDERLINE *, IUnknown *) override {
		// Scintilla paints style/hotspot underlines after the paragraph glyphs.
		// Reject a violated layout contract instead of silently losing an effect.
		return Record(E_NOTIMPL);
	}
	HRESULT STDMETHODCALLTYPE DrawStrikethrough(void *, FLOAT, FLOAT,
		const DWRITE_STRIKETHROUGH *, IUnknown *) override {
		return Record(E_NOTIMPL);
	}
	HRESULT STDMETHODCALLTYPE DrawInlineObject(void *context, FLOAT x, FLOAT y, IDWriteInlineObject *object,
		BOOL sideways, BOOL rtl, IUnknown *effect) override {
		return Invoke([&]() -> HRESULT {
			if (!object) return E_POINTER;
			return object->Draw(context, this, x, y, sideways, rtl, effect);
		});
	}
};

}

namespace {

HRESULT DrawParagraph(IDWriteFactory *factory, ID2D1RenderTarget *target, IDWriteTextLayout *layout,
	std::span<const ParagraphGlyphRun> runs, float baseline,
	D2D1_POINT_2F origin, D2D1_RECT_F clip, ID2D1Brush *brush,
	D2D1_DRAW_TEXT_OPTIONS options, const DWRITE_TEXT_RANGE *row, std::span<const ParagraphBrushRun> foreground) noexcept {
	if (!factory || !target || !brush) return E_POINTER;
	if (!Finite(clip) || !std::isfinite(origin.x) || !std::isfinite(origin.y)) return E_INVALIDARG;
	if (!std::isfinite(baseline)) return E_INVALIDARG;
	if (row && uint64_t{row->startPosition} + row->length > UINT32_MAX) return E_INVALIDARG;
	uint64_t previousEnd = 0;
	for (const auto &range : foreground) {
		const uint64_t end = uint64_t{range.text.startPosition} + range.text.length;
		if (!range.brush || !range.text.length || end > UINT32_MAX || range.text.startPosition < previousEnd) return E_INVALIDARG;
		previousEnd = end;
	}
	if (clip.right <= clip.left || clip.bottom <= clip.top) return S_OK;
	if (row && !row->length) return S_OK;
	try {
		const auto systemDraw = [&]() noexcept {
			target->DrawTextLayout(origin, layout, brush, options);
			return S_OK;
		};
		// Only an unfiltered request may delegate to DrawTextLayout. Doing so
		// for a row would silently reintroduce neighbouring-row ink and colours.
		const auto unsupported = [&](HRESULT result) noexcept { return !layout || row || !foreground.empty() ? result : systemDraw(); };
		constexpr unsigned supportedOptions = D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT |
			D2D1_DRAW_TEXT_OPTIONS_NO_SNAP | D2D1_DRAW_TEXT_OPTIONS_DISABLE_COLOR_BITMAP_SNAPPING;
		if (options & ~supportedOptions) return unsupported(E_NOTIMPL);
		if (layout && !row && foreground.empty() && !(options & D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT)) return systemDraw();
		ComPtr<ID2D1DeviceContext4> context;
		ComPtr<ID2D1DeviceContext7> modern;
		ComPtr<IDWriteFactory4> placement;
		HRESULT capability = target->QueryInterface(IID_PPV_ARGS(&context));
		if (FAILED(capability)) return unsupported(capability);
		capability = factory->QueryInterface(IID_PPV_ARGS(&placement));
		if (FAILED(capability)) return unsupported(capability);
		// Absence selects the full Context4 colour path, not monochrome output.
		(void)target->QueryInterface(IID_PPV_ARGS(&modern));
		D2D1_MATRIX_3X2_F transform;
		target->GetTransform(&transform);
		FLOAT dpiX, dpiY;
		target->GetDpi(&dpiX, &dpiY);
		if (!Finite(transform)) return E_INVALIDARG;
		if (context->GetUnitMode() != D2D1_UNIT_MODE_DIPS ||
			!std::isfinite(dpiX) || dpiX <= 0 || dpiX != dpiY ||
			(layout && (layout->GetFlowDirection() != DWRITE_FLOW_DIRECTION_TOP_TO_BOTTOM ||
			(layout->GetReadingDirection() != DWRITE_READING_DIRECTION_LEFT_TO_RIGHT &&
				layout->GetReadingDirection() != DWRITE_READING_DIRECTION_RIGHT_TO_LEFT)))) return unsupported(E_NOTIMPL);
		if (layout && !row && foreground.empty() && !Translation(transform)) return systemDraw();

		ComPtr<ClippedTextRenderer> renderer;
		renderer.Attach(new ClippedTextRenderer(placement.Get(), context.Get(), modern.Get(), brush,
			transform, clip, dpiX, options, row, foreground));
		float paintBaseline = origin.y + baseline;
		if (!layout && !(options & D2D1_DRAW_TEXT_OPTIONS_NO_SNAP))
			paintBaseline = SnapBaseline(paintBaseline, transform, dpiX / 96);
		if (!layout && !std::isfinite(paintBaseline)) return E_INVALIDARG;
		HRESULT result = S_OK;
		if (layout) result = layout->Draw(nullptr, renderer.Get(), origin.x, origin.y);
		else for (const auto &run : runs) {
			result = renderer->DrawGlyphRun(nullptr, origin.x + run.originX, paintBaseline,
				DWRITE_MEASURING_MODE_NATURAL, &run.glyphs, &run.text, nullptr);
			if (FAILED(result)) break;
		}
		// Some DirectWrite versions keep drawing and return S_OK after a failed
		// callback. Preserve its first failure and never repaint a partial draw.
		return FAILED(renderer->Status()) ? renderer->Status() : result;
	} catch (const std::bad_alloc &) {
		return E_OUTOFMEMORY;
	} catch (...) {
		return E_FAIL;
	}
}

}

HRESULT DrawParagraphTextClipped(IDWriteFactory *factory, ID2D1RenderTarget *target,
	IDWriteTextLayout *layout, D2D1_POINT_2F origin, D2D1_RECT_F clip, ID2D1Brush *brush,
	D2D1_DRAW_TEXT_OPTIONS options, const DWRITE_TEXT_RANGE *row, std::span<const ParagraphBrushRun> foreground) noexcept {
	if (!layout) return E_POINTER;
	return DrawParagraph(factory, target, layout, {}, 0, origin, clip, brush, options, row, foreground);
}

HRESULT DrawParagraphGlyphRunsClipped(IDWriteFactory *factory, ID2D1RenderTarget *target,
	std::span<const ParagraphGlyphRun> runs, float baseline, D2D1_POINT_2F origin,
	D2D1_RECT_F clip, ID2D1Brush *brush, D2D1_DRAW_TEXT_OPTIONS options,
	std::span<const ParagraphBrushRun> foreground) noexcept {
	return DrawParagraph(factory, target, nullptr, runs, baseline, origin, clip, brush, options, nullptr, foreground);
}

}
