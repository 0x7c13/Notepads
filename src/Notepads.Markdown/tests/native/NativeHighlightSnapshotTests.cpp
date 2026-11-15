// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeHighlightSnapshotTests.h"
#include "Reading/NativeHighlightSnapshot.h"
#include <algorithm>
#include <array>
#include <chrono>
#include <cstring>
#include <exception>
#include <source_location>
#include <string_view>
#include <thread>

namespace Notepads::Markdown::Native::HighlightSnapshotTests
{
    namespace
    {
        enum class Case : uint8_t
        {
            Unicode,
            Skipped,
            Ownership
        };

        struct AssertionFailure
        {
            int32_t status;
            std::wstring detail;
        };

        void Require(bool condition, std::wstring const &detail,
                     std::source_location location = std::source_location::current())
        {
            if (!condition)
                throw AssertionFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE, L"Highlight diagnostic line " +
                                                                  std::to_wstring(location.line()) + L": " + detail};
        }

        void Check(int32_t status)
        {
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
                throw NativeFailure{status};
        }

        template <typename T> T Pod() noexcept
        {
            T value{};
            value.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, static_cast<uint32_t>(sizeof(T))};
            return value;
        }

        template <typename Predicate> bool Until(Predicate predicate)
        {
            auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(5);
            while (!predicate())
            {
                if (std::chrono::steady_clock::now() >= deadline)
                    return false;
                std::this_thread::sleep_for(std::chrono::milliseconds(1));
            }
            return true;
        }

        template <typename F> void ExpectStatus(int32_t expected, F &&body)
        {
            try
            {
                body();
            }
            catch (NativeFailure const &failure)
            {
                Require(failure.status == expected, L"Expected status=" + std::to_wstring(expected) + L", actual=" +
                                                        std::to_wstring(failure.status));
                return;
            }
            Require(false, L"Expected native failure=" + std::to_wstring(expected));
        }

        std::string Source(Case kind)
        {
            if (kind == Case::Skipped)
                return "```unknown\nplain😀\n```\n\n```rs\n" + std::string(8193, 'x') + "\n```\n";
            std::string source = "before\r\n\r\n```json\r\n{\"a\": 42, \"b\": \"中😀\"}\r\n```\r\n\r\n"
                                 "```RuSt\r\n// 😀\r\nlet 名 = \"😀\";\r\nlet n = 42;\r\n```\r\n";
            for (auto alias : {"cpp", "cc", "hpp", "hh", "hxx", "h++", "CC", "HpP"})
                source += std::string("\r\n```") + alias +
                          " extra\r\n// 中😀\r\nconst char* s = \"😀\";\r\nint n = 42;\r\n```\r\n";
            for (auto alias : {"yaml", "yml", "YAML"})
                source += std::string("\r\n```") + alias + "\r\n# 中😀\r\nname: \"😀\"\r\ncount: 42\r\n```\r\n";
            for (auto alias : {"sh", "bash", "SHELL"})
                source += std::string("\r\n```") + alias +
                          "\r\n# 中😀\r\nprintf '%s\\n' 'Markdown literal * * *'\r\nif true; then\r\n    echo \"中😀 $value\"\r\nfi\r\n```\r\n";
            source += "\r\n```csharp\r\n// 中😀\r\n```\r\n"
                      "\r\n```html\r\n<strong>中😀</strong>\r\n```\r\n"
                      "\r\n```css\r\n.x { width: 42px; }\r\n```\r\n"
                      "\r\n```sql\r\nSELECT '中😀';\r\n```\r\n"
                      "\r\n```diff\r\n-旧😀\r\n+新😀\r\n```\r\n"
                      "\r\n```go\r\nfunc Sample() {}\r\n```\r\n"
                      "\r\n```java\r\npublic class Sample {}\r\n```\r\n"
                      "\r\n```gv\r\n// 中文 😀\r\ndigraph { start -> end [label=\"示例\"]; }\r\n```\r\n"
                      "\r\n```octave\r\n% 中文 😀\r\nx = 42;\r\ndisp('示例');\r\n```\r\n"
                      "\r\n```delphi\r\n{ 中文 😀 }\r\nbegin\r\n  WriteLn('示例');\r\nend.\r\n```\r\n"
                      "\r\n```reStructuredText\r\n.. note::\r\n\r\n   Literal: ``示例``\r\n```\r\n"
                      "\r\n```osascript\r\n-- 中文 😀\r\nset labelText to \"示例\"\r\nreturn labelText\r\n```\r\n"
                      "\r\n```Dlang\r\n/+ 中文 😀 +/\r\nimmutable string label = \"示例\";\r\n```\r\n"
                      "\r\n```haml\r\n/ 中文 😀\r\n%section.panel\r\n  %p= \"示例\"\r\n```\r\n"
                      "\r\n```html+ruby\r\n<%# 中文 😀 %>\r\n<strong><%= \"示例\" %></strong>\r\n```\r\n"
                      "\r\n```JSONC\r\n{\r\n  // 中文 😀\r\n  \"count\": 42,\r\n}\r\n```\r\n"
                      "\r\n```zsh\r\n# 中文 😀\r\nprintf '%s\\n' '示例'\r\n```\r\n"
                      "\r\n```xhtml\r\n<!-- 中文 😀 -->\r\n<br title=\"示例\" />\r\n```\r\n"
                      "\r\n```phtml\r\n<!-- 中文 😀 -->\r\n<section><?= label() ?></section>\r\n```\r\n"
                      "\r\n```ts:line-numbers=9 {1}\r\ninterface User { name: string }\r\n```\r\n"
                      "\r\n```tsx\r\nconst node = <section title={name}>你好 {name}</section>;\r\n```\r\n"
                      "\r\n```pwsh\r\n# 参数示例\r\nWrite-Output \"你好\"\r\n```\r\n"
                      "\r\n```toml\r\n[project]\r\nenabled = true\r\n```\r\n"
                      "\r\n```containerfile\r\nFROM alpine:3.22 AS build\r\nRUN echo \"ready\"\r\n```\r\n";
            return source;
        }

        bool Boundary(std::wstring_view text, size_t offset) noexcept
        {
            if (offset > text.size())
                return false;
            return offset == 0 || offset == text.size() ||
                   !(text[offset - 1] >= 0xD800 && text[offset - 1] <= 0xDBFF && text[offset] >= 0xDC00 &&
                     text[offset] <= 0xDFFF);
        }

        std::wstring Describe(notepads_markdown_highlight_summary const &value)
        {
            return L"blocks=" + std::to_wstring(value.blocks) + L", spans=" + std::to_wstring(value.spans) +
                   L", Complete=" + std::to_wstring(value.complete_blocks) + L", Skipped=" +
                   std::to_wstring(value.skipped_blocks) + L", Failed=" + std::to_wstring(value.failed_blocks) +
                   L", entryLimited=" + std::to_wstring(value.entry_limited);
        }

        void CheckText(NativeHighlightSnapshot const &snapshot, MainTextSnapshot const &main,
                       notepads_markdown_highlight_block const &block, std::wstring_view expected)
        {
            Require(block.logical_end >= block.logical_start && block.logical_end <= main.Text().size() &&
                        main.Text().substr(static_cast<size_t>(block.logical_start),
                                           static_cast<size_t>(block.logical_end - block.logical_start)) == expected,
                    L"Code logical range differs from fixed semantic UTF16 text; node=" + std::to_wstring(block.node));
            auto spans = snapshot.Spans(block);
            uint64_t position{};
            for (auto const &span : spans)
            {
                Require(span.start_utf16 == position && span.length_utf16 != 0 &&
                            Boundary(expected, span.start_utf16) &&
                            Boundary(expected, uint64_t{span.start_utf16} + span.length_utf16),
                        L"Span coverage or scalar boundary failed; node=" + std::to_wstring(block.node) + L", start=" +
                            std::to_wstring(span.start_utf16) + L", length=" + std::to_wstring(span.length_utf16));
                position += span.length_utf16;
            }
            Require(block.state != NOTEPADS_MARKDOWN_HIGHLIGHT_COMPLETE || position == expected.size(),
                    L"Complete token partition must cover the exact fixed UTF16 text.");
        }

        notepads_markdown_highlight_span const &At(NativeHighlightSnapshot const &snapshot, notepads_markdown_highlight_block const &block,
                                    uint32_t position)
        {
            for (auto const &span : snapshot.Spans(block))
                if (span.start_utf16 <= position && position - span.start_utf16 < span.length_utf16)
                    return span;
            throw AssertionFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE,
                                   L"No token covers fixed code position=" + std::to_wstring(position)};
        }

        void CheckUnicode(NativeHighlightSnapshot const &snapshot, MainTextSnapshot const &main)
        {
            auto const &summary = snapshot.Summary();
            Require(summary.blocks == 40 && summary.complete_blocks == 40 && summary.skipped_blocks == 0 &&
                        summary.failed_blocks == 0 && summary.entry_limited == 0,
                    L"Fixed fence summary; " + Describe(summary));
            auto const rows = snapshot.Blocks();
            Require(rows[0].language == 3 && rows[1].language == 2 && rows[0].node < rows[1].node &&
                        snapshot.Find(rows[0].node) == &rows[0] && snapshot.Find(rows[1].node) == &rows[1] &&
                        snapshot.Find(UINT32_MAX) == nullptr,
                    L"Fenced languages and node lookups must match actual Main CodeBlocks.");
            constexpr std::wstring_view json = L"{\"a\": 42, \"b\": \"中😀\"}\n";
            constexpr std::wstring_view rust = L"// 😀\nlet 名 = \"😀\";\nlet n = 42;\n";
            CheckText(snapshot, main, rows[0], json);
            CheckText(snapshot, main, rows[1], rust);
            auto const &number = At(snapshot, rows[0], 6);
            Require(number.role == NOTEPADS_MARKDOWN_HIGHLIGHT_NUMBER && number.start_utf16 == 6 && number.length_utf16 == 2 &&
                        At(snapshot, rows[0], 17).role == NOTEPADS_MARKDOWN_HIGHLIGHT_STRING,
                    L"JSON fixed number(6,2) and Unicode string roles.");
            Require(At(snapshot, rows[1], 3).role == NOTEPADS_MARKDOWN_HIGHLIGHT_COMMENT &&
                        At(snapshot, rows[1], 15).role == NOTEPADS_MARKDOWN_HIGHLIGHT_STRING &&
                        At(snapshot, rows[1], 28).role == NOTEPADS_MARKDOWN_HIGHLIGHT_NUMBER,
                    L"Rust fixed Unicode comment/string/number positions 3/15/28.");
            constexpr std::wstring_view cpp = L"// 中😀\nconst char* s = \"😀\";\nint n = 42;\n";
            auto canonical = snapshot.Spans(rows[2]);
            Require(At(snapshot, rows[2], 3).role == NOTEPADS_MARKDOWN_HIGHLIGHT_COMMENT &&
                        At(snapshot, rows[2], static_cast<uint32_t>(cpp.find(L"42"))).role == NOTEPADS_MARKDOWN_HIGHLIGHT_NUMBER,
                    L"C++ canonical comment/number roles must be real tokens.");
            for (size_t index = 2; index < 10; ++index)
            {
                Require(rows[index].language == 1 && rows[index].state == NOTEPADS_MARKDOWN_HIGHLIGHT_COMPLETE,
                        L"C++ fence aliases must reuse the existing language code and complete state.");
                CheckText(snapshot, main, rows[index], cpp);
                auto tokens = snapshot.Spans(rows[index]);
                Require(tokens.size() == canonical.size(), L"C++ alias token counts differ.");
                for (size_t token = 0; token < tokens.size(); ++token)
                    Require(std::memcmp(&tokens[token], &canonical[token], sizeof(notepads_markdown_highlight_span)) == 0,
                            L"C++ alias tokens differ from cpp, including Unicode scalar boundaries.");
            }
            constexpr std::wstring_view yaml = L"# 中😀\nname: \"😀\"\ncount: 42\n";
            for (size_t index = 10; index < 13; ++index)
            {
                Require(rows[index].language == 6 && rows[index].state == NOTEPADS_MARKDOWN_HIGHLIGHT_COMPLETE,
                        L"YAML aliases must use language6 and real complete tokens.");
                CheckText(snapshot, main, rows[index], yaml);
                Require(At(snapshot, rows[index], 2).role == NOTEPADS_MARKDOWN_HIGHLIGHT_COMMENT &&
                            At(snapshot, rows[index], static_cast<uint32_t>(yaml.find(L"42"))).role == NOTEPADS_MARKDOWN_HIGHLIGHT_NUMBER,
                        L"YAML comments and numbers must have real roles with Unicode bounds.");
            }
            constexpr std::wstring_view shell =
                L"# 中😀\nprintf '%s\\n' 'Markdown literal * * *'\nif true; then\n    echo \"中😀 $value\"\nfi\n";
            auto shellTokens = snapshot.Spans(rows[13]);
            for (size_t index = 13; index < 16; ++index)
            {
                Require(rows[index].language == 7 && rows[index].state == NOTEPADS_MARKDOWN_HIGHLIGHT_COMPLETE,
                        L"Shell aliases must use language7 and complete Bash tokens.");
                CheckText(snapshot, main, rows[index], shell);
                Require(At(snapshot, rows[index], 2).role == NOTEPADS_MARKDOWN_HIGHLIGHT_COMMENT &&
                            At(snapshot, rows[index], static_cast<uint32_t>(shell.find(L"printf"))).role == NOTEPADS_MARKDOWN_HIGHLIGHT_FUNCTION &&
                            At(snapshot, rows[index], static_cast<uint32_t>(shell.find(L"Markdown literal"))).role == NOTEPADS_MARKDOWN_HIGHLIGHT_STRING &&
                            At(snapshot, rows[index], static_cast<uint32_t>(shell.find(L"if true"))).role == NOTEPADS_MARKDOWN_HIGHLIGHT_KEYWORD,
                        L"Shell comments, builtins, quoted literals and control keywords have real roles.");
                auto tokens = snapshot.Spans(rows[index]);
                Require(tokens.size() == shellTokens.size(), L"Shell alias token counts differ.");
                for (size_t token = 0; token < tokens.size(); ++token)
                    Require(std::memcmp(&tokens[token], &shellTokens[token], sizeof(notepads_markdown_highlight_span)) == 0,
                            L"Shell alias tokens differ, including Unicode boundaries.");
            }
            struct Fence
            {
                uint32_t language;
                std::wstring_view code;
                std::wstring_view token;
                uint32_t role;
            };
            constexpr Fence additional[] = {
                {8, L"// 中😀\n", L"中", NOTEPADS_MARKDOWN_HIGHLIGHT_COMMENT},
                {9, L"<strong>中😀</strong>\n", L"strong", NOTEPADS_MARKDOWN_HIGHLIGHT_TYPE},
                {10, L".x { width: 42px; }\n", L"42", NOTEPADS_MARKDOWN_HIGHLIGHT_NUMBER},
                {11, L"SELECT '中😀';\n", L"SELECT", NOTEPADS_MARKDOWN_HIGHLIGHT_KEYWORD},
                {12, L"-旧😀\n+新😀\n", L"-旧", NOTEPADS_MARKDOWN_HIGHLIGHT_REMOVED},
                {13, L"func Sample() {}\n", L"Sample", NOTEPADS_MARKDOWN_HIGHLIGHT_FUNCTION},
                {14, L"public class Sample {}\n", L"Sample", NOTEPADS_MARKDOWN_HIGHLIGHT_TYPE},
                {38, L"// 中文 😀\ndigraph { start -> end [label=\"示例\"]; }\n", L"示例", NOTEPADS_MARKDOWN_HIGHLIGHT_STRING},
                {39, L"% 中文 😀\nx = 42;\ndisp('示例');\n", L"42", NOTEPADS_MARKDOWN_HIGHLIGHT_NUMBER},
                {40, L"{ 中文 😀 }\nbegin\n  WriteLn('示例');\nend.\n", L"示例", NOTEPADS_MARKDOWN_HIGHLIGHT_STRING},
                {41, L".. note::\n\n   Literal: ``示例``\n", L"示例", NOTEPADS_MARKDOWN_HIGHLIGHT_STRING},
                {42, L"-- 中文 😀\nset labelText to \"示例\"\nreturn labelText\n",
                     L"示例", NOTEPADS_MARKDOWN_HIGHLIGHT_STRING},
                {43, L"/+ 中文 😀 +/\nimmutable string label = \"示例\";\n",
                     L"中文", NOTEPADS_MARKDOWN_HIGHLIGHT_COMMENT},
                {44, L"/ 中文 😀\n%section.panel\n  %p= \"示例\"\n",
                     L"示例", NOTEPADS_MARKDOWN_HIGHLIGHT_STRING},
                {45, L"<%# 中文 😀 %>\n<strong><%= \"示例\" %></strong>\n",
                     L"中文", NOTEPADS_MARKDOWN_HIGHLIGHT_COMMENT},
                {3, L"{\n  // 中文 😀\n  \"count\": 42,\n}\n", L"中文", NOTEPADS_MARKDOWN_HIGHLIGHT_COMMENT},
                {7, L"# 中文 😀\nprintf '%s\\n' '示例'\n", L"printf", NOTEPADS_MARKDOWN_HIGHLIGHT_FUNCTION},
                {9, L"<!-- 中文 😀 -->\n<br title=\"示例\" />\n", L"br", NOTEPADS_MARKDOWN_HIGHLIGHT_TYPE},
                {29, L"<!-- 中文 😀 -->\n<section><?= label() ?></section>\n",
                     L"label", NOTEPADS_MARKDOWN_HIGHLIGHT_FUNCTION},
                {46, L"interface User { name: string }\n", L"interface", NOTEPADS_MARKDOWN_HIGHLIGHT_TYPE},
                {47, L"const node = <section title={name}>你好 {name}</section>;\n",
                     L"title", NOTEPADS_MARKDOWN_HIGHLIGHT_KEYWORD},
                {48, L"# 参数示例\nWrite-Output \"你好\"\n", L"参数", NOTEPADS_MARKDOWN_HIGHLIGHT_COMMENT},
                {49, L"[project]\nenabled = true\n", L"true", NOTEPADS_MARKDOWN_HIGHLIGHT_KEYWORD},
                {50, L"FROM alpine:3.22 AS build\nRUN echo \"ready\"\n", L"FROM", NOTEPADS_MARKDOWN_HIGHLIGHT_KEYWORD},
            };
            for (size_t index = 0; index < std::size(additional); ++index)
            {
                auto const &fence = additional[index];
                auto const &row = rows[16 + index];
                CheckText(snapshot, main, row, fence.code);
                Require(row.language == fence.language && row.state == NOTEPADS_MARKDOWN_HIGHLIGHT_COMPLETE &&
                            At(snapshot, row, static_cast<uint32_t>(fence.code.find(fence.token))).role == fence.role,
                        L"The new fence must publish its real language and token role through the native snapshot.");
            }
            Require(At(snapshot, rows[20], 5).role == NOTEPADS_MARKDOWN_HIGHLIGHT_ADDED,
                    L"An inserted Diff line must have a distinct native role.");
            auto foreignCopy = rows[0];
            ExpectStatus(NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT, [&] { (void)snapshot.Spans(foreignCopy); });
        }

        void CheckSkipped(NativeHighlightSnapshot const &snapshot, MainTextSnapshot const &main)
        {
            auto const &summary = snapshot.Summary();
            Require(summary.blocks == 2 && summary.complete_blocks == 0 && summary.skipped_blocks == 2 &&
                        summary.failed_blocks == 0 && summary.spans == 0,
                    L"Unknown and overlong line have explicit skipped states; " + Describe(summary));
            auto const rows = snapshot.Blocks();
            Require(rows[0].state == NOTEPADS_MARKDOWN_HIGHLIGHT_SKIPPED && rows[0].reason == 1 && rows[0].language == 0 &&
                        rows[1].state == NOTEPADS_MARKDOWN_HIGHLIGHT_SKIPPED && rows[1].reason == 4 && rows[1].language == 2 &&
                        snapshot.Spans(rows[0]).empty() && snapshot.Spans(rows[1]).empty(),
                    L"UnknownLanguage and LineBytes must remain distinct, without partial token spans.");
            CheckText(snapshot, main, rows[0], L"plain😀\n");
            auto longLine = std::wstring(8193, L'x') + L"\n";
            CheckText(snapshot, main, rows[1], longLine);
        }

        struct Frozen
        {
            notepads_markdown_highlight_summary summary;
            std::vector<notepads_markdown_highlight_block> blocks;
            std::vector<notepads_markdown_highlight_span> spans;
            explicit Frozen(NativeHighlightSnapshot const &snapshot) : summary(snapshot.Summary())
            {
                auto rows = snapshot.Blocks();
                blocks.assign(rows.begin(), rows.end());
                for (auto const &row : rows)
                {
                    auto tokens = snapshot.Spans(row);
                    spans.insert(spans.end(), tokens.begin(), tokens.end());
                }
            }
            void Check(NativeHighlightSnapshot const &snapshot) const
            {
                Require(std::memcmp(&summary, &snapshot.Summary(), sizeof(summary)) == 0,
                        L"Retired-source summary changed.");
                auto rows = snapshot.Blocks();
                Require(rows.size() == blocks.size(), L"Retired-source row count changed.");
                size_t position{};
                for (size_t index = 0; index < rows.size(); ++index)
                {
                    Require(std::memcmp(&blocks[index], &rows[index], sizeof(notepads_markdown_highlight_block)) == 0,
                            L"Retired-source CodeBlock changed.");
                    auto tokens = snapshot.Spans(rows[index]);
                    Require(tokens.size() <= spans.size() - position, L"Retired-source token count changed.");
                    for (auto const &token : tokens)
                    {
                        Require(std::memcmp(&spans[position++], &token, sizeof(token)) == 0,
                                L"Retired-source token changed.");
                    }
                }
                Require(position == spans.size(), L"Retired-source token partition changed.");
            }
        };

        void SetFailure(CoreValidation::CheckResult &check, std::exception_ptr failure)
        {
            try
            {
                std::rethrow_exception(failure);
            }
            catch (AssertionFailure const &error)
            {
                check.status = error.status;
                check.detail = error.detail;
            }
            catch (NativeFailure const &error)
            {
                check.status = error.status;
                check.detail = L"Highlight/model native status=" + std::to_wstring(error.status) +
                               L", mapped HRESULT=" + std::to_wstring(error.code().value);
            }
            catch (winrt::hresult_error const &error)
            {
                check.status = error.code().value;
                check.detail = error.message().c_str();
            }
            catch (...)
            {
                check.status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                check.detail = L"Unexpected highlight diagnostic exception.";
            }
        }

        winrt::Windows::Foundation::IAsyncAction RunCase(winrt::Windows::UI::Core::CoreDispatcher dispatcher,
                                                         std::shared_ptr<CoreControllerTests::Results> results,
                                                         std::shared_ptr<CoreValidation::Context> context, Case kind)
        {
            constexpr std::array names{L"highlight_snapshot_json_rust_unicode", L"highlight_snapshot_explicit_skips",
                                       L"highlight_snapshot_retired_source_owners"};
            auto const name = names[static_cast<size_t>(kind)];
            auto source = Source(kind);
            auto controller = CoreController::CreatePreview(dispatcher);
            auto lifetime = controller->Lifetime();
            CoreCompletionTicket ticket;
            CoreModelPin pin;
            notepads_markdown_core_cancel *sourceFlag{}, *foreignFlag{};
            std::shared_ptr<ReadingMemory> memory;
            std::shared_ptr<MainTextSnapshot const> main;
            std::shared_ptr<NativeHighlightSnapshot const> snapshot;
            std::unique_ptr<Frozen> frozen;
            bool registered{}, sourceRegistered{};
            std::exception_ptr failure;
            try
            {
                context->PublishController(controller.get());
                registered = true;
                auto descriptor = Pod<notepads_markdown_core_snapshot>();
                descriptor.source.document[0] = 0x48;
                descriptor.source.document[1] = static_cast<uint8_t>(kind);
                descriptor.source.document[15] = 0x31;
                descriptor.source.revision = 1;
                descriptor.profile = NOTEPADS_MARKDOWN_CORE_G3;
                Check(controller
                          ->SubmitUtf8({reinterpret_cast<uint8_t const *>(source.data()), source.size()}, descriptor,
                                       &ticket)
                          .status);
                co_await winrt::resume_background();
                Require(Until(
                            [&]
                            {
                                context->Arbitrate();
                                return ticket.Done();
                            }),
                        L"Real model parse timed out.");
                Check(ticket.Status());
                co_await winrt::resume_foreground(dispatcher);
                pin = controller->PinInstalled();
                co_await winrt::resume_background();
                Require(static_cast<bool>(pin), L"The actual UI-installed G3 model was not pinned.");
                auto summary = Pod<notepads_markdown_core_model_summary_result>();
                auto error = Pod<notepads_markdown_core_error>();
                Check(notepads_markdown_core_model_summary(pin.Get(), &summary, &error));
                Check(notepads_markdown_core_model_cancel_create(pin.Get(), &sourceFlag, &error));
                context->Publish(sourceFlag);
                sourceRegistered = true;
                memory = ReadingMemory::Create(std::move(pin), sourceFlag, summary.identity);
                main = MainTextSnapshot::Build(memory, sourceFlag, 31, NextReadingTextToken());
                std::wstring const original{main->Text()};
                snapshot = NativeHighlightSnapshot::Build(memory, sourceFlag);
                if (kind == Case::Skipped)
                    CheckSkipped(*snapshot, *main);
                else
                    CheckUnicode(*snapshot, *main);
                Require(main->Text() == original && snapshot->Memory() == memory,
                        L"Highlighting may not alter Main text or create a separate model root.");
                frozen = std::make_unique<Frozen>(*snapshot);
                if (kind == Case::Ownership)
                {
                    Check(notepads_markdown_preview_cancel_create(&foreignFlag, &error));
                    ExpectStatus(NOTEPADS_MARKDOWN_CORE_WRONG_BUDGET,
                                 [&] { (void)NativeHighlightSnapshot::Build(memory, foreignFlag); });
                    Check(notepads_markdown_core_cancel_request(sourceFlag));
                    ExpectStatus(NOTEPADS_MARKDOWN_CORE_CANCELLED, [&] { (void)NativeHighlightSnapshot::Build(memory, sourceFlag); });
                    frozen->Check(*snapshot);
                    Require(main->Text() == original, L"Rejected highlight work changed existing Main text.");
                }
            }
            catch (...)
            {
                failure = std::current_exception();
            }

            if (sourceRegistered)
                context->Withdraw(sourceFlag);
            if (registered)
                context->WithdrawController(controller.get());
            controller->Stop();
            co_await winrt::resume_background();
            context->RecordCleanup(notepads_markdown_core_cancel_destroy(sourceFlag), name);
            context->RecordCleanup(notepads_markdown_core_cancel_destroy(foreignFlag), name);
            sourceFlag = foreignFlag = nullptr;
            pin.Reset();
            main.reset();
            memory.reset();
            auto drain = controller->Drain();
            controller.reset();
            ticket = {};
            if (!failure)
            {
                try
                {
                    Require(snapshot && frozen, L"No immutable snapshot survived cleanup.");
                    frozen->Check(*snapshot);
                    Require(lifetime.Snapshot().modelPins == 1 && !lifetime.Snapshot().fullyReleased,
                            L"Only the immutable highlight owner must retain its native model pin.");
                    std::weak_ptr<ReadingMemory> weakMemory = snapshot->Memory();
                    std::weak_ptr<NativeHighlightSnapshot const> weakSnapshot = snapshot;
                    snapshot.reset();
                    Require(weakSnapshot.expired() && !weakMemory.expired(),
                            L"The funded weak snapshot control block still owns its ReadingAllocator root.");
                    weakSnapshot.reset();
                    Require(weakMemory.expired() && lifetime.Snapshot().modelPins == 0,
                            L"The last funded snapshot control block did not release its model owner.");
                    weakMemory.reset();
                }
                catch (...)
                {
                    failure = std::current_exception();
                }
            }
            snapshot.reset();
            bool const released = Until([&] { return lifetime.Snapshot().fullyReleased; });
            auto final = lifetime.Snapshot();
            results->lastDrain = drain;
            results->lastLifetime = final;
            CoreValidation::CheckResult check{
                name,
                !failure && released && drain.coreDrained && drain.driverExited && drain.primaryStatus == NOTEPADS_MARKDOWN_CORE_OK &&
                    drain.cleanupStatus == NOTEPADS_MARKDOWN_CORE_OK && drain.primaryHresult == 0 && drain.cleanupHresult == 0 &&
                    final.cleanupStatus == NOTEPADS_MARKDOWN_CORE_OK && final.cleanupHresult == 0,
                NOTEPADS_MARKDOWN_CORE_OK,
                L"Real G3 on a worker; one whole Rust token batch, fixed JSON/Rust UTF16/scalar-role oracle "
                L"or explicit unknown/overlong-line skips, unchanged Main text, and immutable reads after source/"
                L"controller retirement. Native capacities use model-root HeapAlloc reserve/admit; native pin and "
                L"last-owner observations do not measure Rust auxiliary totals, codec/driver memory or process RSS."};
            if (failure)
                SetFailure(check, failure);
            else if (!check.passed)
            {
                check.status = NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
                check.detail = L"Highlight teardown failed: released=" + std::to_wstring(released) + L", pins=" +
                               std::to_wstring(final.modelPins) + L", primary=" + std::to_wstring(drain.primaryStatus) +
                               L", cleanup=" + std::to_wstring(drain.cleanupStatus);
            }
            if (!check.passed)
                context->RecordPrimary(CoreValidation::FaultKind::Assertion, check.status, name);
            results->checks.push_back(std::move(check));
            context->RecordCleanup(drain.cleanupStatus, name);
            context->RecordCleanup(drain.cleanupHresult, name);
            context->RecordCleanup(final.cleanupStatus, name);
            context->RecordCleanup(final.cleanupHresult, name);
            if (!released)
                context->RecordCleanupInvariant(name);
            context->Arbitrate();
            co_await winrt::resume_foreground(dispatcher);
        }
    }

    winrt::Windows::Foundation::IAsyncAction RunAsync(winrt::Windows::UI::Core::CoreDispatcher dispatcher,
                                                      std::shared_ptr<CoreControllerTests::Results> results,
                                                      std::shared_ptr<CoreValidation::Context> context)
    {
        if (!dispatcher || !dispatcher.HasThreadAccess() || !results || !context)
            throw winrt::hresult_invalid_argument();
        results->checks.reserve(results->checks.size() + 3);
        for (auto kind : {Case::Unicode, Case::Skipped, Case::Ownership})
        {
            context->Arbitrate();
            co_await RunCase(dispatcher, results, context, kind);
        }
    }
}
