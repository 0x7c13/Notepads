// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeMathCoordinatorTests.h"
#include "Resources/Diagrams/NativeDiagramPresentation.h"
#include <algorithm>
#include <new>

namespace Notepads::Markdown::Native::MathCoordinatorTests
{
    namespace
    {
        notepads_markdown_core_error Error() noexcept
        {
            return {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        }
        notepads_markdown_math_usage Usage(NativeMathControl &control)
        {
            notepads_markdown_math_usage result{};
            result.header = {NOTEPADS_MARKDOWN_MATH_ABI_VERSION, sizeof(result)};
            auto error = Error();
            if (control.Usage(&result, &error) != NOTEPADS_MARKDOWN_CORE_OK)
                throw MathMemoryFailure{error.status, error};
            return result;
        }
        struct CapacityFixture
        {
            static constexpr uint32_t FullControls =
                NOTEPADS_MARKDOWN_DERIVED_PROCESS_RESIDENT_LIMIT / NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT;
            static_assert(FullControls * NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT == NOTEPADS_MARKDOWN_DERIVED_PROCESS_RESIDENT_LIMIT);
            NativeMathControl controls[FullControls + 1];
            NativeMathWorker::PreparedNode *nodes[FullControls][NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT]{};
            ~CapacityFixture()
            {
                for (auto &row : nodes)
                    for (auto &node : row)
                        if (node)
                            NativeMathWorker::DestroyQueued(std::exchange(node, nullptr));
            }
        };
        struct ThreadFixture
        {
            winrt::handle done{CreateEvent(nullptr, TRUE, FALSE, nullptr)};
            bool signaled{};
            bool retired{};
            static void Complete(void *context, notepads_markdown_math_job *job, NativeMathWorkerResult const &result) noexcept
            {
                auto node = static_cast<NativeMathWorker::PreparedNode *>(context);
                auto self = static_cast<ThreadFixture *>(NativeMathWorker::PreparedOwner(node));
                self->signaled = result.stack.threadSignaled && result.status == NOTEPADS_MARKDOWN_CORE_OK &&
                                 result.stack.withinStackBudget && result.stack.headroom >= 1536 * 1024;
                auto error = Error();
                self->retired = notepads_markdown_math_job_retire_metadata(job, &error) == NOTEPADS_MARKDOWN_CORE_OK &&
                                notepads_markdown_math_job_state(job) == NOTEPADS_MARKDOWN_MATH_METADATA_ONLY;
                SetEvent(self->done.get());
            }
        };
        void NeverCalled(void *, notepads_markdown_math_job *, NativeMathWorkerResult const &) noexcept
        {
        }
        bool SameRow(notepads_markdown_math_usage_row const &a, notepads_markdown_math_usage_row const &b) noexcept
        {
            return a.bytes == b.bytes && a.retained_bytes == b.retained_bytes && a.metadata_bytes == b.metadata_bytes &&
                   a.jobs == b.jobs && a.residents == b.residents;
        }
        // Deliberate synchronous dispatch reentry, not real UI/dispatcher coverage.
        // It observes the exact state exposed before the transport can return.
        struct InlineDispatcher : winrt::implements<InlineDispatcher,
            winrt::Windows::UI::Core::ICoreDispatcher, winrt::Windows::UI::Core::ICoreDispatcher2>
        {
            bool HasThreadAccess() const noexcept { return true; }
            void ProcessEvents(winrt::Windows::UI::Core::CoreProcessEventsOption) { winrt::throw_hresult(E_NOTIMPL); }
            winrt::Windows::Foundation::IAsyncAction RunAsync(winrt::Windows::UI::Core::CoreDispatcherPriority,
                winrt::Windows::UI::Core::DispatchedHandler const &) { throw winrt::hresult_not_implemented(); }
            winrt::Windows::Foundation::IAsyncAction RunIdleAsync(
                winrt::Windows::UI::Core::IdleDispatchedHandler const &) { throw winrt::hresult_not_implemented(); }
            static winrt::Windows::Foundation::IAsyncOperation<bool> Done() { co_return true; }
            winrt::Windows::Foundation::IAsyncOperation<bool> TryRunAsync(winrt::Windows::UI::Core::CoreDispatcherPriority,
                winrt::Windows::UI::Core::DispatchedHandler const &handler)
            {
                handler();
                return Done();
            }
            winrt::Windows::Foundation::IAsyncOperation<bool> TryRunIdleAsync(
                winrt::Windows::UI::Core::IdleDispatchedHandler const &) { throw winrt::hresult_not_implemented(); }
        };
        struct FailureTarget : winrt::implements<FailureTarget, winrt::Windows::Foundation::IStringable>
        {
            winrt::handle done{CreateEvent(nullptr, TRUE, FALSE, nullptr)};
            MathCoordinatorHandle math;
            DiagramCoordinatorHandle diagram;
            std::atomic<bool> terminal{};
            uint64_t attempt{};
            winrt::hstring ToString() const { return L"Coordinator failure publication"; }
            static void Math(winrt::Windows::Foundation::IInspectable const &target, MathPresentationHandle const &payload) noexcept
            {
                auto self = winrt::get_self<FailureTarget>(target.as<winrt::Windows::Foundation::IStringable>());
                self->terminal.store(payload && payload->State() == NativeMathState::Failed &&
                    payload->Status() == NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE &&
                    self->math->AttemptStates(self->attempt)[0] == payload->State() &&
                    self->math->PendingCountForDiagnostics() == 0, std::memory_order_release);
                SetEvent(self->done.get());
            }
            static void Diagram(winrt::Windows::Foundation::IInspectable const &target, DiagramPresentationHandle const &payload) noexcept
            {
                auto self = winrt::get_self<FailureTarget>(target.as<winrt::Windows::Foundation::IStringable>());
                self->terminal.store(payload && payload->State() == NativeDiagramState::Failed &&
                    payload->Status() == NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE &&
                    self->diagram->AttemptStates(self->attempt)[0] == payload->State() &&
                    self->diagram->PendingCountForDiagnostics() == 0, std::memory_order_release);
                SetEvent(self->done.get());
            }
        };
        void CheckMathFailurePublication(notepads_markdown_core_model const *model, NativeMathKey key, ID2D1Factory *factory)
        {
            auto target = winrt::make_self<FailureTarget>();
            if (!target->done)
                winrt::throw_last_error();
            auto baseline = NativeMathWorker::LiveNodes();
            NativeMathDeliverySink sink;
            sink.target = winrt::make_weak(target.as<winrt::Windows::Foundation::IInspectable>());
            sink.dispatcher = winrt::make<InlineDispatcher>().as<winrt::Windows::UI::Core::CoreDispatcher>();
            sink.invoke = FailureTarget::Math;
            target->math = NativeMathCoordinator::Create(sink);
            target->attempt = key.attempt;
            target->math->BeginSource(key.attempt);
            // A real successful job fails the native provenance gate after join.
            // Preparation still captures the actual accepted model identity.
            ++key.identity.revision;
            auto error = Error();
            auto requested = target->math->Request(model, key, 0, factory, &error);
            auto waited = WaitForSingleObjectEx(target->done.get(), 15000, FALSE);
            target->math->CancelAll();
            auto until = GetTickCount64() + 5000;
            while (NativeMathWorker::LiveNodes() != baseline && GetTickCount64() < until)
                Sleep(1);
            if (requested != NativeMathState::Pending || waited != WAIT_OBJECT_0 ||
                !target->terminal.load(std::memory_order_acquire) || NativeMathWorker::LiveNodes() != baseline)
                winrt::throw_hresult(E_UNEXPECTED);
        }
        void CheckDeliveryReceipts()
        {
            struct State { uint32_t settled{}, released{}, storage{}; bool accepted{}; };
            auto make = [](State &state)
            {
                return DerivedDeliveryReceipt(&state, 17, 3,
                    [](void *owner, uint64_t attempt, uint32_t ordinal, bool accepted) noexcept
                    {
                        auto &state = *static_cast<State *>(owner);
                        ++state.settled;
                        state.accepted = accepted && attempt == 17 && ordinal == 3;
                    },
                    [](void *owner) noexcept { ++static_cast<State *>(owner)->released; });
            };
            State accepted;
            {
                auto receipt = make(accepted);
                auto moved = std::move(receipt);
                moved.Complete(true);
                moved.Complete(false);
            }
            State dropped;
            { auto receipt = make(dropped); }
            State mathFailed;
            auto math = NativeMathTransport::Post({}, {}, make(mathFailed));
            State diagramFailed;
            auto diagram = NativeDiagramTransport::PostEmbedded({}, {}, nullptr, 0, &diagramFailed,
                [](void *owner) noexcept { ++static_cast<State *>(owner)->storage; }, make(diagramFailed));
            State mathEmbeddedFailed;
            auto mathEmbedded = NativeMathTransport::PostEmbedded({}, {}, nullptr, 0, &mathEmbeddedFailed,
                [](void *owner) noexcept { ++static_cast<State *>(owner)->storage; });
            if (accepted.settled != 1 || accepted.released != 1 || !accepted.accepted ||
                dropped.settled != 1 || dropped.released != 1 || dropped.accepted ||
                SUCCEEDED(math) || mathFailed.settled != 1 || mathFailed.released != 1 || mathFailed.accepted ||
                SUCCEEDED(diagram) || diagramFailed.settled != 1 || diagramFailed.released != 1 ||
                diagramFailed.storage != 1 || diagramFailed.accepted ||
                SUCCEEDED(mathEmbedded) || mathEmbeddedFailed.storage != 1)
                winrt::throw_hresult(E_UNEXPECTED);
        }
    }
    Results Run(notepads_markdown_core_model const *model, NativeMathKey const &key)
    {
        CheckDeliveryReceipts();
        Results result;
        winrt::com_ptr<ID2D1Factory> factory;
        winrt::check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, factory.put()));
        auto error = Error();
        CapacityFixture capacity;
        for (auto &control : capacity.controls)
            if (control.Initialize(&error) != NOTEPADS_MARKDOWN_CORE_OK)
                throw MathMemoryFailure{error.status, error};
        auto baseline = Usage(capacity.controls[0]);
        if (baseline.process.jobs || baseline.process.residents)
            throw MathMemoryFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        result.queuedMetadataBeforeThread = true;
        for (uint32_t group = 0; group < CapacityFixture::FullControls; ++group)
        {
            for (uint32_t ordinal = 0; ordinal < NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT; ++ordinal)
            {
                auto status =
                    NativeMathWorker::Prepare(capacity.controls[group].Get(), model, key, ordinal, factory.get(),
                                              &capacity, NeverCalled, &capacity.nodes[group][ordinal], &error);
                if (status != NOTEPADS_MARKDOWN_CORE_OK)
                    throw MathMemoryFailure{status, error};
            }
            auto usage = Usage(capacity.controls[group]);
            result.controlResident128 = usage.control.residents == NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT;
            if (!result.controlResident128)
                throw MathMemoryFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
            result.queuedMetadataBeforeThread =
                result.queuedMetadataBeforeThread && usage.control.jobs == 0 &&
                usage.control.metadata_bytes > baseline.control.metadata_bytes;
            if (group == 0)
            {
                NativeMathWorker::PreparedNode *extra{};
                auto denied = NativeMathWorker::Prepare(capacity.controls[0].Get(), model, key,
                                                         NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT, factory.get(),
                                                         &capacity, NeverCalled, &extra, &error);
                auto refused = Usage(capacity.controls[0]);
                result.rejected129And257 =
                    denied == NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT && !extra && error.limit_kind == 108 &&
                    error.requested == NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT + 1 &&
                    error.limit == NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT && SameRow(refused.control, usage.control) &&
                    SameRow(refused.process, usage.process);
                if (extra)
                    NativeMathWorker::DestroyQueued(extra);
            }
        }
        auto filled = Usage(capacity.controls[0]);
        result.processResident256 =
            filled.process.residents == NOTEPADS_MARKDOWN_DERIVED_PROCESS_RESIDENT_LIMIT && filled.process.jobs == 0;
        NativeMathWorker::PreparedNode *extra{};
        auto &overflow = capacity.controls[CapacityFixture::FullControls];
        auto overflowBefore = Usage(overflow);
        auto deniedProcess = NativeMathWorker::Prepare(overflow.Get(), model, key, 0, factory.get(), &capacity,
                                                       NeverCalled, &extra, &error);
        auto processRefused = Usage(overflow);
        result.rejected129And257 =
            result.rejected129And257 && deniedProcess == NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT && !extra && error.limit_kind == 107 &&
            error.requested == NOTEPADS_MARKDOWN_DERIVED_PROCESS_RESIDENT_LIMIT + 1 &&
            error.limit == NOTEPADS_MARKDOWN_DERIVED_PROCESS_RESIDENT_LIMIT &&
            SameRow(processRefused.control, overflowBefore.control) &&
            SameRow(processRefused.process, overflowBefore.process);
        if (extra)
            NativeMathWorker::DestroyQueued(extra);
        NativeMathWorker::DestroyQueued(std::exchange(capacity.nodes[0][0], nullptr));
        auto reopened = NativeMathWorker::Prepare(overflow.Get(), model, key, 0, factory.get(), &capacity,
                                                  NeverCalled, &extra, &error);
        auto recovered = Usage(overflow);
        result.residentLastFreeReopensCapacity =
            reopened == NOTEPADS_MARKDOWN_CORE_OK && extra && recovered.control.residents == 1 &&
            recovered.process.residents == NOTEPADS_MARKDOWN_DERIVED_PROCESS_RESIDENT_LIMIT && recovered.process.jobs == 0;
        if (extra)
            NativeMathWorker::DestroyQueued(std::exchange(extra, nullptr));
        for (auto &row : capacity.nodes)
            for (auto &node : row)
                NativeMathWorker::DestroyQueued(std::exchange(node, nullptr));
        auto emptied = Usage(capacity.controls[0]);
        result.queuedRealFreeRestoresBaseline =
            SameRow(emptied.control, baseline.control) && SameRow(emptied.process, baseline.process);
        ThreadFixture fixture;
        if (!fixture.done)
            winrt::throw_last_error();
        NativeMathWorker::PreparedNode *node{};
        auto status = NativeMathWorker::Prepare(capacity.controls[0].Get(), model, key, 0, factory.get(), &fixture,
                                                ThreadFixture::Complete, &node, &error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
            throw MathMemoryFailure{status, error};
        auto started = NativeMathWorker::StartPrepared(node);
        if (started.status != NOTEPADS_MARKDOWN_CORE_OK)
        {
            NativeMathWorker::DestroyQueued(node);
            throw MathMemoryFailure{started.status};
        }
        auto wait = WaitForSingleObjectEx(fixture.done.get(), 15000, FALSE);
        if (wait != WAIT_OBJECT_0)
        {
            NativeMathWorker::CancelPrepared(node);
            // The stack fixture must outlive its actual callback, including a
            // slow device. This diagnostic is never invoked from the UI thread.
            (void)WaitForSingleObjectEx(fixture.done.get(), INFINITE, FALSE);
        }
        result.preparedThreadReallySignaled = fixture.signaled;
        auto terminal = Usage(capacity.controls[0]);
        result.terminalRetiresJobKeepsResident =
            fixture.retired && terminal.control.jobs == 0 && terminal.control.residents == 1;
        NativeMathWorker::ReleasePrepared(node);
        auto until = GetTickCount64() + 5000;
        do
        {
            emptied = Usage(capacity.controls[0]);
            if (SameRow(emptied.control, baseline.control))
                break;
            Sleep(1);
        } while (GetTickCount64() < until);
        result.terminalLastFreeRestoresBaseline =
            SameRow(emptied.control, baseline.control) && SameRow(emptied.process, baseline.process);
        auto coordinator = NativeMathCoordinator::Create({});
        coordinator->BeginSource(key.attempt);
        auto first = coordinator->Request(nullptr, key, 0, factory.get(), &error);
        auto second = coordinator->Request(model, key, 0, factory.get(), &error);
        result.prepareFailureDeduplicated =
            first == NativeMathState::Failed && second == first && coordinator->AttemptState(key.attempt, 0) == first;
        constexpr uint32_t middleOrdinal = NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT / 2;
        constexpr uint32_t lastOrdinal = NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT - 1;
        auto middle = coordinator->Request(nullptr, key, middleOrdinal, factory.get(), &error);
        auto last = coordinator->Request(nullptr, key, lastOrdinal, factory.get(), &error);
        auto states = coordinator->AttemptStates(key.attempt);
        result.expandedOrdinalsTracked =
            middle == NativeMathState::Failed && last == NativeMathState::Failed &&
            states[0] == first && states[middleOrdinal] == middle && states[lastOrdinal] == last &&
            states[middleOrdinal - 1] == NativeMathState::None && states[lastOrdinal - 1] == NativeMathState::None;
        result.oversizedOrdinalLimited =
            coordinator->Request(model, key, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT, factory.get(), &error) ==
                NativeMathState::Limited &&
            coordinator->AttemptState(key.attempt, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT) == NativeMathState::Limited;
        coordinator->BeginSource(key.attempt + 1);
        auto retired = coordinator->AttemptStates(key.attempt);
        auto current = coordinator->AttemptStates(key.attempt + 1);
        result.expandedOrdinalsTracked = result.expandedOrdinalsTracked &&
            std::all_of(retired.begin(), retired.end(), [](auto state) { return state == NativeMathState::Limited; }) &&
            std::all_of(current.begin(), current.end(), [](auto state) { return state == NativeMathState::None; });
        coordinator->CancelAll();
        CheckMathFailurePublication(model, key, factory.get());
        return result;
    }
    void CheckDiagramFailurePublication(notepads_markdown_core_model const *model, NativeDiagramKey key,
                                        notepads_markdown_diagram_font_capability const &fonts,
                                        notepads_markdown_diagram_style const &style)
    {
        auto target = winrt::make_self<FailureTarget>();
        if (!target->done)
            winrt::throw_last_error();
        auto baseline = NativeDiagramWorker::LiveNodes();
        NativeDiagramDeliverySink sink;
        sink.target = winrt::make_weak(target.as<winrt::Windows::Foundation::IInspectable>());
        sink.dispatcher = winrt::make<InlineDispatcher>().as<winrt::Windows::UI::Core::CoreDispatcher>();
        sink.invoke = FailureTarget::Diagram;
        target->math = NativeMathCoordinator::Create({});
        target->diagram = NativeDiagramCoordinator::Create(target->math, model, sink);
        target->attempt = key.attempt;
        target->diagram->BeginSource(key.attempt);
        ++key.identity.revision;
        auto error = Error();
        auto requested = target->diagram->Request(model, key, 0, fonts, style, &error);
        auto waited = WaitForSingleObjectEx(target->done.get(), 15000, FALSE);
        target->diagram->CancelAll();
        auto until = GetTickCount64() + 5000;
        while (NativeDiagramWorker::LiveNodes() != baseline && GetTickCount64() < until)
            Sleep(1);
        if (requested != NativeDiagramState::Pending || waited != WAIT_OBJECT_0 ||
            !target->terminal.load(std::memory_order_acquire) || NativeDiagramWorker::LiveNodes() != baseline)
            winrt::throw_hresult(E_UNEXPECTED);
        auto rejected = NativeDiagramCoordinator::Create(target->math, model, {});
        rejected->BeginSource(key.attempt);
        requested = rejected->Request(model, key, 0, fonts, style, &error);
        until = GetTickCount64() + 15000;
        while ((rejected->AttemptState(key.attempt, 0) == NativeDiagramState::Pending ||
                NativeDiagramWorker::LiveNodes() != baseline) && GetTickCount64() < until)
            Sleep(1);
        rejected->CancelAll();
        if (requested != NativeDiagramState::Pending ||
            rejected->AttemptState(key.attempt, 0) != NativeDiagramState::Unavailable ||
            rejected->PendingCountForDiagnostics() != 0 || NativeDiagramWorker::LiveNodes() != baseline)
            winrt::throw_hresult(E_UNEXPECTED);
    }
}
