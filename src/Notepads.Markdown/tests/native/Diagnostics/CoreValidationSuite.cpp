// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Diagnostics/CoreValidationSuite.h"
#include "Core/CoreController.h"
#include "notepads/markdown/platform_validation.h"
#include <cstring>
#include <initializer_list>
#include <span>
#include <string_view>
#include <type_traits>
#include <utility>

namespace Notepads::Markdown::Native::CoreValidation
{
    namespace
    {
        std::atomic<uint32_t> liveWorkers{};
        std::atomic<uint32_t> liveHandles{};

        struct CheckFailure
        {
            FaultKind kind;
            int32_t status;
            wchar_t const *operation;
        };

        struct ArbitrationFailure : winrt::hresult_error
        {
            explicit ArbitrationFailure(std::wstring const &detail) : hresult_error(E_FAIL, detail)
            {
            }
        };

        struct Audit
        {
            Context &context;
            int32_t status{NOTEPADS_MARKDOWN_CORE_OK};

            explicit Audit(Context &request) noexcept : context(request)
            {
            }

            void Require(bool condition, wchar_t const *detail) const
            {
                if (!condition)
                {
                    context.RecordPrimary(FaultKind::Assertion, status, detail);
                    throw CheckFailure{FaultKind::Assertion, status, detail};
                }
            }

            void Call(wchar_t const *operation, int32_t actual, notepads_markdown_core_error const &error,
                      int32_t expected = NOTEPADS_MARKDOWN_CORE_OK, notepads_markdown_core_cancel const *origin = nullptr)
            {
                status = actual;
                if (actual != expected)
                {
                    if (actual == NOTEPADS_MARKDOWN_CORE_CANCELLED && error.status == actual &&
                        context.CancellationRequested() && context.IsPublishedOrigin(origin))
                    {
                        throw CheckFailure{FaultKind::UnexpectedStatus, actual, operation};
                    }
                    context.RecordPrimary(FaultKind::UnexpectedStatus, actual, operation);
                    throw CheckFailure{FaultKind::UnexpectedStatus, actual, operation};
                }
                if (actual != NOTEPADS_MARKDOWN_CORE_OK)
                {
                    Require(error.status == actual, L"typed error matches the actual ABI return");
                }
            }

            void ExpectStatus(wchar_t const *operation, int32_t actual, int32_t expected = NOTEPADS_MARKDOWN_OK)
            {
                status = actual;
                if (actual != expected)
                {
                    context.RecordPrimary(FaultKind::UnexpectedStatus, actual, operation);
                    throw CheckFailure{FaultKind::UnexpectedStatus, actual, operation};
                }
            }
        };

        template <typename T> T Pod() noexcept
        {
            T value{};
            value.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, static_cast<uint32_t>(sizeof(T))};
            return value;
        }

        template <typename T, auto Destroy> class Handle
        {
        public:
            explicit Handle(Context &context, notepads_markdown_core_cancel const *origin = nullptr) noexcept
                : _context(context), _origin(origin)
            {
            }
            Handle(Handle const &) = delete;
            Handle &operator=(Handle const &) = delete;
            Handle(Handle &&other) noexcept
                : _context(other._context), _value(std::exchange(other._value, nullptr)), _origin(other._origin)
            {
            }
            ~Handle()
            {
                Reset();
            }
            T *Get() const noexcept
            {
                return _value;
            }
            notepads_markdown_core_cancel const *Origin() const noexcept
            {
                return _origin;
            }
            void Adopt(T *value)
            {
                WINRT_ASSERT(_value == nullptr);
                _value = value;
                if (_value != nullptr)
                {
                    liveHandles.fetch_add(1, std::memory_order_relaxed);
                    if constexpr (std::is_same_v<T, notepads_markdown_core_cancel>)
                    {
                        _context.Publish(_value);
                    }
                }
            }
            void Reset() noexcept
            {
                if (auto value = std::exchange(_value, nullptr))
                {
                    if constexpr (std::is_same_v<T, notepads_markdown_core_cancel>)
                    {
                        _context.Withdraw(value);
                    }
                    auto status = Destroy(value);
                    _context.RecordCleanup(status, L"ABI3 handle destroy");
                    // Real destroy has returned before C++ ownership is retired.
                    liveHandles.fetch_sub(1, std::memory_order_release);
                }
            }

        private:
            Context &_context;
            T *_value{};
            // Borrowed identity only; the suite's enclosing Cancel guard outlives this handle.
            notepads_markdown_core_cancel const *_origin{};
        };

        using Cancel = Handle<notepads_markdown_core_cancel, notepads_markdown_core_cancel_destroy>;
        using Session = Handle<notepads_markdown_core_session, notepads_markdown_core_session_destroy>;
        using Job = Handle<notepads_markdown_core_job, notepads_markdown_core_job_destroy>;
        using Model = Handle<notepads_markdown_core_model, notepads_markdown_core_model_destroy>;
        using Stream = Handle<notepads_markdown_core_stream, notepads_markdown_core_stream_destroy>;
        using Query = Handle<notepads_markdown_core_query, notepads_markdown_core_query_destroy>;

        void NewCancel(Context &context, Audit &audit, Cancel &cancel, notepads_markdown_core_model const *model = nullptr)
        {
            context.Arbitrate();
            auto error = Pod<notepads_markdown_core_error>();
            notepads_markdown_core_cancel *raw{};
            auto status = model != nullptr ? notepads_markdown_core_model_cancel_create(model, &raw, &error) :
                                            notepads_markdown_core_cancel_create(nullptr, &raw, &error);
            cancel.Adopt(raw);
            audit.Call(model != nullptr ? L"model_cancel_create" : L"cancel_create", status, error);
            audit.Require(raw != nullptr, L"successful cancel creation returns a handle");
            // Publish inherits a request racing creation, including fresh flags.
            context.Arbitrate();
        }

