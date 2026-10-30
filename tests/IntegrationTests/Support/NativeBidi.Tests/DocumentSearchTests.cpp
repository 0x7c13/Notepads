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

size_t CheckPreparedReplacementDecorations() {
    using namespace Scintilla;
    using namespace Scintilla::Internal;
    size_t checks = 0;
    const auto require = [&](bool condition) {
        if (!condition) throw std::runtime_error("Prepared replacement decoration regression.");
        ++checks;
    };
    const auto text = [](Document &document) {
        std::string result(static_cast<size_t>(document.Length()), '\0');
        document.GetCharRange(result.data(), 0, document.Length());
        return result;
    };
    const auto checkRuns = [&](Document &document) {
        document.decorations->SetCurrentIndicator(8);
        if (document.Length()) {
            document.decorations->FillRange(document.Length() - 1, 1, 1);
            require(document.decorations->ValueAt(8, document.Length() - 1) == 1);
        }
        for (const auto *decoration : document.decorations->View()) {
            require(decoration->Length() == document.Length());
            // The paint loop must progress even across unmarked runs and EOLs.
            for (Sci::Position position = 0; position < document.Length();) {
                const auto end = decoration->EndRun(position);
                require(end > position && end <= document.Length());
                position = end;
            }
        }
    };
    const std::array<std::string_view, 4> contents{"", "a", "b", "cat Cat scatter cat\rsecond line\rthird"};
    for (const auto before : contents) {
        for (const auto after : contents) {
            Document document(DocumentOption::Default);
            document.InsertString(0, before);
            document.SetSavePoint();
            checkRuns(document);
            Document candidate(DocumentOption::Default);
            candidate.SetUndoCollection(false);
            require(candidate.AddData(after.data(), static_cast<Sci_Position>(after.size())) == static_cast<int>(Status::Ok));
            checkRuns(candidate);
            document.PrepareWholeDocumentReplacement(candidate);
            document.ReplaceWithPreparedDocument(candidate);
            require(text(document) == after);
            checkRuns(document);
            if (!before.empty() || !after.empty()) {
                document.Undo();
                require(text(document) == before);
                checkRuns(document);
                document.Redo();
                require(text(document) == after);
                checkRuns(document);
            }
            // Also materialize a decoration after an empty replacement.
            document.InsertString(document.Length(), "!");
            checkRuns(document);
        }
    }
    return checks;
}

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
    // Candidate limits constrain starts only: matches and whole-word checks
    // retain the complete document range, including across the buffer gap.
    for (const auto haystack : {"a\0b a\0b"sv, " prefix prefixX PREFIX "sv, "\xc3\x9f SS ss "sv, "\xe4\xb8\xad\xe6\x96\x87 x"sv}) {
        for (const auto query : {"a\0b"sv, "prefix"sv, "SS"sv, "\xc3\x9f"sv, "\xe4\xb8\xad\xe6\x96\x87"sv}) {
            Document document(DocumentOption::Default);
            document.SetDBCSCodePage(CpUtf8);
            document.SetCaseFolder(std::make_unique<CaseFolderUnicode>());
            document.InsertString(0, haystack);
            const auto gap = document.MovePositionOutsideChar(document.Length() / 2, 1, false);
            document.InsertString(gap, "!");
            document.DeleteChars(gap, 1);
            require(document.GapPosition() == gap, "Candidate-limit fixture places the buffer gap.");
            for (const auto flags : {FindOption::None, FindOption::MatchCase, FindOption::WholeWord}) {
                for (Sci::Position start = 0; start < document.Length(); start = document.NextPosition(start, 1)) {
                    auto expectedLength = static_cast<Sci::Position>(query.size());
                    const auto expected = document.FindText(start, document.Length(), query.data(), flags, &expectedLength);
                    for (auto end = start; end <= document.Length(); ++end) {
                        auto length = static_cast<Sci::Position>(query.size());
                        const auto actual = document.FindText(start, document.Length(), query.data(), flags, &length, end);
                        require(actual == (expected >= end ? -1 : expected) && (actual < 0 || length == expectedLength),
                            "Forward candidate limits preserve Unicode folding, NUL, whole words and crossing matches.");
                    }
                }
            }
        }
    }
    return checks;
}
