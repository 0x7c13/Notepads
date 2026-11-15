// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Reading/NativeReadingSnapshot.h"
#include <string_view>

namespace Notepads::Markdown::Native::ScriptTests
{
    inline constexpr std::string_view FixtureSource =
        "pre <sup>a</sup><sub>́</sub> post.\n\n"
        "same <sup>a</sup><sup>́</sup> mono <sup>á</sup>.\n\n"
        "[linked <sup>raise</sup> end](target)\n\n"
        "<sub><code>x</code><kbd>y</kbd><samp>z</samp></sub>\n\n"
        "<sup>image ![tiny](missing.png)</sup>\n\n"
        "<sup>$x$</sup>\n\n"
        "entity <sup>a&#x301;</sup> emoji <sup>👩&#x200d;💻</sup>\n\n"
        "link-entity <sup>[👩&#x200d;💻](target)</sup>\n\n"
        "two-links <sup>[left](target)[right](target)</sup>\n\n"
        "<sup>👩</sup><sub>‍💻</sub>\n\n"
        "<sub>A</sub><br><sup>B</sup>\n\n"
        "<sup>̧́</sup>\n\n"
        "<code>̧́</code>\n\n"
        "<sup><code>̧́</code></sup>\n\n"
        "tail <sup>note<br></sup>";
    inline constexpr std::wstring_view FixtureMain =
        L"pre á post.\nsame á mono á.\nlinked raise end\nxyz\nimage \xfffc\n\xfffc\n"
        L"entity á emoji 👩‍💻\nlink-entity 👩‍💻\ntwo-links leftright\n👩‍💻\nA\nB\ņ́\ņ́\ņ́\ntail note\n";

    // Additional authored short scripts exercise the same four-slot queue. The
    // original shaping/geometry fixture and its expected Main remain unchanged.
    inline constexpr std::string_view NearbySuffixSource =
        "\n\nnear-one <sup>🧪</sup>\n\nnear-two <sup>🧪</sup>"
        "\n\nnear-three <sup>🧪</sup>\n\nnear-four <sup>🧪</sup>";
    inline constexpr std::wstring_view NearbySuffixMain =
        L"\nnear-one 🧪\nnear-two 🧪\nnear-three 🧪\nnear-four 🧪";

    // Runs against an accepted current model and its actual Rust/native leases.
    std::wstring Run(std::shared_ptr<ReadingMemory> const &memory);
}