        void Prepare(Context &context, Audit &audit, Session &session, Job &job, Cancel &update,
            uint8_t const* bytes, size_t length, notepads_markdown_core_snapshot const &descriptor)
        {
            auto error = Pod<notepads_markdown_core_error>();
            notepads_markdown_core_job* rawJob{};
            notepads_markdown_core_cancel* rawCancel{};
            auto status = notepads_markdown_core_session_start_request(session.Get(), context.CancellationRequested() ? 1u : 0u,
                &rawJob, &rawCancel, &error);
            job.Adopt(rawJob);
            update.Adopt(rawCancel);
            if (status == NOTEPADS_MARKDOWN_CORE_CANCELLED && context.CancellationRequested())
            {
                context.Arbitrate();
            }
            audit.Call(L"session_start_request", status, error, NOTEPADS_MARKDOWN_CORE_OK, update.Get());
            auto result = Pod<notepads_markdown_core_submit_result>();
            status = notepads_markdown_core_job_replace_utf8(session.Get(), job.Get(), bytes, length, &descriptor, 0, &result, &error);
            audit.Call(L"job_replace_utf8", status, error, NOTEPADS_MARKDOWN_CORE_OK, update.Get());
            audit.Require(result.state_valid == 1 && result.source_accepted == 1 && result.has_ready == 1,
                L"source accepted and exactly one owned ready ticket");
        }

        notepads_markdown_core_snapshot Snapshot(uint32_t profile, uint8_t keyByte = 0x21) noexcept
        {
            auto result = Pod<notepads_markdown_core_snapshot>();
            result.source.document[0] = 4;
            result.source.document[15] = keyByte;
            result.source.revision = 1;
            result.profile = profile;
            return result;
        }

        bool SameIdentity(notepads_markdown_core_identity const &left, notepads_markdown_core_identity const &right) noexcept
        {
            return std::memcmp(left.document, right.document, sizeof(left.document)) == 0 &&
                left.revision == right.revision && left.generation == right.generation &&
                left.profile == right.profile && left.extensions == right.extensions;
        }

        struct Fixture
        {
            Cancel cancel;
            Cancel update;
            Session session;
            Job job;
            Model model;
            notepads_markdown_core_model_summary_result summary{Pod<notepads_markdown_core_model_summary_result>()};

            Fixture(Context &context, Audit &audit, std::string_view input, uint8_t keyByte = 0x21)
                : cancel(context), update(context), session(context), job(context), model(context)
            {
                NewCancel(context, audit, cancel);
                auto error = Pod<notepads_markdown_core_error>();
                notepads_markdown_core_session *rawSession{};
                auto status = notepads_markdown_core_session_create(cancel.Get(), NOTEPADS_MARKDOWN_CORE_C0, 0, &rawSession, &error);
                session.Adopt(rawSession);
                audit.Call(L"session_create C0", status, error, NOTEPADS_MARKDOWN_CORE_OK, cancel.Get());
                audit.Require(rawSession != nullptr, L"successful session creation returns a handle");
                auto descriptor = Snapshot(NOTEPADS_MARKDOWN_CORE_C0, keyByte);
                Prepare(context, audit, session, job, update, reinterpret_cast<uint8_t const*>(input.data()), input.size(), descriptor);
                audit.Call(L"job_run", notepads_markdown_core_job_run(job.Get(), &error), error, NOTEPADS_MARKDOWN_CORE_OK, update.Get());
                notepads_markdown_core_model *rawModel{};
                status = notepads_markdown_core_job_install(session.Get(), job.Get(), &rawModel, &error);
                model.Adopt(rawModel);
                audit.Call(L"job_install", status, error, NOTEPADS_MARKDOWN_CORE_OK, update.Get());
                audit.Require(rawModel != nullptr, L"successful install returns a model");
                job.Reset();
                audit.Call(L"model_summary", notepads_markdown_core_model_summary(model.Get(), &summary, &error), error);
                audit.Require(summary.main_flow_id != 0 && summary.flows != 0, L"actual Main flow exists");
                audit.Require(summary.identity.profile == NOTEPADS_MARKDOWN_CORE_C0 && summary.identity.extensions == 0,
                    L"model identity is the requested C0 policy");
                audit.Require(summary.identity.revision == 1 && summary.identity.generation != 0,
                    L"actual revision and installed generation are present");
                audit.Require(std::memcmp(summary.identity.document, descriptor.source.document, 16) == 0,
                    L"installed model retains its actual source document");
                context.Arbitrate();
            }
        };

        notepads_markdown_core_query_descriptor Descriptor(Fixture const &fixture, uint32_t domain, uint64_t start,
                                           uint64_t end, uint32_t affinity = NOTEPADS_MARKDOWN_CORE_BEFORE) noexcept
        {
            auto result = Pod<notepads_markdown_core_query_descriptor>();
            result.identity = fixture.summary.identity;
            result.domain = domain;
            result.target_kind = NOTEPADS_MARKDOWN_CORE_FLOW;
            // Original research fragment/flow numbers are not production IDs.
            result.target_id = fixture.summary.main_flow_id;
            result.affinity = affinity;
            result.start = start;
            result.end = end;
            return result;
        }

        Stream NewStream(Context &context, Audit &audit, notepads_markdown_core_model const *model, notepads_markdown_core_cancel const *cancel)
        {
            auto error = Pod<notepads_markdown_core_error>();
            notepads_markdown_core_stream *raw{};
            auto status = notepads_markdown_core_stream_create(model, cancel, &raw, &error);
            Stream stream(context, cancel);
            stream.Adopt(raw);
            audit.Call(L"stream_create", status, error, NOTEPADS_MARKDOWN_CORE_OK, cancel);
            audit.Require(raw != nullptr, L"successful stream creation returns a handle");
            return stream;
        }

