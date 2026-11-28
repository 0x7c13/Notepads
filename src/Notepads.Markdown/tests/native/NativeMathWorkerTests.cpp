// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeMathWorkerTests.h"
#include "NativeMathMemoryTests.h"
#include "NativeMathArtifactTests.h"
#include <chrono>
#include <thread>

namespace Notepads::Markdown::Native::MathWorkerTests
{
    namespace
    {
        void Require(bool value, wchar_t const *detail)
        {
            if (!value)
                throw winrt::hresult_error(E_FAIL, detail);
        }

        notepads_markdown_core_error Error() noexcept
        {
            return {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        }
        notepads_markdown_math_usage Usage(notepads_markdown_math_control *control)
        {
            notepads_markdown_math_usage usage{};
            usage.header = {NOTEPADS_MARKDOWN_MATH_ABI_VERSION, sizeof(usage)};
            auto error = Error();
            Require(notepads_markdown_math_control_usage(control, &usage, &error) == NOTEPADS_MARKDOWN_CORE_OK, L"real math policy usage");
            return usage;
        }

        struct Job
        {
            notepads_markdown_math_job *value{};
            ~Job()
            {
                if (value)
                {
                    auto error = Error();
                    (void)notepads_markdown_math_job_release(value, &error);
                }
            }
            void Release()
            {
                auto error = Error();
                auto current = value;
                value = nullptr;
                Require(notepads_markdown_math_job_release(current, &error) == NOTEPADS_MARKDOWN_CORE_OK, L"release real caller math owner");
            }
        };

        struct CompletionFixture
        {
            winrt::handle done{CreateEventExW(nullptr, nullptr, CREATE_EVENT_MANUAL_RESET, EVENT_ALL_ACCESS)};
            winrt::handle reached{CreateEventExW(nullptr, nullptr, CREATE_EVENT_MANUAL_RESET, EVENT_ALL_ACCESS)};
            winrt::handle permit{CreateEventExW(nullptr, nullptr, CREATE_EVENT_MANUAL_RESET, EVENT_ALL_ACCESS)};
            NativeMathWorkerResult result;
            std::atomic<bool> called{};
            notepads_markdown_math_control *control{};
            bool checkUnavailable{};
            bool checkComputed{};
            int32_t callbackCheckStatus{NOTEPADS_MARKDOWN_CORE_OK};
            winrt::hstring callbackCheckDetail;
            MathArtifactTests::Results artifact;

            CompletionFixture()
            {
                Require(done && reached && permit, L"fixed test events created");
            }
            ~CompletionFixture()
            {
                // Failure rescue releases only this fixed tail gate. The
                // fixture remains alive until the worker has really retired.
                SetEvent(permit.get());
                if (started)
                {
                    // This failure-only background cleanup keeps the fixed
                    // stack fixture alive until its real callback has finished.
                    // It cannot turn a timed-out assertion into a passing one.
                    (void)WaitForSingleObjectEx(done.get(), INFINITE, FALSE);
                    while (NativeMathWorker::LiveNodes() != 0)
                        std::this_thread::sleep_for(std::chrono::milliseconds(1));
                }
            }
            bool started{};
            static void Complete(void *context, notepads_markdown_math_job *job, NativeMathWorkerResult const &result) noexcept
            {
                auto self = static_cast<CompletionFixture *>(context);
                self->result = result;
                try
                {
                    if (self->checkUnavailable)
                        (void)MathMemoryTests::RunUnavailable(job, self->control);
                    if (self->checkComputed)
                        self->artifact = MathArtifactTests::RunComputed(job, self->control);
                }
                catch (winrt::hresult_error const &failure)
                {
                    self->callbackCheckStatus = failure.code().value;
                    self->callbackCheckDetail = failure.message();
                }
                catch (...)
                {
                    self->callbackCheckStatus = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                }
                self->called.store(true, std::memory_order_release);
                SetEvent(self->done.get());
            }
            NativeMathWorkerTestHooks Hooks() const noexcept
            {
                return {reached.get(), permit.get(), 0};
            }
        };

        void Prepare(Job &job, notepads_markdown_math_control *control, notepads_markdown_core_model const *model, uint32_t object)
        {
            auto error = Error();
            Require(notepads_markdown_math_job_prepare(control, model, object, &job.value, &error) == NOTEPADS_MARKDOWN_CORE_OK,
                    L"prepare exact accepted math occurrence");
        }

        void AwaitRetirement(notepads_markdown_math_control *control, notepads_markdown_math_usage const &baseline)
        {
            auto end = std::chrono::steady_clock::now() + std::chrono::seconds(3);
            do
            {
                auto usage = Usage(control);
                if (NativeMathWorker::LiveNodes() == 0 && usage.process.bytes == baseline.process.bytes &&
                    usage.control.bytes == baseline.control.bytes && usage.process.jobs == baseline.process.jobs &&
                    usage.control.jobs == baseline.control.jobs &&
                    usage.process.residents == baseline.process.residents &&
                    usage.control.residents == baseline.control.residents)
                    return;
                std::this_thread::sleep_for(std::chrono::milliseconds(1));
            } while (std::chrono::steady_clock::now() < end);
            Require(false, L"all worker known capacity and actual-job retire to the real policy baseline");
        }
    }

