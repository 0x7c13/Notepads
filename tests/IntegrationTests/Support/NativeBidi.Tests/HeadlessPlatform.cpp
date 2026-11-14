// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <string>
#include <string_view>
#include <vector>

#include "ScintillaTypes.h"
#include "Geometry.h"
#include "Platform.h"
#include "Debugging.h"
#include "UniqueString.h"
#include "Indicator.h"
#include "LineMarker.h"
#include "Style.h"
#include "ViewStyle.h"

// PositionCache shares a translation unit with document/style consumers. Link
// their real core implementations, but never substitute fake platform behavior.
// These tests shape through DirectWrite directly and must not request UI state.
namespace {
thread_local bool constructingGeometryStyle = false;
[[noreturn]] void UnexpectedPlatformCall(const char *name) noexcept {
    std::fprintf(stderr, "FAIL: headless cache test requested platform operation %s\n", name);
    std::abort();
}
}

Scintilla::Internal::ViewStyle MakeGeometryTestStyle() {
    // ViewStyle stores these defaults in its constructor, but the geometry
    // fixture never realizes a font or uses theme colors. Keep the exception
    // confined to construction so other headless consumers remain fail-closed.
    struct DefaultsScope {
        DefaultsScope() { constructingGeometryStyle = true; }
        ~DefaultsScope() { constructingGeometryStyle = false; }
    } defaults;
    return Scintilla::Internal::ViewStyle();
}

namespace Scintilla::Internal {
std::shared_ptr<Font> Font::Allocate(const FontParameters &) { UnexpectedPlatformCall("Font::Allocate"); }

namespace Platform {
ColourRGBA Chrome() {
    if (constructingGeometryStyle) return ColourRGBA::Grey(240);
    UnexpectedPlatformCall("Chrome");
}
ColourRGBA ChromeHighlight() {
    if (constructingGeometryStyle) return ColourRGBA::Grey(255);
    UnexpectedPlatformCall("ChromeHighlight");
}
const char *DefaultFont() {
    if (constructingGeometryStyle) return "Consolas";
    UnexpectedPlatformCall("DefaultFont");
}
int DefaultFontSize() {
    if (constructingGeometryStyle) return 12;
    UnexpectedPlatformCall("DefaultFontSize");
}
void DebugPrintf(const char *, ...) noexcept { UnexpectedPlatformCall("DebugPrintf"); }
void Assert(const char *condition, const char *file, int line) noexcept {
    std::fprintf(stderr, "FAIL: Scintilla assertion %s at %s:%d\n", condition, file, line);
    std::abort();
}
}
}
