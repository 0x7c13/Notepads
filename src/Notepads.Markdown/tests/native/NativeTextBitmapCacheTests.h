// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Reading/NativeReadingSnapshot.h"
#include <string>
#include <string_view>

namespace Notepads::Markdown::Native::TextBitmapCacheTests
{
    // Preserve authored assertion text across cleanup and thread transitions;
    // HRESULT restricted-error information is not the diagnostic transport.
    struct Failure
    {
        std::wstring detail;
    };

    // The first paragraph is the unchanged Emoji sequence from Markdown
    // Rendering Lab.md, CASE 05-B, line 349. Keep its ZWJ/VS/modifier sequences.
    inline constexpr std::string_view FixtureSource =
        "😀 😅 🫠 🥹 🫨 🥳 🧪 ✨ · 👍 👍🏻 👍🏽 👍🏿 · 👩🏽‍💻 👨‍👩‍👧‍👦 🧑‍🚀 · 🇨🇳 🇯🇵 🇺🇳 · 🏳️‍🌈 🏴‍☠️ · 1️⃣ #️⃣ *️⃣\n\n"
        "Mixed 😀 中 é שלום **bold** *italic 🧪* ~~strike ✨~~.";
    inline constexpr std::wstring_view FixtureMain =
        L"😀 😅 🫠 🥹 🫨 🥳 🧪 ✨ · 👍 👍🏻 👍🏽 👍🏿 · 👩🏽‍💻 👨‍👩‍👧‍👦 🧑‍🚀 · 🇨🇳 🇯🇵 🇺🇳 · 🏳️‍🌈 🏴‍☠️ · 1️⃣ #️⃣ *️⃣\n"
        L"Mixed 😀 中 é שלום bold italic 🧪 strike ✨.";

    // Independent fixture: BMP heading/paragraph/table text plus supplementary
    // family Emoji, CJK and combining marks in an inline-code/link paragraph. The original
    // two-paragraph Lab Emoji fixture above remains unchanged.
    inline constexpr std::string_view StaticFixtureSource =
        "# Static heading\n\n"
        "Plain BMP 中 Ω **bold** *italic* ~~strike~~.\n\n"
        "| Head A | Head B |\n|---|---|\n| Static cell | **Bold cell** |\n\n"
        "A [linked label](https://example.invalid) stays dynamic.\n\n"
        "An `👨‍👩‍👧‍👦 𠮷 ā́` badge [stays static](https://example.invalid/code).";
    inline constexpr std::wstring_view StaticFixtureMain =
        L"Static heading\nPlain BMP 中 Ω bold italic strike.\n"
        L"Head A\tHead B\nStatic cell\tBold cell\n"
        L"A linked label stays dynamic.\nAn 👨‍👩‍👧‍👦 𠮷 ā́ badge stays static.";

    // Background-only; borrows the real reading fixture's original model.
    // One bounded WARP pixel oracle, with no window or public ABI additions.
    std::wstring Run(std::shared_ptr<ReadingMemory> const &memory);
    std::wstring RunStatic(std::shared_ptr<ReadingMemory> const &memory);
    std::wstring RunScripts(std::shared_ptr<ReadingMemory> const &memory);
    std::wstring RunImageRelayout(std::shared_ptr<ReadingMemory> const &memory);
}