        uint64_t Utf16Length(Audit &audit, std::span<uint8_t const> bytes)
        {
            uint64_t result{};
            for (size_t at = 0; at < bytes.size();)
            {
                uint32_t scalar = bytes[at++];
                uint32_t continuation{};
                uint32_t minimum{};
                if (scalar < 0x80)
                {
                    ++result;
                    continue;
                }
                if (scalar >= 0xC2 && scalar <= 0xDF)
                {
                    scalar &= 0x1F;
                    continuation = 1;
                    minimum = 0x80;
                }
                else if (scalar >= 0xE0 && scalar <= 0xEF)
                {
                    scalar &= 0x0F;
                    continuation = 2;
                    minimum = 0x800;
                }
                else
                {
                    audit.Require(scalar >= 0xF0 && scalar <= 0xF4, L"batch starts with a valid UTF8 scalar");
                    scalar &= 7;
                    continuation = 3;
                    minimum = 0x10000;
                }
                audit.Require(continuation <= bytes.size() - at, L"4B stream batch never splits a scalar");
                for (uint32_t index = 0; index < continuation; ++index)
                {
                    auto next = bytes[at++];
                    audit.Require((next & 0xC0) == 0x80, L"batch has valid UTF8 continuation bytes");
                    scalar = (scalar << 6) | (next & 0x3F);
                }
                audit.Require(scalar >= minimum && scalar <= 0x10FFFF &&
                    !(scalar >= 0xD800 && scalar <= 0xDFFF), L"batch contains complete Unicode scalars");
                result += scalar > 0xFFFF ? 2 : 1;
            }
            return result;
        }

        void ReadMain(Context &context, Audit &audit, Stream &stream, std::string_view expected, uint64_t utf16)
        {
            std::string actual;
            uint64_t units{};
            bool done{};
            // Finite harness bound, derived only from the fixed expected literal.
            for (size_t batch = 0; batch <= expected.size(); ++batch)
            {
                context.Arbitrate();
                std::array<uint8_t, 4> bytes{};
                auto read = Pod<notepads_markdown_core_read_result>();
                auto error = Pod<notepads_markdown_core_error>();
                auto status = notepads_markdown_core_stream_read(stream.Get(), bytes.data(), bytes.size(), &read, &error);
                audit.Call(L"stream_read 4B", status, error, NOTEPADS_MARKDOWN_CORE_OK, stream.Origin());
                audit.Require(read.written <= bytes.size() && read.done <= 1, L"stream returns a valid batch");
                units += Utf16Length(audit, std::span<uint8_t const>(bytes.data(), static_cast<size_t>(read.written)));
                actual.append(reinterpret_cast<char const *>(bytes.data()), static_cast<size_t>(read.written));
                audit.Require(read.next == units, L"Main next is cumulative UTF16 rather than UTF8");
                if (read.done != 0)
                {
                    done = true;
                    break;
                }
                audit.Require(read.written != 0, L"unfinished stream makes progress");
            }
            audit.Require(done && actual == expected && units == utf16, L"Main matches the independent literal");
            std::array<uint8_t, 4> bytes{};
            auto read = Pod<notepads_markdown_core_read_result>();
            auto error = Pod<notepads_markdown_core_error>();
            audit.Call(L"stream_read after done", notepads_markdown_core_stream_read(stream.Get(), bytes.data(), bytes.size(),
                &read, &error), error, NOTEPADS_MARKDOWN_CORE_OK, stream.Origin());
            audit.Require(read.written == 0 && read.next == utf16 && read.done == 1,
                L"read after done is empty and preserves Main UTF16");
        }

        Query NewQuery(Context &context, Audit &audit, Fixture const &fixture,
                       notepads_markdown_core_cancel const *cancel, std::span<notepads_markdown_core_query_descriptor const> descriptors,
                       bool ranges = false)
        {
            auto error = Pod<notepads_markdown_core_error>();
            notepads_markdown_core_query *raw{};
            auto status = ranges ? notepads_markdown_core_map_ranges(fixture.model.Get(), cancel, descriptors.data(),
                descriptors.size(), &raw, &error) : notepads_markdown_core_map_points(fixture.model.Get(), cancel,
                descriptors.data(), descriptors.size(), &raw, &error);
            Query query(context, cancel);
            query.Adopt(raw);
            audit.Call(ranges ? L"map_ranges" : L"map_points", status, error, NOTEPADS_MARKDOWN_CORE_OK, cancel);
            audit.Require(raw != nullptr, L"successful batch query returns a handle");
            return query;
        }

        template <typename T> std::vector<T> ReadTable(Audit &audit, Query &query, uint32_t table, uint64_t count)
        {
            audit.Require(count <= 64 && count <= NOTEPADS_MARKDOWN_CORE_MAX_BATCH, L"fixed small response table stays bounded");
            std::vector<T> rows(static_cast<size_t>(count));
            static_assert(alignof(T) == 8);
            auto output = Pod<notepads_markdown_core_read_result>();
            auto error = Pod<notepads_markdown_core_error>();
            auto status = notepads_markdown_core_query_read(query.Get(), table, 0, count, static_cast<uint32_t>(sizeof(T)), rows.data(), &output, &error);
            audit.Call(L"query_read exact wire rows", status, error, NOTEPADS_MARKDOWN_CORE_OK, query.Origin());
            audit.Require(output.written == count && output.next == count && output.done == 1,
                L"complete table export has the exact row count and done");
            return rows;
        }

        struct Tables
        {
            notepads_markdown_core_query_summary_result summary;
            std::vector<notepads_markdown_core_point> points;
            std::vector<notepads_markdown_core_range> ranges;
            std::vector<notepads_markdown_core_selection> selections;
            std::vector<notepads_markdown_core_span> spans;
            std::vector<notepads_markdown_core_position> anchors;
            std::vector<notepads_markdown_core_contributor> contributors;
            std::vector<notepads_markdown_core_contribution> contributions;

