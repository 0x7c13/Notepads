// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include <cstddef>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <stdexcept>
#include <string>
#include <string_view>
#include <vector>

#include "ScintillaTypes.h"
#include "Debugging.h"
#include "Geometry.h"
#include "Platform.h"
#include "UniqueString.h"
#include "Indicator.h"
#include "LineMarker.h"
#include "Style.h"
#include "ViewStyle.h"

Scintilla::Internal::ViewStyle MakeGeometryTestStyle();

size_t CheckGutterGeometry() {
    using namespace Scintilla::Internal;
    size_t checks = 0;
    const auto require = [&](bool condition, const char *message) {
        if (!condition) throw std::runtime_error(message);
        ++checks;
    };
    const auto rectangle = [&](PRectangle actual, PRectangle expected, const char *message) {
        require(actual.left == expected.left && actual.top == expected.top &&
            actual.right == expected.right && actual.bottom == expected.bottom, message);
    };
    ViewStyle style = MakeGeometryTestStyle();
    style.marginInside = true;
    style.leftMarginWidth = 6;
    style.rightMarginWidth = 2;
    style.ms = {MarginStyle(Scintilla::MarginType::Number, 20),
        MarginStyle(Scintilla::MarginType::Symbol, 8)};
    style.CalculateMarginWidthAndMask();
    const PRectangle client(10, 20, 310, 220);
    require(style.GutterWidth() == 28, "The gutter excludes asymmetric physical text spacing");
    require(style.TextStart(false) == 34 && style.RightInset(false) == 2,
        "LTR places both margin columns before the six-pixel text gap");
    require(style.TextStart(true) == 6 && style.RightInset(true) == 30,
        "RTL moves the gutter while preserving physical left and right spacing");
    rectangle(style.TextRectangle(client, false), PRectangle(44, 20, 308, 220), "LTR text bounds include client origin");
    rectangle(style.TextRectangle(client, true), PRectangle(16, 20, 280, 220), "RTL text bounds reserve the right gutter");
    rectangle(style.MarginRectangle(client, false), PRectangle(10, 20, 38, 220), "LTR gutter excludes the adjacent text gap");
    rectangle(style.MarginRectangle(client, true), PRectangle(282, 20, 310, 220), "RTL gutter occupies the physical right edge");

    struct Hit { double x; bool rtl; int margin; };
    for (const auto hit : {
        Hit{9.99, false, -1}, {10, false, 0}, {29.99, false, 0},
        {30, false, 1}, {37.99, false, 1}, {38, false, -1},
        {281.99, true, -1}, {282, true, 1}, {289.99, true, 1},
        {290, true, 0}, {309.99, true, 0}, {310, true, -1}}) {
        require(style.MarginFromLocation(Point(hit.x, 30), client, hit.rtl) == hit.margin,
            "Margin hit testing follows physical half-open boundaries and reversed column placement");
    }

    style.ms[0].width = 0;
    style.CalculateMarginWidthAndMask();
    require(style.GutterWidth() == 8 && style.MarginFromLocation(Point(10, 30), client, false) == 1 &&
        style.MarginFromLocation(Point(309, 30), client, true) == 1,
        "A hidden first margin does not consume width or steal the remaining margin's hits");
    style.ms[1].width = 0;
    style.CalculateMarginWidthAndMask();
    for (const bool rtl : {false, true}) {
        require(style.GutterWidth() == 0 && style.TextStart(rtl) == 6 && style.RightInset(rtl) == 2,
            "Disabling all margins leaves only the original physical spacing");
        rectangle(style.TextRectangle(client, rtl), PRectangle(16, 20, 308, 220), "Hidden gutters recover the full text width");
        require(style.MarginRectangle(client, rtl).Width() == 0 &&
            style.MarginFromLocation(Point(10, 30), client, rtl) == -1 &&
            style.MarginFromLocation(Point(309, 30), client, rtl) == -1,
            "Zero-width gutters paint and capture no area");
    }

    style.marginInside = false;
    style.ms[0].width = 20;
    style.ms[1].width = 8;
    style.CalculateMarginWidthAndMask();
    for (const bool rtl : {false, true}) {
        require(style.ExternalMarginWidth() == 28 && style.TextStart(rtl) == 6 && style.RightInset(rtl) == 2,
            "A separate native margin window retains its existing text layout in either direction");
        rectangle(style.MarginRectangle(client, rtl), PRectangle(10, 20, 38, 220), "External margin painting remains left based");
        require(style.MarginFromLocation(Point(-28, 30), client, rtl) == 0 &&
            style.MarginFromLocation(Point(-8, 30), client, rtl) == 1 &&
            style.MarginFromLocation(Point(0, 30), client, rtl) == -1,
            "External margin hit testing preserves the legacy negative-coordinate contract");
    }
    return checks;
}
