// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Reading/NativeReadingSnapshot.h"
#include <string>
#include <string_view>

namespace Notepads::Markdown::Native::TaskInteractionTests
{
    // Authored interaction fixture. Mixed original UTF8 newline widths and a
    // non-BMP prefix deliberately differ from Main UTF16 object coordinates.
    inline constexpr std::string_view FixtureSource{"中😀\r\n\r\n- [ ] Alpha\n- [x] Beta\r\n- [X] Gamma\r"};
    inline constexpr std::wstring_view FixtureMain{L"中😀\n\xfffc Alpha\n\xfffc Beta\n\xfffc Gamma"};
    inline constexpr std::string_view StaticFixtureSource{
        "<label><input type=\"CHECK&#98;OX\" checked disabled>中😀 <b>Alpha</b> &amp; one "
        "<img src=\"icon.png\" alt=\"图标\"></label>\r\n\r\n"
        "<label><input type=\"checkbox\" disabled=\"false\">Beta</label>\r\n\r\n"
        "<label><input type=\"checkbox\" checked=\"false\" disabled>Gamma</label>\r\n\r\n- [ ] GFM"};
    inline constexpr std::wstring_view StaticFixtureMain{
        L"\xfffc 中😀 Alpha & one \xfffc\n\xfffc Beta\n\xfffc Gamma\n\xfffc GFM"};

    struct Results
    {
        bool passed{};
        uint32_t transportChecks{};
        uint32_t clusterChecks{};
        uint32_t descriptorNegativeChecks{};
        bool sameDescriptorAcrossLayouts{};
        bool themedFocusPixels{};
        bool hiddenFocusHasNoPixels{};
        bool ownersReleased{};
        std::wstring detail;
    };

    // Background-only: real Core/FFI/DWrite Frames and WARP replay, no window,
    // UI input injection, or host Undo. Those remain independent UI gates.
    Results Run(std::shared_ptr<MainTextSnapshot const> const &main, notepads_markdown_core_cancel *sourceFlag);
    std::wstring RunStaticHtmlCheckbox(std::shared_ptr<MainTextSnapshot const> const &main,
                                       notepads_markdown_core_cancel *sourceFlag);
}
