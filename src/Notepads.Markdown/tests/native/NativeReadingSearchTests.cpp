// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeReadingSearchTests.h"
#include "Reading/NativeReadingSearch.h"
#include "Reading/NativeReadingSnapshot.h"
#include <algorithm>
#include <array>
#include <exception>
#include <limits>
#include <vector>

namespace Notepads::Markdown::Native::ReadingSearchTests
{
    namespace
    {
        void Require(bool value, wchar_t const *detail)
        {
            if (!value)
                throw winrt::hresult_error(E_FAIL, detail);
        }

        template <typename F> void ExpectHresult(HRESULT expected, F &&body)
        {
            try
            {
                body();
            }
            catch (winrt::hresult_error const &failure)
            {
                Require(failure.code().value == expected, L"search exact expected HRESULT");
                return;
            }
            throw winrt::hresult_error(E_FAIL, L"search expected failure was not raised");
        }

        template <typename T> T Pod() noexcept
        {
            T result{};
            result.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(T)};
            return result;
        }

        // Independent scalar predicate and old per-position CompareStringOrdinal
        // algorithm. Never calls the NLS window implementation or its validator.
        bool ScalarBoundary(std::wstring_view text, size_t offset) noexcept
        {
            if (offset > text.size())
                return false;
            if (offset == 0 || offset == text.size())
                return true;
            return !(text[offset - 1] >= 0xD800 && text[offset - 1] < 0xDC00 && text[offset] >= 0xDC00 &&
                     text[offset] < 0xE000);
        }

        ReadingSearchResult Oracle(std::wstring_view text, std::wstring_view query, uint32_t start,
                                   uint32_t length, bool backward, bool ignoreCase)
        {
            auto first = static_cast<int64_t>(start);
            auto last = first + length - static_cast<int64_t>(query.size());
            for (auto position = backward ? last : first; position >= first && position <= last;
                 position += backward ? -1 : 1)
            {
                if (!ScalarBoundary(text, static_cast<size_t>(position)) ||
                    !ScalarBoundary(text, static_cast<size_t>(position) + query.size()))
                    continue;
                ::SetLastError(ERROR_SUCCESS);
                auto compared = ::CompareStringOrdinal(text.data() + position, static_cast<int>(query.size()),
                                                       query.data(), static_cast<int>(query.size()),
                                                       ignoreCase ? TRUE : FALSE);
                if (!compared)
                    throw winrt::hresult_error(HRESULT_FROM_WIN32(::GetLastError()));
                if (compared == CSTR_EQUAL)
                    return {true, static_cast<uint32_t>(position), static_cast<uint32_t>(query.size())};
            }
            return {};
        }

        void Compare(std::wstring_view text, std::wstring_view query, uint32_t start, uint32_t length,
                     Results &results)
        {
            for (bool backward : {false, true})
                for (bool ignoreCase : {false, true})
                {
                    auto expected = Oracle(text, query, start, length, backward, ignoreCase);
                    auto actual = FindReadingText(text, query, start, length, backward, ignoreCase);
                    Require(actual.found == expected.found && actual.start == expected.start &&
                                actual.length == expected.length,
                            L"window search differs from independent linear ordinal oracle");
                    ++results.comparisons;
                }
        }

