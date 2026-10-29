// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <iterator>
#include <map>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <string_view>
#include <vector>
#include "ScintillaTypes.h"
#include "ILoader.h"
#include "ILexer.h"
#include "Debugging.h"
#include "CharacterType.h"
#include "CharacterCategoryMap.h"
#include "Position.h"
#include "SplitVector.h"
#include "Partitioning.h"
#include "RunStyles.h"
#include "CellBuffer.h"
#include "CharClassify.h"
#include "Decoration.h"
#include "CaseFolder.h"
#include "Document.h"

size_t CheckDocumentSearchBounds() {
    using namespace Scintilla;
    using namespace Scintilla::Internal;
    using namespace std::literals;
    constexpr auto text = "abcabcd\0c"sv;
    constexpr std::array queries{"ab"sv, "bc"sv, "cd"sv, "c"sv, "abcd"sv,
        "BC"sv, "c\0"sv, "d\0"sv, "\0c"sv, "\0"sv, "abcabcd\0c?"sv};
    size_t checks = 0;
    auto require = [&](bool condition, const std::string &message) {
        if (!condition) throw std::runtime_error(message);
        ++checks;
    };
    auto lower = [](std::string_view value) {
        std::string result(value);
        for (auto &ch : result) if (ch >= 'A' && ch <= 'Z') ch = static_cast<char>(ch + ('a' - 'A'));
        return result;
    };

    for (const int codePage : {0, CpUtf8}) {
        for (size_t gap = 0; gap <= text.size(); ++gap) {
            Document document(DocumentOption::Default);
            document.SetDBCSCodePage(codePage);
            if (codePage == CpUtf8) document.SetCaseFolder(std::make_unique<CaseFolderUnicode>());
            else document.SetCaseFolder(std::make_unique<CaseFolderTable>());
            document.InsertString(0, text);
            document.InsertString(static_cast<Sci::Position>(gap), "!");
            document.DeleteChars(static_cast<Sci::Position>(gap), 1);
            require(document.GapPosition() == static_cast<Sci::Position>(gap), "Search fixture places the buffer gap");
            for (size_t start = 0; start <= text.size(); ++start) {
                for (size_t end = start; end <= text.size(); ++end) {
                    for (const auto query : queries) {
                        for (const bool backward : {false, true}) {
                            for (const bool ignoreCase : {false, true}) {
                                // A bounded standard-library view is independent of Scintilla's
                                // split buffer, candidate scan and out-of-document CharAt behavior.
                                const auto range = text.substr(start, end - start);
                                const auto haystack = ignoreCase ? lower(range) : std::string(range);
                                const auto needle = ignoreCase ? lower(query) : std::string(query);
                                const auto found = backward ? haystack.rfind(needle) : haystack.find(needle);
                                const auto expected = found == std::string::npos ? Sci::Position{-1} :
                                    static_cast<Sci::Position>(start + found);
                                auto length = static_cast<Sci::Position>(query.size());
                                const auto actual = document.FindText(
                                    static_cast<Sci::Position>(backward ? end : start),
                                    static_cast<Sci::Position>(backward ? start : end), query.data(),
                                    ignoreCase ? FindOption::None : FindOption::MatchCase, &length);
                                require(actual == expected && (actual < 0 || length == static_cast<Sci::Position>(query.size())),
                                    "Search bounds: range=" + std::to_string(start) + ":" + std::to_string(end) +
                                    " gap=" + std::to_string(gap) + " backward=" + std::to_string(backward) +
                                    " ignoreCase=" + std::to_string(ignoreCase) + " queryBytes=" + std::to_string(query.size()) +
                                    " expected=" + std::to_string(expected) + " actual=" + std::to_string(actual));
                            }
                        }
                    }
                }
            }
        }
    }
    return checks;
}