            Tables(Audit &audit, Query &query, notepads_markdown_core_identity const &identity)
                : summary(Pod<notepads_markdown_core_query_summary_result>())
            {
                auto error = Pod<notepads_markdown_core_error>();
                audit.Call(L"query_summary", notepads_markdown_core_query_summary(query.Get(), &summary, &error), error, NOTEPADS_MARKDOWN_CORE_OK, query.Origin());
                audit.Require(SameIdentity(summary.identity, identity), L"response keeps the complete model identity");
                points = ReadTable<notepads_markdown_core_point>(audit, query, NOTEPADS_MARKDOWN_CORE_POINTS, summary.points);
                ranges = ReadTable<notepads_markdown_core_range>(audit, query, NOTEPADS_MARKDOWN_CORE_RANGES, summary.ranges);
                selections = ReadTable<notepads_markdown_core_selection>(audit, query, NOTEPADS_MARKDOWN_CORE_SELECTIONS, summary.selections);
                spans = ReadTable<notepads_markdown_core_span>(audit, query, NOTEPADS_MARKDOWN_CORE_SPANS, summary.spans);
                anchors = ReadTable<notepads_markdown_core_position>(audit, query, NOTEPADS_MARKDOWN_CORE_ANCHORS, summary.anchors);
                contributors = ReadTable<notepads_markdown_core_contributor>(audit, query, NOTEPADS_MARKDOWN_CORE_CONTRIBUTORS, summary.contributors);
                contributions = ReadTable<notepads_markdown_core_contribution>(audit, query, NOTEPADS_MARKDOWN_CORE_CONTRIBUTIONS, summary.contributions);
            }
        };

        template <typename T> std::span<T const> Slice(Audit &audit, std::vector<T> const &values, notepads_markdown_core_rows rows)
        {
            audit.Require(static_cast<uint64_t>(rows.start) + rows.count <= values.size(),
                L"wire Rows index the actual complete corresponding table");
            return std::span<T const>(values).subspan(rows.start, rows.count);
        }

        void Boundary(Audit &audit, Tables const &tables, notepads_markdown_core_point const &point, uint32_t mainFlow,
                      uint32_t direction, uint64_t chosen, uint64_t start, uint64_t end, uint32_t relation)
        {
            uint32_t domain = direction == NOTEPADS_MARKDOWN_CORE_SOURCE_TO_TARGET ? NOTEPADS_MARKDOWN_CORE_FLOW_UTF16 : NOTEPADS_MARKDOWN_CORE_SOURCE_UTF8;
            audit.Require(point.target_kind == NOTEPADS_MARKDOWN_CORE_FLOW && point.target_id == mainFlow && point.direction == direction,
                L"point uses the actual Main target and expected direction");
            audit.Require(point.chosen.domain == domain && point.chosen.value == chosen,
                L"point chooses the independent expected boundary");
            audit.Require(point.related.has_envelope == 1 && point.related.envelope.domain == domain &&
                point.related.envelope.start == start && point.related.envelope.end == end,
                L"point related envelope matches the independent oracle");
            uint32_t effects = relation == NOTEPADS_MARKDOWN_CORE_ATOMIC ? NOTEPADS_MARKDOWN_CORE_EFFECT_ATOMIC :
                relation == NOTEPADS_MARKDOWN_CORE_HIDDEN ? NOTEPADS_MARKDOWN_CORE_EFFECT_HIDDEN : 0;
            audit.Require(point.correspondence == relation && point.effects == effects,
                L"point correspondence and effects match the independent oracle");
            audit.Require(!Slice(audit, tables.contributors, point.contributors).empty(),
                L"point contains real contributing rows");
            (void)Slice(audit, tables.spans, point.related.spans);
            (void)Slice(audit, tables.anchors, point.related.anchors);
        }

        struct ExpectedAtom
        {
            uint32_t kind;
            uint64_t sourceStart;
            uint64_t sourceEnd;
            uint64_t targetStart;
            uint64_t targetEnd;
            uint32_t actualId{};
        };

        template <typename T> void Atom(Audit &audit, ExpectedAtom &expected, T const &actual)
        {
            audit.Require(actual.atom != 0 && actual.source_part == 0 && actual.kind == expected.kind &&
                actual.source_start == expected.sourceStart && actual.source_end == expected.sourceEnd &&
                actual.target_start == expected.targetStart && actual.target_end == expected.targetEnd,
                L"contributor matches original kind/source/target, not a research fragment number");
            if (expected.actualId == 0)
            {
                expected.actualId = actual.atom;
            }
            audit.Require(actual.atom == expected.actualId, L"one semantic atom retains its actual model ID");
        }

        void Contributors(Audit &audit, Tables const &tables, notepads_markdown_core_rows rows,
                          std::array<ExpectedAtom, 3> &atoms, std::initializer_list<size_t> indices,
                          std::initializer_list<uint32_t> relations)
        {
            auto actual = Slice(audit, tables.contributors, rows);
            audit.Require(actual.size() == indices.size() && actual.size() == relations.size(),
                L"boundary contributor count matches the independent oracle");
            auto index = indices.begin();
            auto relation = relations.begin();
            for (auto const &row : actual)
            {
                Atom(audit, atoms[*index++], row);
                audit.Require(row.correspondence == *relation++, L"boundary contributor relation is exact");
            }
        }

        void RelatedSelection(Audit &audit, Tables const &tables, notepads_markdown_core_related const &related,
                              uint32_t domain, uint64_t start, uint64_t end,
                              std::initializer_list<uint64_t> expectedAnchors)
        {
            auto spans = Slice(audit, tables.spans, related.spans);
            audit.Require(spans.size() == 1 && spans[0].domain == domain && spans[0].start == start && spans[0].end == end,
                L"selection has the independent exact related span");
            audit.Require(related.has_envelope == 1 && related.envelope.domain == domain &&
                related.envelope.start == start && related.envelope.end == end,
                L"selection envelope matches its independent span");
            auto anchors = Slice(audit, tables.anchors, related.anchors);
            audit.Require(anchors.size() == expectedAnchors.size(), L"selection anchor count is exact");
            auto expected = expectedAnchors.begin();
            for (auto const &anchor : anchors)
            {
                audit.Require(anchor.domain == domain && anchor.value == *expected++, L"selection anchor is exact");
            }
        }