        void CheckSmallCorpus(Results &results)
        {
            std::array texts{std::wstring{}, std::wstring{L"aaaaa"}, std::wstring{L"abababa"},
                             std::wstring{L"中😀e\u0301 שלום"}, std::wstring{L"AaİıiIßẞΣσςKk"},
                             std::wstring{L"\r\n\t&\xFFFC"}, std::wstring{L"a\0Aa\0a", 6},
                             std::wstring{L"𐐀𐐨👩🏽‍💻"}};
            std::array queries{std::wstring{L"a"}, std::wstring{L"A"}, std::wstring{L"aa"}, std::wstring{L"aba"},
                               std::wstring{L"i"}, std::wstring{L"σ"}, std::wstring{L"k"}, std::wstring{L"中"},
                               std::wstring{L"😀"}, std::wstring{L"e\u0301"}, std::wstring{L"é"},
                               std::wstring{L"👩🏽‍💻"}, std::wstring{L"𐐨"}, std::wstring{L"\t"},
                               std::wstring{L"\n"}, std::wstring{L"\xFFFC"}, std::wstring{L"\0", 1},
                               std::wstring{L"a\0A", 3}, std::wstring(1, wchar_t{0xD83D}),
                               std::wstring(1, wchar_t{0xDE00}), std::wstring{L"absent"}};
            for (auto const &text : texts)
                for (uint32_t start = 0; start <= text.size(); ++start)
                {
                    if (!ScalarBoundary(text, start))
                        continue;
                    for (uint32_t end = start; end <= text.size(); ++end)
                    {
                        if (!ScalarBoundary(text, end))
                            continue;
                        for (auto const &query : queries)
                            Compare(text, query, start, end - start, results);
                    }
                }
        }

        uint32_t Random(uint32_t &state) noexcept
        {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            return state;
        }

        void CheckGeneratedCorpus(Results &results)
        {
            constexpr std::array atoms{std::wstring_view{L"a"}, std::wstring_view{L"A"}, std::wstring_view{L"b"},
                                       std::wstring_view{L"中"}, std::wstring_view{L"😀"}, std::wstring_view{L"e\u0301"},
                                       std::wstring_view{L"\0", 1}, std::wstring_view{L"\xFFFC"},
                                       std::wstring_view{L"\t"}, std::wstring_view{L"\n"}, std::wstring_view{L"𐐀"}};
            uint32_t random = 0x793D153B;
            for (uint32_t iteration = 0; iteration < 512; ++iteration)
            {
                std::wstring text;
                std::vector<uint32_t> boundaries{0};
                for (uint32_t index = 0; index < 24; ++index)
                {
                    text += atoms[Random(random) % atoms.size()];
                    boundaries.push_back(static_cast<uint32_t>(text.size()));
                }
                auto left = Random(random) % boundaries.size();
                auto right = Random(random) % boundaries.size();
                if (left > right)
                    std::swap(left, right);
                auto queryFirst = Random(random) % (boundaries.size() - 1);
                auto queryEnd = queryFirst + 1 + Random(random) % (boundaries.size() - queryFirst - 1);
                auto query = std::wstring_view{text}.substr(boundaries[queryFirst],
                                                          boundaries[queryEnd] - boundaries[queryFirst]);
                Compare(text, query, boundaries[left], boundaries[right] - boundaries[left], results);
                Compare(text, atoms[Random(random) % atoms.size()], 0, static_cast<uint32_t>(text.size()), results);
            }
        }

        void CheckWindowBoundaries(Results &results)
        {
            for (uint32_t queryLength : {1u, 2u, 3u, 4096u, 4097u, 8193u, 16385u})
            {
                std::wstring query(queryLength, L'q');
                query[0] = L'Z';
                Require(IsValidReadingSearchQuery(query), L"helper preserves valid old UIA query beyond 4096");
                for (uint32_t position : {8191u, 8192u, 8193u, 16383u, 16384u, 16385u})
                {
                    std::wstring text(queryLength + ReadingSearchWindowCandidates * 3, L'x');
                    text.replace(position, queryLength, query);
                    Compare(text, query, 0, static_cast<uint32_t>(text.size()), results);
                    Compare(text, query, position, queryLength, results);
                    if (queryLength > 1)
                        Compare(text, query, position, queryLength - 1, results);
                }
            }
            std::wstring overlaps(ReadingSearchWindowCandidates * 3 + 2, L'a');
            Compare(overlaps, L"aaa", 0, static_cast<uint32_t>(overlaps.size()), results);
            Compare(overlaps, L"aaa", 8191, 8193, results);
            for (uint32_t position : {8191u, 8192u, 8193u})
            {
                std::wstring text(ReadingSearchWindowCandidates * 3, L'x');
                text.replace(position, 3, L"😀中");
                Compare(text, L"😀中", 0, static_cast<uint32_t>(text.size()), results);
                Compare(text, std::wstring(1, wchar_t{0xD83D}), 0, static_cast<uint32_t>(text.size()), results);
            }
            ::SetLastError(ERROR_ACCESS_DENIED);
            Require(!FindReadingText(L"text", L"missing", 0, 4, false, false).found,
                    L"stale Win32 last error does not turn no-candidate search into failure");
            ::SetLastError(ERROR_ACCESS_DENIED);
            Require(!FindReadingText(L"text", L"z", 0, 4, false, false).found,
                    L"stale Win32 last error is cleared before an actual NLS call");
        }