    Results Run(notepads_markdown_core_model const *model, uint32_t object)
    {
        Results results{};
        NativeMathControl control, otherControl;
        auto error = Error();
        Require(control.Initialize(&error) == NOTEPADS_MARKDOWN_CORE_OK && otherControl.Initialize(&error) == NOTEPADS_MARKDOWN_CORE_OK,
                L"two native controls share module runtime");
        auto baseline = Usage(control.Get());
        {
            Job job, second;
            Prepare(job, control.Get(), model, object);
            Require(notepads_markdown_math_job_activate(job.value, &error) == NOTEPADS_MARKDOWN_CORE_OK, L"activate real 64MiB job");
            Prepare(second, control.Get(), model, object);
            auto denied = notepads_markdown_math_job_activate(second.value, &error);
            Require(denied == NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT, L"same control cannot activate a second actual worker");
            second.Release();
            results.secondControlJobRejected = true;
            CompletionFixture fixture;
            // The fixed worker gate record is part of the prepaid 4096-byte
            // WorkerNode. Event owners here are fixed test infrastructure; no
            // arbitrary worker observer or uncharged dynamic record is added.
            auto hooks = fixture.Hooks();
            hooks.forceInsufficientHeadroom = true;
            fixture.control = control.Get();
            fixture.checkUnavailable = true;
            auto start = NativeMathWorker::StartForTesting(job.value, CompletionFixture::Complete, &fixture, hooks);
            Require(start.status == NOTEPADS_MARKDOWN_CORE_OK, L"create real candidate math thread and registered reaper");
            fixture.started = true;
            Require(WaitForSingleObjectEx(fixture.reached.get(), 3000, FALSE) == WAIT_OBJECT_0,
                    L"real math run returned before the held thread tail");
            Require(!fixture.called.load(std::memory_order_acquire), L"run return never completes the owner");
            auto held = Usage(control.Get());
            auto shared = Usage(otherControl.Get());
            Require(held.control.jobs == 1 && held.process.jobs == baseline.process.jobs + 1 &&
                        held.control.bytes >= baseline.control.bytes + 64ull * 1024 * 1024 &&
                        shared.process.bytes == held.process.bytes && shared.process.jobs == held.process.jobs,
                    L"tail still owns source/process/control 64MiB and the shared actual-job slot");
            results.tailHeldFullGrant = true;
            SetEvent(fixture.permit.get());
            Require(WaitForSingleObjectEx(fixture.done.get(), 3000, FALSE) == WAIT_OBJECT_0,
                    L"completion follows actual worker termination");
            auto const &observed = fixture.result;
            Require(observed.status == NOTEPADS_MARKDOWN_MATH_STACK_UNAVAILABLE,
                    L"insufficient trusted headroom rejects before parser");
            Require(observed.stack.measured && observed.stack.withinStackBudget && observed.stack.headroom > 0 &&
                        observed.stack.committedBytes <= 2ull * 1024 * 1024 &&
                        observed.stack.threadId != GetCurrentThreadId() && observed.stack.threadSignaled &&
                        observed.tailWaitResult == WAIT_OBJECT_0,
                    L"new thread measured real committed stack and completed only after handle signaled");
            Require(fixture.callbackCheckStatus == NOTEPADS_MARKDOWN_CORE_OK, L"Unavailable geometry keeps the real grant unchanged");
            results.stack = observed.stack;
            results.insufficientHeadroomRejected = true;
            results.completionAfterSignaled = true;
            job.Release();
            AwaitRetirement(control.Get(), baseline);
        }
        {
            Job job;
            Prepare(job, control.Get(), model, object);
            Require(notepads_markdown_math_job_activate(job.value, &error) == NOTEPADS_MARKDOWN_CORE_OK, L"activate cancel fixture");
            Require(notepads_markdown_math_job_cancel(job.value, &error) == NOTEPADS_MARKDOWN_CORE_OK, L"cancel disjoint real atomic owner");
            CompletionFixture fixture;
            auto start = NativeMathWorker::Start(job.value, CompletionFixture::Complete, &fixture);
            Require(start.status == NOTEPADS_MARKDOWN_CORE_OK, L"cancelled active job still starts its real retirement worker");
            fixture.started = true;
            Require(WaitForSingleObjectEx(fixture.done.get(), 3000, FALSE) == WAIT_OBJECT_0,
                    L"cancel does not replace thread termination");
            Require(fixture.result.status == NOTEPADS_MARKDOWN_CORE_CANCELLED && fixture.result.stack.threadSignaled,
                    L"typed cancellation after actual exit");
            job.Release();
            AwaitRetirement(control.Get(), baseline);
            results.cancellationRetired = true;
        }
        {
            Job job;
            Prepare(job, control.Get(), model, object);
            Require(notepads_markdown_math_job_activate(job.value, &error) == NOTEPADS_MARKDOWN_CORE_OK, L"activate create-failure fixture");
            CompletionFixture fixture;
            auto hooks = fixture.Hooks();
            hooks.simulatedCreateThreadError = ERROR_NOT_ENOUGH_MEMORY;
            auto start = NativeMathWorker::StartForTesting(job.value, CompletionFixture::Complete, &fixture, hooks);
            Require(start.status == NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY && start.win32Error == ERROR_NOT_ENOUGH_MEMORY &&
                        NativeMathWorker::LiveNodes() == 0 && !fixture.called.load(std::memory_order_acquire),
                    L"injected failure at CreateThread site frees the node without callback");
            Require(Usage(control.Get()).control.jobs == 1 && notepads_markdown_math_job_state(job.value) == NOTEPADS_MARKDOWN_MATH_ACTIVATED,
                    L"create failure leaves caller's real grant and slot intact");
            auto retry = NativeMathWorker::StartForTesting(job.value, CompletionFixture::Complete, &fixture, hooks);
            Require(retry.status == NOTEPADS_MARKDOWN_CORE_INVALID_STATE && NativeMathWorker::LiveNodes() == 0 &&
                        !fixture.called.load(std::memory_order_acquire),
                    L"creation failure consumes the single prepaid node/thread start; retry needs a fresh job");
            job.Release();
            AwaitRetirement(control.Get(), baseline);
            results.injectedCreateFailureRetired = true;
        }
        {
            Job job;
            Prepare(job, control.Get(), model, object);
            Require(notepads_markdown_math_job_activate(job.value, &error) == NOTEPADS_MARKDOWN_CORE_OK, L"activate real formula producer");
            CompletionFixture fixture;
            fixture.control = control.Get();
            fixture.checkComputed = true;
            auto start = NativeMathWorker::Start(job.value, CompletionFixture::Complete, &fixture);
            Require(start.status == NOTEPADS_MARKDOWN_CORE_OK, L"start normal measured production formula worker");
            fixture.started = true;
            Require(WaitForSingleObjectEx(fixture.done.get(), 8000, FALSE) == WAIT_OBJECT_0,
                    L"real formula and funded native geometry complete after thread signaled");
            if (fixture.callbackCheckStatus != NOTEPADS_MARKDOWN_CORE_OK)
                throw winrt::hresult_error(fixture.callbackCheckStatus, fixture.callbackCheckDetail);
            Require(fixture.result.status == NOTEPADS_MARKDOWN_CORE_OK && fixture.result.stack.threadSignaled &&
                        fixture.callbackCheckStatus == NOTEPADS_MARKDOWN_CORE_OK,
                    L"actual production formula, funded geometry, arena and last weak owner controls");
            job.Release();
            AwaitRetirement(control.Get(), baseline);
            results.computedArtifactRetired = true;
            results.actualRetained = fixture.artifact.actualRetained;
            results.commands = fixture.artifact.commands;
            results.primitives = fixture.artifact.primitives;
            results.advance = fixture.artifact.advance;
        }
        Require(NativeMathWorker::ReaperWaitFailures() == 0, L"no reaper wait failure during these checks");
        Require(NativeMathWorker::NodeFreeFailures() == 0, L"all observed native node HeapFree calls succeeded");
        results.baselineRestored = true;
        return results;
    }
}