        void TextCase(Context &context, Audit &audit, std::string_view source, std::string_view expected, uint64_t units)
        {
            Fixture fixture(context, audit, source);
            audit.Require(fixture.summary.main_utf16 == units, L"summary Main UTF16 matches the literal");
            auto stream = NewStream(context, audit, fixture.model.Get(), fixture.cancel.Get());
            ReadMain(context, audit, stream, expected, units);
        }

        void Points(Context &context, Audit &audit)
        {
            // Original m1-provenance independent fixture bytes.
            Fixture partial(context, audit, ">\t\tfoo\n", 0x09);
            auto stream = NewStream(context, audit, partial.model.Get(), partial.cancel.Get());
            ReadMain(context, audit, stream, "  foo\n", 6);
            std::array descriptors{
                Descriptor(partial, NOTEPADS_MARKDOWN_CORE_FLOW_UTF16, 1, 1, NOTEPADS_MARKDOWN_CORE_BEFORE),
                Descriptor(partial, NOTEPADS_MARKDOWN_CORE_FLOW_UTF16, 1, 1, NOTEPADS_MARKDOWN_CORE_AFTER)};
            auto query = NewQuery(context, audit, partial, partial.cancel.Get(), descriptors);
            Tables tables(audit, query, partial.summary.identity);
            audit.Require(tables.summary.kind == 1 && tables.points.size() == 2, L"two point results are exported");
            for (size_t index = 0; index < 2; ++index)
            {
                auto const &point = tables.points[index];
                Boundary(audit, tables, point, partial.summary.main_flow_id, NOTEPADS_MARKDOWN_CORE_TARGET_TO_SOURCE,
                    2 + index, 2, 3, NOTEPADS_MARKDOWN_CORE_ATOMIC);
                auto rows = Slice(audit, tables.contributors, point.contributors);
                audit.Require(rows.size() == 1, L"partial tab has exactly one actual contributing atom");
                ExpectedAtom atom{NOTEPADS_MARKDOWN_CORE_TRANSFORM_ATOM, 2, 3, 0, 2};
                Atom(audit, atom, rows[0]);
                audit.Require(rows[0].correspondence == NOTEPADS_MARKDOWN_CORE_ATOMIC, L"partial tab contributor is atomic");
            }
            Fixture code(context, audit, "`abc`", 0x11);
            std::array codeDescriptors{
                Descriptor(code, NOTEPADS_MARKDOWN_CORE_SOURCE_UTF8, 2, 2, NOTEPADS_MARKDOWN_CORE_BEFORE),
                Descriptor(code, NOTEPADS_MARKDOWN_CORE_FLOW_UTF16, 2, 2, NOTEPADS_MARKDOWN_CORE_AFTER)};
            auto codeQuery = NewQuery(context, audit, code, code.cancel.Get(), codeDescriptors);
            Tables codeTables(audit, codeQuery, code.summary.identity);
            audit.Require(codeTables.summary.kind == 1 && codeTables.points.size() == 2, L"two code point results exported");
            Boundary(audit, codeTables, codeTables.points[0], code.summary.main_flow_id, NOTEPADS_MARKDOWN_CORE_SOURCE_TO_TARGET,
                1, 1, 1, NOTEPADS_MARKDOWN_CORE_EXACT);
            Boundary(audit, codeTables, codeTables.points[1], code.summary.main_flow_id, NOTEPADS_MARKDOWN_CORE_TARGET_TO_SOURCE,
                3, 3, 3, NOTEPADS_MARKDOWN_CORE_EXACT);
            ExpectedAtom copy{NOTEPADS_MARKDOWN_CORE_COPY_ATOM, 1, 4, 0, 3};
            for (auto const &point : codeTables.points)
            {
                auto rows = Slice(audit, codeTables.contributors, point.contributors);
                audit.Require(rows.size() == 1 && rows[0].correspondence == NOTEPADS_MARKDOWN_CORE_EXACT,
                    L"code interior has one exact Copy contributor");
                Atom(audit, copy, rows[0]);
            }
        }