        struct StopCheckpoint
        {
            ReadingSearchOperation operation;
            uint32_t calls{};
            uint32_t stopAt{};
            bool invalidate{};

            static void Check(void *context)
            {
                auto &state = *static_cast<StopCheckpoint *>(context);
                if (++state.calls == state.stopAt)
                {
                    if (state.invalidate)
                        state.operation.Invalidate();
                    else
                        state.operation.Cancel();
                }
                state.operation.Check();
            }
        };

        void CheckCheckpoints(std::wstring_view text, std::wstring_view query, bool backward)
        {
            StopCheckpoint reference;
            (void)FindReadingText(text, query, 0, static_cast<uint32_t>(text.size()), backward, false,
                                  StopCheckpoint::Check, &reference);
            Require(reference.calls >= 2, L"search checks stop on entry and before every return");
            for (bool invalidate : {false, true})
                for (uint32_t index = 1; index <= reference.calls; ++index)
                {
                    StopCheckpoint state;
                    state.stopAt = index;
                    state.invalidate = invalidate;
                    ExpectHresult(invalidate ? UIA_E_ELEMENTNOTAVAILABLE : HRESULT_FROM_WIN32(ERROR_CANCELLED),
                                  [&]
                                  {
                                      (void)FindReadingText(text, query, 0, static_cast<uint32_t>(text.size()), backward,
                                                            false, StopCheckpoint::Check, &state);
                                  });
                    Require(state.calls == index, L"search stop occurs at the requested deterministic checkpoint");
                }
        }

        void CheckValidationAndStops()
        {
            Require(!IsValidReadingSearchQuery({}) && !IsValidReadingSearchQuery(std::wstring(1, wchar_t{0xD800})) &&
                        !IsValidReadingSearchQuery(std::wstring(1, wchar_t{0xDFFF})) &&
                        !IsValidReadingSearchQuery(std::wstring{L"x\xD800"}) &&
                        IsValidReadingSearchQuery(std::wstring{L"\0", 1}) && IsValidReadingSearchQuery(L"😀"),
                    L"nonempty complete UTF16 query validation including embedded NUL");
            ExpectHresult(E_INVALIDARG, [] { (void)FindReadingText(L"😀", L"x", 1, 1, false, false); });
            ExpectHresult(E_INVALIDARG, [] { (void)FindReadingText(L"😀", L"x", 0, 1, false, false); });
            ExpectHresult(E_INVALIDARG, [] { (void)FindReadingText(L"a", L"x", 2, 0, false, false); });
            ExpectHresult(E_INVALIDARG,
                          [] { (void)FindReadingText(L"a", L"x", std::numeric_limits<uint32_t>::max(), 1, false, false); });
            ExpectHresult(E_INVALIDARG, [] { (void)FindReadingText(L"a", L"", 0, 1, false, false); });
            for (bool backward : {false, true})
            {
                CheckCheckpoints(L"a", L"a", backward); // entry, before/after NLS, before successful return
                CheckCheckpoints(L"", L"x", backward);
                CheckCheckpoints(L"x", L"xx", backward);
                CheckCheckpoints(L"😀", std::wstring(1, wchar_t{0xD83D}), backward);
                CheckCheckpoints(std::wstring(ReadingSearchWindowCandidates * 2 + 1, L'a'), L"z", backward);
            }
        }

