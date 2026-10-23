// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once

#include <d2d1.h>
#include "ScintillaTypes.h"

namespace Scintilla::Internal {

constexpr D2D1_TEXT_ANTIALIAS_MODE TextAntialiasMode(FontQuality quality) noexcept {
	switch (quality & FontQuality::QualityMask) {
	case FontQuality::QualityNonAntialiased: return D2D1_TEXT_ANTIALIAS_MODE_ALIASED;
	case FontQuality::QualityAntialiased: return D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE;
	case FontQuality::QualityLcdOptimized: return D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE;
	default: return D2D1_TEXT_ANTIALIAS_MODE_DEFAULT;
	}
}

}