        void Ranges(Context &context, Audit &audit)
        {
            Fixture fixture(context, audit, "`abc`", 0x11);
            // Original m1-range code.logical-excludes-edge-hidden / code.source-includes-hidden.
            std::array descriptors{
                Descriptor(fixture, NOTEPADS_MARKDOWN_CORE_FLOW_UTF16, 0, 3),
                Descriptor(fixture, NOTEPADS_MARKDOWN_CORE_SOURCE_UTF8, 0, 5)};
            auto query = NewQuery(context, audit, fixture, fixture.cancel.Get(), descriptors, true);
            Tables tables(audit, query, fixture.summary.identity);
            audit.Require(tables.summary.kind == 2 && tables.ranges.size() == 2 && tables.selections.size() == 2,
                L"two real nonempty range selections exported");
            std::array<ExpectedAtom, 3> atoms{{
                {NOTEPADS_MARKDOWN_CORE_HIDDEN_ATOM, 0, 1, 0, 0},
                {NOTEPADS_MARKDOWN_CORE_COPY_ATOM, 1, 4, 0, 3},
                {NOTEPADS_MARKDOWN_CORE_HIDDEN_ATOM, 4, 5, 3, 3}}};
            for (size_t index = 0; index < 2; ++index)
            {
                bool fromSource = index != 0;
                auto const &tag = tables.ranges[index];
                audit.Require(tag.kind == 2, L"nonempty range is tagged Selection");
                auto selections = Slice(audit, tables.selections, tag.rows);
                audit.Require(selections.size() == 1, L"each code range has one selection");
                auto const &selection = selections[0];
                uint32_t direction = fromSource ? NOTEPADS_MARKDOWN_CORE_SOURCE_TO_TARGET : NOTEPADS_MARKDOWN_CORE_TARGET_TO_SOURCE;
                audit.Require(selection.target_kind == NOTEPADS_MARKDOWN_CORE_FLOW &&
                    selection.target_id == fixture.summary.main_flow_id && selection.direction == direction &&
                    selection.expanded == 0 && selection.effects ==
                    (fromSource ? static_cast<uint32_t>(NOTEPADS_MARKDOWN_CORE_EFFECT_HIDDEN) : 0u),
                    L"selection target/direction/expanded/effects match the independent oracle");
                if (fromSource)
                {
                    Boundary(audit, tables, selection.start, fixture.summary.main_flow_id, direction, 0, 0, 0, NOTEPADS_MARKDOWN_CORE_EXACT);
                    Boundary(audit, tables, selection.end, fixture.summary.main_flow_id, direction, 3, 3, 3, NOTEPADS_MARKDOWN_CORE_EXACT);
                    Contributors(audit, tables, selection.start.contributors, atoms, {0}, {NOTEPADS_MARKDOWN_CORE_EXACT});
                    Contributors(audit, tables, selection.end.contributors, atoms, {2}, {NOTEPADS_MARKDOWN_CORE_EXACT});
                    RelatedSelection(audit, tables, selection.related, NOTEPADS_MARKDOWN_CORE_FLOW_UTF16, 0, 3, {0, 3});
                }
                else
                {
                    Boundary(audit, tables, selection.start, fixture.summary.main_flow_id, direction, 0, 0, 1, NOTEPADS_MARKDOWN_CORE_HIDDEN);
                    Boundary(audit, tables, selection.end, fixture.summary.main_flow_id, direction, 5, 4, 5, NOTEPADS_MARKDOWN_CORE_HIDDEN);
                    Contributors(audit, tables, selection.start.contributors, atoms, {0, 1}, {NOTEPADS_MARKDOWN_CORE_HIDDEN, NOTEPADS_MARKDOWN_CORE_EXACT});
                    Contributors(audit, tables, selection.end.contributors, atoms, {1, 2}, {NOTEPADS_MARKDOWN_CORE_EXACT, NOTEPADS_MARKDOWN_CORE_HIDDEN});
                    RelatedSelection(audit, tables, selection.related, NOTEPADS_MARKDOWN_CORE_SOURCE_UTF8, 1, 4, {});
                }
                auto content = Slice(audit, tables.contributions, selection.contributions);
                audit.Require(content.size() == (fromSource ? size_t{3} : size_t{1}),
                    L"logical range excludes edge Hidden while source range includes both");
                for (size_t at = 0; at < content.size(); ++at)
                {
                    Atom(audit, atoms[fromSource ? at : 1], content[at]);
                    audit.Require(content[at].expanded == 0, L"original code contributions are not expanded");
                }
            }
            audit.Require(atoms[0].actualId != 0 && atoms[1].actualId != 0 && atoms[2].actualId != 0 &&
                atoms[0].actualId != atoms[1].actualId && atoms[0].actualId != atoms[2].actualId &&
                atoms[1].actualId != atoms[2].actualId, L"independent semantic atoms resolve to distinct actual IDs");
        }

        void Retained(Context &context, Audit &audit)
        {
            Fixture fixture(context, audit, "`abc`", 0x11);
            auto error = Pod<notepads_markdown_core_error>();
            audit.Call(L"cancel old model token only", notepads_markdown_core_cancel_request(fixture.cancel.Get()), error);
            fixture.session.Reset();
            Cancel fresh(context);
            NewCancel(context, audit, fresh, fixture.model.Get());
            Model clone(context);
            notepads_markdown_core_model *raw{};
            auto status = notepads_markdown_core_model_clone(fixture.model.Get(), fresh.Get(), &raw, &error);
            clone.Adopt(raw);
            audit.Call(L"model_clone with fresh same-root token", status, error, NOTEPADS_MARKDOWN_CORE_OK, fresh.Get());
            audit.Require(raw != nullptr, L"model clone returns an independent handle");
            auto summary = Pod<notepads_markdown_core_model_summary_result>();
            audit.Call(L"retained model_summary", notepads_markdown_core_model_summary(clone.Get(), &summary, &error), error);
            audit.Require(SameIdentity(summary.identity, fixture.summary.identity) && summary.main_utf16 == 3,
                L"model survives job/session destruction and old-token cancellation");
            auto stream = NewStream(context, audit, clone.Get(), fresh.Get());
            std::array descriptor{Descriptor(fixture, NOTEPADS_MARKDOWN_CORE_SOURCE_UTF8, 2, 2)};
            auto query = NewQuery(context, audit, fixture, fresh.Get(), descriptor);
            fixture.model.Reset();
            clone.Reset();
            ReadMain(context, audit, stream, "abc", 3);
            Tables tables(audit, query, fixture.summary.identity);
            audit.Require(tables.points.size() == 1, L"query survives both explicit model handles");
            Boundary(audit, tables, tables.points[0], fixture.summary.main_flow_id,
                NOTEPADS_MARKDOWN_CORE_SOURCE_TO_TARGET, 1, 1, 1, NOTEPADS_MARKDOWN_CORE_EXACT);
        }

        void WrongGeneration(Context &context, Audit &audit)
        {
            Fixture fixture(context, audit, "`abc`", 0x11);
            auto descriptor = Descriptor(fixture, NOTEPADS_MARKDOWN_CORE_FLOW_UTF16, 0, 0);
            ++descriptor.identity.generation;
            Query query(context, fixture.cancel.Get());
            notepads_markdown_core_query *raw{};
            auto error = Pod<notepads_markdown_core_error>();
            auto status = notepads_markdown_core_map_points(fixture.model.Get(), fixture.cancel.Get(), &descriptor, 1, &raw, &error);
            query.Adopt(raw);
            audit.Call(L"map_points wrong generation", status, error, NOTEPADS_MARKDOWN_CORE_WRONG_GENERATION, fixture.cancel.Get());
            audit.Require(raw == nullptr, L"wrong generation returns no consumable query");
            audit.Require(error.expected == fixture.summary.identity.generation &&
                error.actual == descriptor.identity.generation, L"typed generation error retains expected/actual");
        }