        void CheckGate()
        {
            ReadingSearchGate gate;
            std::array<std::shared_ptr<ReadingSearchOperation>, ReadingSearchMaximumInFlight> operations;
            for (auto &operation : operations)
                operation = gate.Acquire();
            Require(gate.ActiveCount() == ReadingSearchMaximumInFlight, L"four live search permits");
            ExpectHresult(HRESULT_FROM_WIN32(ERROR_BUSY), [&] { (void)gate.Acquire(); });
            operations[0]->Cancel();
            gate.Invalidate();
            ExpectHresult(HRESULT_FROM_WIN32(ERROR_CANCELLED), [&] { operations[0]->Check(); });
            for (size_t index = 1; index < operations.size(); ++index)
                ExpectHresult(UIA_E_ELEMENTNOTAVAILABLE, [&] { operations[index]->Check(); });
            operations[1]->Cancel();
            operations[1]->Invalidate();
            ExpectHresult(HRESULT_FROM_WIN32(ERROR_CANCELLED), [&] { operations[1]->Check(); });
            ExpectHresult(HRESULT_FROM_WIN32(ERROR_BUSY), [&] { (void)gate.Acquire(); });

            auto queuedOwner = operations[2];
            operations[2].reset();
            Require(gate.ActiveCount() == ReadingSearchMaximumInFlight,
                    L"queued owner retains its permit after caller cancellation/release");
            ExpectHresult(HRESULT_FROM_WIN32(ERROR_BUSY), [&] { (void)gate.Acquire(); });
            queuedOwner.reset();
            Require(gate.ActiveCount() == ReadingSearchMaximumInFlight - 1, L"last queued owner retires slot");
            operations[2] = gate.Acquire();
            operations[2]->Check(); // new operation did not inherit old invalidation
            operations = {};
            Require(gate.ActiveCount() == 0, L"all search permits retired");
            for (uint32_t index = 0; index < 16; ++index)
            {
                auto fresh = gate.Acquire();
                fresh->Check();
                Require(gate.ActiveCount() == 1, L"permit can be repeatedly reused after all exit categories");
            }
            Require(gate.ActiveCount() == 0, L"reused permit releases at last real owner");
        }

        struct CancelOwner
        {
            notepads_markdown_core_cancel *value{};
            ~CancelOwner()
            {
                (void)notepads_markdown_core_cancel_destroy(value);
            }
        };

        struct Reservation
        {
            notepads_markdown_preview_allocation *value{};
            ~Reservation()
            {
                (void)notepads_markdown_preview_allocation_destroy(value);
            }
            void Reset()
            {
                Require(notepads_markdown_preview_allocation_destroy(value) == NOTEPADS_MARKDOWN_CORE_OK, L"real root reservation cleanup");
                value = nullptr;
            }
        };

        uint64_t ReserveHeadroom(notepads_markdown_core_model *model, notepads_markdown_core_cancel *cancel)
        {
            // Each probe is an actual reservation in the supplied accepted model's
            // root and is destroyed before the next probe. No fake funding token,
            // huge heap allocation, reset of a fault, or new Rust observer is used.
            constexpr uint64_t previewAuxiliaryLimit = 256ull * 1024 * 1024;
            uint64_t low{}, high = previewAuxiliaryLimit;
            while (low < high)
            {
                auto candidate = low + (high - low + 1) / 2;
                Reservation probe;
                auto error = Pod<notepads_markdown_core_error>();
                auto status = notepads_markdown_preview_model_reserve(model, cancel, candidate, &probe.value, &error);
                if (status == NOTEPADS_MARKDOWN_CORE_OK)
                {
                    Require(probe.value != nullptr, L"headroom probe owns a real reservation");
                    probe.Reset();
                    low = candidate;
                }
                else
                {
                    Require(status == NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT && error.status == status &&
                                error.limit_kind == NOTEPADS_MARKDOWN_CORE_AUXILIARY_CAPACITY && !probe.value,
                            L"headroom probe only treats actual auxiliary quota as capacity exhaustion");
                    high = candidate - 1;
                }
            }
            return low;
        }

