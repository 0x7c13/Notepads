// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include <windows.h>
#include <algorithm>
#include <cstddef>
#include <limits>
#include <stdexcept>
#include <string>
#include <string_view>

#include "Utf8PositionIndex.h"

size_t CheckUtf8PositionIndex() {
    using namespace Scintilla::Internal;
    size_t checks = 0;
    const auto require = [&](bool condition) {
        if (!condition) throw std::runtime_error("UTF-8 position index geometry/work budget");
        ++checks;
    };
    const auto mixed = std::string(reinterpret_cast<const char *>(u8"中文😀e\u0301\t")) + '\0';
    for (size_t prefix = 250; prefix <= 260; ++prefix) {
        std::string text(prefix, 'x');
        for (int i = 0; i < 40; ++i) text += mixed;
        const Utf8PositionIndex index(text);
        // Descending queries include the interior of surrogate/multibyte sequences
        // and cross every checkpoint. Preserve existing prefix-conversion semantics.
        for (size_t position = text.size() + 1; position-- > 0;) {
            size_t scanned = 0;
            require(index.Utf16Position(position, &scanned) == UTF16Length(std::string_view(text).substr(0, position)));
            require(scanned <= 259);
        }
        for (size_t position = 0; position <= text.size();) {
            const int expected = position == 0 ? 0 : MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS,
                text.data(), static_cast<int>(position), nullptr, 0);
            require(position == 0 || expected > 0);
            require(index.Utf16Position(position) == static_cast<size_t>(expected));
            if (position == text.size()) break;
            position += UTF8DrawBytes(text.data() + position, text.size() - position);
        }
        require(index.Utf16Position(std::numeric_limits<size_t>::max()) == UTF16Length(text));
        for (size_t position = 0; position <= UTF16Length(text) + 1; ++position) {
            size_t scanned = 0;
            require(index.BytePosition(position, &scanned) == UTF8PositionFromUTF16Position(text, position));
            require(scanned <= 259);
        }
        require(index.BytePosition(std::numeric_limits<size_t>::max()) == text.size());
    }
    for (const bool nonAscii : {false, true}) {
        std::string text(256 * 1024, 'x');
        if (nonAscii) text += mixed;
        const Utf8PositionIndex index(text);
        size_t totalScanned = 0;
        for (size_t i = 0; i < 2000; ++i) {
            // Distant, non-monotonic endpoints must not depend on a warm cursor.
            const size_t position = (i * 7919) % (256 * 1024);
            size_t scanned = 0;
            require(index.Utf16Position(position, &scanned) == position);
            require(scanned <= (nonAscii ? 259U : 0U));
            totalScanned += scanned;
            require(index.BytePosition(position, &scanned) == position);
            require(scanned <= (nonAscii ? 259U : 0U));
        }
        require(totalScanned <= 2000 * 259);
    }
    require(Utf8PositionIndex().Utf16Position(100) == 0);
    require(Utf8PositionIndex().BytePosition(100) == 0);
    // Native prefix conversion also has defined behavior for a truncated code point.
    for (const std::string tail : {"\xf0", "\xf0\x9f", "\xf0\x9f\x98"}) {
        const std::string text = std::string(255, 'x') + tail;
        const Utf8PositionIndex index(text);
        for (size_t position = 254; position <= text.size(); ++position)
            require(index.Utf16Position(position) == UTF16Length(std::string_view(text).substr(0, position)));
        for (size_t position = 254; position <= 258; ++position)
            require(index.BytePosition(position) == UTF8PositionFromUTF16Position(text, position));
    }
    return checks;
}