        void WrongRoot(Context &context, Audit &audit)
        {
            Fixture fixture(context, audit, "`abc`", 0x11);
            Cancel foreign(context);
            NewCancel(context, audit, foreign);
            Stream stream(context);
            notepads_markdown_core_stream *raw{};
            auto error = Pod<notepads_markdown_core_error>();
            auto status = notepads_markdown_core_stream_create(fixture.model.Get(), foreign.Get(), &raw, &error);
            stream.Adopt(raw);
            audit.Call(L"stream_create wrong root", status, error, NOTEPADS_MARKDOWN_CORE_WRONG_BUDGET);
            audit.Require(raw == nullptr, L"foreign-root token returns no consumable stream");
        }

        void RejectProfileOrExtensions(Context &context, Audit &audit, uint32_t profile, uint32_t extensions)
        {
            Cancel cancel(context);
            Session session(context);
            NewCancel(context, audit, cancel);
            auto error = Pod<notepads_markdown_core_error>();
            notepads_markdown_core_session *raw{};
            auto status = notepads_markdown_core_session_create(cancel.Get(), profile, extensions, &raw, &error);
            session.Adopt(raw);
            audit.ExpectStatus(L"session_create rejects unknown configuration", status, NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT);
            audit.Require(raw == nullptr, L"invalid configuration has no session");
        }

        void PreCancel(Context &context, Audit &audit)
        {
            Cancel cancel(context);
            Session session(context);
            NewCancel(context, audit, cancel);
            auto error = Pod<notepads_markdown_core_error>();
            audit.Call(L"local pre-cancel request", notepads_markdown_core_cancel_request(cancel.Get()), error);
            notepads_markdown_core_session *raw{};
            auto status = notepads_markdown_core_session_create(cancel.Get(), NOTEPADS_MARKDOWN_CORE_C0, 0, &raw, &error);
            session.Adopt(raw);
            audit.Call(L"pre-cancel session_create", status, error, NOTEPADS_MARKDOWN_CORE_CANCELLED);
            audit.Require(raw == nullptr, L"pre-cancel returns no consumable session");
        }