        struct FundedRetirementObservation
        {
            ReadingSearchGate const &gate;
            std::shared_ptr<MainTextSnapshot const> const &main;
            notepads_markdown_core_cancel *flag;
            uint64_t headroom;
            long mainReferences;
            long memoryReferences;
            bool observed{};
            bool payloadRetiredWithPermit{};
            std::exception_ptr failure;

            void Observe() noexcept
            {
                observed = true;
                try
                {
                    payloadRetiredWithPermit = gate.ActiveCount() == 1 && main.use_count() == mainReferences &&
                                               main->Memory().use_count() == memoryReferences &&
                                               ReserveHeadroom(main->ModelOwner().Get(), flag) == headroom;
                }
                catch (...)
                {
                    failure = std::current_exception();
                }
            }
        };

        struct FundedRetirementObserver
        {
            FundedRetirementObservation *observation{};

            ~FundedRetirementObserver()
            {
                if (observation)
                    observation->Observe();
            }
        };

        struct FundedSearchJob
        {
            // Reverse member destruction retires paid query/Main before the
            // observer, then releases the permit only after that observation.
            std::shared_ptr<ReadingSearchOperation> operation;
            FundedRetirementObserver retirement;
            std::shared_ptr<MainTextSnapshot const> main;
            ReadingString query;

            FundedSearchJob(std::shared_ptr<MainTextSnapshot const> snapshot,
                            std::shared_ptr<ReadingSearchOperation> permit,
                            FundedRetirementObservation *observation = nullptr)
                : operation(std::move(permit)), retirement{observation}, main(std::move(snapshot)),
                  query(ReadingAllocator<wchar_t>{main->Memory()})
            {
            }
        };