        template <typename F> void Check(Context &context, SuiteResult &result, wchar_t const *name, F &&body)
        {
            context.Arbitrate();
            Audit audit(context);
            try
            {
                body(audit);
            }
            catch (CheckFailure const &)
            {
                // The call-site recorded a hard fault, or observed a proven cooperative cancellation.
            }
            catch (...)
            {
                context.RecordException(std::current_exception(), L"fixed check body");
            }
            // The body has returned or unwound; all of its local guards have retired.
            context.Arbitrate();
            result.checks.push_back({name, true, audit.status, L"Actual native calls and fixed independent assertions passed."});
        }
    }

    Context::Context() noexcept
    {
        liveWorkers.fetch_add(1, std::memory_order_relaxed);
    }

    Context::~Context()
    {
        WINRT_ASSERT(std::all_of(_tokens.begin(), _tokens.end(), [](auto token)
        {
            return token == nullptr;
        }));
        liveWorkers.fetch_sub(1, std::memory_order_release);
    }

    void Context::RequestCancel() noexcept
    {
        RequestCancelUsing<notepads_markdown_core_cancel_request>();
        std::lock_guard guard(_controllerMutex);
        if (_controller != nullptr)
        {
            // The public validation Task closes its current controller scope.
            // Stop uses the same publication gate and prevents later submissions.
            _controller->Stop();
        }
    }

    void Context::PublishController(CoreController* controller) noexcept
    {
        std::lock_guard guard(_controllerMutex);
        if (_controller != nullptr || controller == nullptr)
        {
            RecordCleanupInvariant(L"controller cancellation registration");
            return;
        }
        _controller = controller;
        if (_requestCancelled.load(std::memory_order_acquire))
        {
            _controller->Stop();
        }
    }

    void Context::WithdrawController(CoreController* controller) noexcept
    {
        std::lock_guard guard(_controllerMutex);
        if (_controller == controller)
        {
            _controller = nullptr;
        }
        else
        {
            RecordCleanupInvariant(L"controller cancellation withdrawal");
        }
    }

    bool Context::CancellationRequested() const noexcept
    {
        return _requestCancelled.load(std::memory_order_acquire);
    }

    bool Context::IsPublishedOrigin(notepads_markdown_core_cancel const *token) const noexcept
    {
        if (token == nullptr)
        {
            return false;
        }
        std::lock_guard guard(_mutex);
        return std::find(_tokens.begin(), _tokens.end(), token) != _tokens.end();
    }

    void Context::RecordPrimary(FaultKind kind, int32_t status, wchar_t const *operation) noexcept
    {
        std::lock_guard guard(_mutex);
        if (!_primary.present)
        {
            _primary = {true, kind, status, operation};
        }
    }

    void Context::RecordException(std::exception_ptr exception, wchar_t const *operation) noexcept
    {
        try
        {
            std::rethrow_exception(exception);
        }
        catch (ArbitrationFailure const &)
        {
            // The original fixed records are already authoritative.
        }
        catch (winrt::hresult_canceled const &error)
        {
            if (!CancellationRequested())
            {
                RecordPrimary(FaultKind::UnexpectedException, error.code().value, operation);
            }
        }
        catch (winrt::hresult_error const &error)
        {
            RecordPrimary(FaultKind::UnexpectedException, error.code().value, operation);
        }
        catch (std::bad_alloc const &)
        {
            RecordPrimary(FaultKind::UnexpectedException, E_OUTOFMEMORY, operation);
        }
        catch (...)
        {
            RecordPrimary(FaultKind::UnexpectedException, E_FAIL, operation);
        }
    }

    void Context::Arbitrate() const
    {
        FaultRecord primary;
        FaultRecord cleanup;
        {
            std::lock_guard guard(_mutex);
            primary = _primary;
            cleanup = _cleanup;
        }
        if (primary.present || cleanup.present)
        {
            std::wstring detail = L"Fixed core validation failed";
            auto append = [&](wchar_t const *label, FaultRecord const &fault)
            {
                if (fault.present)
                {
                    detail += std::wstring(L"; ") + label + L": " + fault.operation +
                        L", kind=" + std::to_wstring(static_cast<int>(fault.kind)) +
                        L", actual=" + std::to_wstring(fault.status);
                }
            };
            append(L"primary", primary);
            append(L"cleanup", cleanup);
            throw ArbitrationFailure(detail);
        }
        if (CancellationRequested())
        {
            throw winrt::hresult_canceled();
        }
    }

    void Context::Publish(notepads_markdown_core_cancel *token)
    {
        PublishUsing<notepads_markdown_core_cancel_request>(token);
    }

    void Context::Withdraw(notepads_markdown_core_cancel *token) noexcept
    {
        std::lock_guard guard(_mutex);
        auto slot = std::find(_tokens.begin(), _tokens.end(), token);
        if (slot != _tokens.end())
        {
            *slot = nullptr;
        }
    }

    void Context::RecordCleanupLocked(FaultKind kind, int32_t status, wchar_t const *operation) noexcept
    {
        if (!_cleanup.present)
        {
            _cleanup = {true, kind, status, operation};
        }
    }

    void Context::RecordCleanup(int32_t status, wchar_t const *operation) noexcept
    {
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
        {
            std::lock_guard guard(_mutex);
            RecordCleanupLocked(FaultKind::CleanupStatus, status, operation);
        }
    }

    void Context::RecordCleanupInvariant(wchar_t const *operation) noexcept
    {
        std::lock_guard guard(_mutex);
        RecordCleanupLocked(FaultKind::CleanupInvariant, 0, operation);
    }

    FaultRecord Context::PrimaryFailure() const noexcept
    {
        std::lock_guard guard(_mutex);
        return _primary;
    }

    FaultRecord Context::Failure() const noexcept
    {
        std::lock_guard guard(_mutex);
        return _cleanup;
    }

    uint32_t LiveWorkers() noexcept
    {
        return liveWorkers.load(std::memory_order_acquire);
    }

    uint32_t LiveHandles() noexcept
    {
        return liveHandles.load(std::memory_order_acquire);
    }

    SuiteResult Run(Context &context)
    {
        SuiteResult result{notepads_markdown_core_abi_version(), {}};
        if (result.abiVersion != NOTEPADS_MARKDOWN_CORE_ABI_VERSION)
        {
            context.RecordPrimary(FaultKind::UnexpectedStatus, static_cast<int32_t>(result.abiVersion),
                L"Core ABI version mismatch");
            context.Arbitrate();
        }
        result.checks.reserve(14);
        notepads_markdown_platform_validation_result platform{};
        platform.abi_version = NOTEPADS_MARKDOWN_PLATFORM_VALIDATION_ABI_VERSION;
        platform.struct_size = sizeof(platform);
        Check(context, result, L"platform-randomstate", [&](Audit &audit)
        {
            audit.ExpectStatus(L"private platform diagnostic; internal stage unknown on failure",
                notepads_markdown_platform_validation_run(&platform));
            audit.Require(platform.rng_checked == 1 && platform.reserved == 0,
                L"fixed RandomState observations");
        });
        Check(context, result, L"platform-payload-drop", [&](Audit &audit)
        {
            audit.ExpectStatus(L"normal payload contained", platform.payload_status, NOTEPADS_MARKDOWN_PANIC);
            audit.Require(platform.payload_drops == 1 && platform.payload_live == 0 &&
                platform.payload_control_owners == 1, L"normal payload exact Drop and sole control owner");
        });
        Check(context, result, L"platform-core-after-panic", [&](Audit &audit)
        {
            audit.ExpectStatus(L"normal payload contained before fresh Core work", platform.payload_status,
                NOTEPADS_MARKDOWN_PANIC);
            // Real source ownership, parsing, installation, stream export and
            // RAII retirement must still succeed after the contained unwind.
            TextCase(context, audit, "**fresh**", "fresh", 5);
        });
        Check(context, result, L"core-basics-cjk-entity", [&](Audit &audit)
        {
            TextCase(context, audit, "**\xE4\xB8\xAD&amp;\xF0\x9F\x98\x80**",
                "\xE4\xB8\xAD&\xF0\x9F\x98\x80", 4);
        });
        Check(context, result, L"core-line-endings", [&](Audit &audit)
        {
            TextCase(context, audit, "a\r\nb  \r\nc", "a b\nc", 5);
        });
        Check(context, result, L"core-empty-main", [&](Audit &audit)
        {
            TextCase(context, audit, "[r]: /dest", "", 0);
        });
        Check(context, result, L"core-points-atomic", [&](Audit &audit)
        {
            Points(context, audit);
        });
        Check(context, result, L"core-ranges-hidden", [&](Audit &audit)
        {
            Ranges(context, audit);
        });
        Check(context, result, L"core-retained-pins", [&](Audit &audit)
        {
            Retained(context, audit);
        });
        Check(context, result, L"core-wrong-generation", [&](Audit &audit)
        {
            WrongGeneration(context, audit);
        });
        Check(context, result, L"core-wrong-root", [&](Audit &audit)
        {
            WrongRoot(context, audit);
        });
        Check(context, result, L"core-unknown-profile", [&](Audit &audit)
        {
            RejectProfileOrExtensions(context, audit, UINT32_MAX, 0);
        });
        Check(context, result, L"core-invalid-extensions", [&](Audit &audit)
        {
            RejectProfileOrExtensions(context, audit, NOTEPADS_MARKDOWN_CORE_G3, UINT32_MAX);
        });
        Check(context, result, L"core-pre-cancel", [&](Audit &audit)
        {
            PreCancel(context, audit);
        });
        context.Arbitrate();
        return result;
    }
}