        void CheckFunding(std::shared_ptr<MainTextSnapshot const> const &main, Results &results)
        {
            Require(main && main->Memory() && main->ModelOwner(), L"search test borrows an actual model/Main root");
            auto model = main->ModelOwner().Get();
            CancelOwner flag;
            auto error = Pod<notepads_markdown_core_error>();
            Require(notepads_markdown_core_model_cancel_create(model, &flag.value, &error) == NOTEPADS_MARKDOWN_CORE_OK,
                    L"search budget observation owns a real independent same-root flag");
            auto headroom = ReserveHeadroom(model, flag.value);
            Require(headroom > ReadingSearchMaximumQueryLength * sizeof(wchar_t), L"actual Inline model has query room");
            ReadingSearchGate gate;
            auto mainReferences = main.use_count();
            auto memoryReferences = main->Memory().use_count();
            FundedRetirementObservation retirement{gate, main, flag.value, headroom, mainReferences, memoryReferences};
            {
                auto job = std::make_shared<FundedSearchJob>(main, gate.Acquire(), &retirement);
                job->query.assign(ReadingSearchMaximumQueryLength, L'q');
            }
            if (retirement.failure)
                std::rethrow_exception(retirement.failure);
            Require(retirement.observed && retirement.payloadRetiredWithPermit && gate.ActiveCount() == 0,
                    L"fixture member destruction restores real query/Main funding before retiring its permit");
            {
                std::array<std::shared_ptr<FundedSearchJob>, ReadingSearchMaximumInFlight> jobs;
                uint64_t payloadCapacity{};
                for (auto &job : jobs)
                {
                    job = std::make_shared<FundedSearchJob>(main, gate.Acquire());
                    job->query.assign(ReadingSearchMaximumQueryLength, L'q');
                    payloadCapacity += (job->query.capacity() + 1) * sizeof(wchar_t);
                }
                auto occupied = ReserveHeadroom(model, flag.value);
                Require(headroom > occupied && headroom - occupied >= payloadCapacity,
                        L"real root charges owned query capacities, terminators and native allocation overhead");
                results.observedQueryCharge = headroom - occupied;
                Require(main.use_count() == mainReferences + static_cast<long>(ReadingSearchMaximumInFlight),
                        L"four real jobs pin the actual immutable Main");
                ExpectHresult(HRESULT_FROM_WIN32(ERROR_BUSY), [&] { (void)gate.Acquire(); });
                auto queued = jobs[0];
                jobs[0]->operation->Cancel();
                jobs[0].reset();
                Require(gate.ActiveCount() == ReadingSearchMaximumInFlight,
                        L"canceled job still has paid query/Main and permit while a completion owner survives");
                queued.reset();
                Require(gate.ActiveCount() == ReadingSearchMaximumInFlight - 1, L"completion retirement releases permit");
                jobs = {};
            }
            Require(gate.ActiveCount() == 0 && main.use_count() == mainReferences &&
                        main->Memory().use_count() == memoryReferences && ReserveHeadroom(model, flag.value) == headroom,
                    L"last job/completion release restores actual root capacity and Main/memory references");
            results.queryCapacityReleased = true;

            Reservation occupied;
            Require(notepads_markdown_preview_model_reserve(model, flag.value, headroom, &occupied.value, &error) == NOTEPADS_MARKDOWN_CORE_OK,
                    L"real root headroom reservation creates deterministic query quota failure");
            {
                auto failedJob = std::make_shared<FundedSearchJob>(main, gate.Acquire());
                ExpectHresult(HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA),
                              [&] { failedJob->query.assign(ReadingSearchMaximumQueryLength, L'x'); });
                Require(failedJob->query.empty(), L"failed query capacity did not publish partial query");
            }
            Require(gate.ActiveCount() == 0, L"query funding rejection does not strand its permit");
            occupied.Reset();
            {
                auto recovered = std::make_shared<FundedSearchJob>(main, gate.Acquire());
                recovered->query.assign(ReadingSearchMaximumQueryLength, L'x');
                recovered->operation->Check();
                Require(recovered->query.size() == ReadingSearchMaximumQueryLength,
                        L"query allocation succeeds after real funding reservation retires");
            }
            Require(gate.ActiveCount() == 0 && main.use_count() == mainReferences &&
                        main->Memory().use_count() == memoryReferences && ReserveHeadroom(model, flag.value) == headroom,
                    L"quota failure and successful reuse restore exact real root headroom and owners");
            results.queryFundingFailure = true;
        }
    }

    Results Run(std::shared_ptr<MainTextSnapshot const> const &main)
    {
        Results results;
        CheckSmallCorpus(results);
        CheckGeneratedCorpus(results);
        CheckWindowBoundaries(results);
        CheckValidationAndStops();
        CheckGate();
        results.permitsAndStops = true;
        Require(main && main->Length(), L"search oracle uses supplied actual Inline Main");
        for (std::wstring_view query : {L"中", L"&", L"😀", L"e\u0301", L"👩🏽‍💻", L"שלום", L"label", L"\xFFFC", L"&amp;",
                                       L"![alt]", L"absent"})
            Compare(main->Text(), query, 0, main->Length(), results);
        CheckFunding(main, results);
        results.passed = results.permitsAndStops && results.queryFundingFailure && results.queryCapacityReleased &&
                         results.comparisons > 10000;
        results.detail = L"Independent linear ordinal oracle comparisons=" + std::to_wstring(results.comparisons) +
                         L"; actual four-query root charge=" + std::to_wstring(results.observedQueryCharge) +
                         L" bytes; scalar/NUL/Unicode/window/old-large-query, deterministic stops, permits and real "
                         L"quota/release checked. Async retained-operation/UI publication lifetime is a host-suite gate.";
        return results;
    }
}
