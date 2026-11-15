// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "CoreControllerTests.h"
#include "Rendering/NativeDocumentRenderer.h"
#include <chrono>
#include <cstring>
#include <string_view>
#include <thread>
#include <utility>

namespace Notepads::Markdown::Native::CoreControllerTests
{
    namespace
    {
        template <typename T> T Pod() noexcept
        {
            T value{};
            value.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, static_cast<uint32_t>(sizeof(T))};
            return value;
        }

        struct Failure
        {
            wchar_t const *detail;
            int32_t status;
            bool cancellationSensitive{};
            bool cleanupOnly{};
        };

        void Require(bool value, wchar_t const *detail, int32_t status = NOTEPADS_MARKDOWN_CORE_INVALID_STATE)
        {
            if (!value)
            {
                throw Failure{detail, status};
            }
        }

        template <typename Predicate> bool Until(Predicate predicate, uint32_t milliseconds = 2000)
        {
            auto end = std::chrono::steady_clock::now() + std::chrono::milliseconds(milliseconds);
            do
            {
                if (predicate())
                {
                    return true;
                }
                // Test observation only. The production driver uses its own event.
                std::this_thread::sleep_for(std::chrono::milliseconds(1));
            } while (std::chrono::steady_clock::now() < end);
            return predicate();
        }

        template <typename Predicate>
        bool ObserveUntil(std::shared_ptr<CoreValidation::Context> const &context, Predicate predicate)
        {
            return Until(
                [&]
                {
                    context->Arbitrate();
                    return predicate();
                });
        }

        bool Reached(std::shared_ptr<CoreValidation::Context> const &context,
                     std::shared_ptr<CoreControllerTestHooks> const &hooks, CoreTestPoint point)
        {
            return ObserveUntil(context, [&] { return hooks->WaitReached(point, 1); });
        }

        notepads_markdown_core_snapshot Descriptor(uint64_t revision) noexcept
        {
            auto value = Pod<notepads_markdown_core_snapshot>();
            value.source.document[0] = 0x71;
            value.source.document[15] = 0x19;
            value.source.revision = revision;
            value.profile = NOTEPADS_MARKDOWN_CORE_C0;
            return value;
        }

        std::span<uint8_t const> Bytes(std::string_view value) noexcept
        {
            return {reinterpret_cast<uint8_t const *>(value.data()), value.size()};
        }

        void Accepted(CoreSubmitResult const &result)
        {
            if (result.status != NOTEPADS_MARKDOWN_CORE_OK)
            {
                throw Failure{L"real source prepare succeeded", result.status,
                              result.status == NOTEPADS_MARKDOWN_CORE_CANCELLED || result.status == NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
            }
            Require(result.sourceState.state_valid == 1 && result.sourceState.has_ready == 1,
                    L"prepare returned current State and this request's Ready permission");
        }

        void MainEquals(CoreModelPin const &pin, std::string_view expected,
                        std::shared_ptr<CoreValidation::Context> const &context)
        {
            Require(static_cast<bool>(pin), L"retained real model exists");
            auto error = Pod<notepads_markdown_core_error>();
            notepads_markdown_core_cancel *cancel{};
            notepads_markdown_core_stream *stream{};
            auto status = notepads_markdown_core_model_cancel_create(pin.Get(), &cancel, &error);
            if (status == NOTEPADS_MARKDOWN_CORE_OK)
            {
                status = notepads_markdown_core_stream_create(pin.Get(), cancel, &stream, &error);
            }
            std::array<uint8_t, 128> bytes{};
            auto read = Pod<notepads_markdown_core_read_result>();
            if (status == NOTEPADS_MARKDOWN_CORE_OK)
            {
                status = notepads_markdown_core_stream_read(stream, bytes.data(), bytes.size(), &read, &error);
            }
            auto streamCleanup = stream ? notepads_markdown_core_stream_destroy(stream) : NOTEPADS_MARKDOWN_CORE_OK;
            auto cancelCleanup = cancel ? notepads_markdown_core_cancel_destroy(cancel) : NOTEPADS_MARKDOWN_CORE_OK;
            context->RecordCleanup(streamCleanup, L"controller retained stream destroy");
            context->RecordCleanup(cancelCleanup, L"controller retained stream cancel destroy");
            Require(status == NOTEPADS_MARKDOWN_CORE_OK, L"retained model's real stream remains readable", status);
            if (streamCleanup != NOTEPADS_MARKDOWN_CORE_OK || cancelCleanup != NOTEPADS_MARKDOWN_CORE_OK)
            {
                throw Failure{L"retained stream/control cleanup",
                              streamCleanup != NOTEPADS_MARKDOWN_CORE_OK ? streamCleanup : cancelCleanup, false, true};
            }
            Require(read.done == 1 && read.written == expected.size() &&
                        std::memcmp(bytes.data(), expected.data(), expected.size()) == 0,
                    L"actual model text contains every committed edit");
        }

        struct DrainRace
        {
            std::atomic<uint32_t> failedStep{};
            std::atomic<bool> rescueUsed{};
            std::atomic<bool> rescueDone{};
            std::atomic<bool> drainStarted{};
            std::atomic<bool> drainDone{};
            std::atomic<bool> coordinatorDone{};
            CoreDrainResult drain;

            void Fail(uint32_t step) noexcept
            {
                uint32_t expected{};
                failedStep.compare_exchange_strong(expected, step);
            }
        };

        template <typename Predicate>
        bool RaceFence(std::shared_ptr<DrainRace> const &race, std::shared_ptr<CoreValidation::Context> const &context,
                       Predicate predicate)
        {
            return Until([&]
                         { return race->failedStep.load() != 0 || context->CancellationRequested() || predicate(); }) &&
                   race->failedStep.load() == 0 && !context->CancellationRequested();
        }

        // Failure-only watchdog. The normal proof never calls Stop here. In the
        // reproduced old race Drain is held after stealing a wake, so this one
        // rescue Stop cannot itself be consumed by that suspended observer.
        void RescueRace(std::shared_ptr<DrainRace> const &race, std::shared_ptr<CoreController> const &controller,
                        std::shared_ptr<CoreControllerTestHooks> const &hooks) noexcept
        {
            if (race->rescueUsed.exchange(true))
            {
                return;
            }
            race->Fail(99);
            hooks->Release(CoreTestPoint::DriverBeforeIdleWait);
            hooks->Release(CoreTestPoint::DriverBeforeSubmitWait);
            hooks->Release(CoreTestPoint::SubmitBeforeEndBorrow);
            controller->Stop();
            // Keep Drain's before/after-wait gates held during this bounded
            // cleanup attempt. A failure remains a failure even if rescue works.
            Until([&] { return controller->Snapshot().coreDrained; });
            hooks->ReleaseAll();
            race->rescueDone.store(true, std::memory_order_release);
        }

        bool CoordinateIdleDrain(std::shared_ptr<DrainRace> const &race,
                                 std::shared_ptr<CoreController> const &controller,
                                 std::shared_ptr<CoreControllerTestHooks> const &hooks,
                                 std::shared_ptr<CoreValidation::Context> const &context)
        {
            if (!RaceFence(race, context, [&] { return hooks->WaitReached(CoreTestPoint::DrainAfterWait, 1); }))
            {
                race->Fail(1);
                RescueRace(race, controller, hooks);
                return false;
            }
            hooks->Release(CoreTestPoint::DriverBeforeIdleWait);
            // Drain remains suspended after its first real wait. Only its own
            // initial Stop has signaled production events; no UI/Submit follows.
            if (!RaceFence(race, context, [&] { return controller->Snapshot().coreDrained; }))
            {
                race->Fail(2);
                RescueRace(race, controller, hooks);
                return false;
            }
            hooks->Release(CoreTestPoint::DrainAfterWait);
            race->coordinatorDone.store(true, std::memory_order_release);
            return true;
        }

        void CoordinateSubmitDrain(std::shared_ptr<DrainRace> const &race,
                                   std::shared_ptr<CoreController> const &controller,
                                   std::shared_ptr<CoreControllerTestHooks> const &hooks,
                                   std::shared_ptr<CoreValidation::Context> const &context) noexcept
        {
            auto fence = [&](CoreTestPoint point, uint32_t step)
            {
                if (!RaceFence(race, context, [&] { return hooks->WaitReached(point, 1); }))
                {
                    race->Fail(step);
                    return false;
                }
                return true;
            };
            // All Submit signals precede its held EndBorrow. Drain is held after
            // Stop but before its first wait, so the driver consumes the old
            // coalesced wake before it reaches the closed/Submit-borrow gate.
            bool coordinated =
                fence(CoreTestPoint::SubmitBeforeEndBorrow, 3) && fence(CoreTestPoint::DrainBeforeWait, 4);
            if (coordinated)
            {
                hooks->Release(CoreTestPoint::DriverBeforeIdleWait);
                coordinated = fence(CoreTestPoint::DriverBeforeSubmitWait, 5);
            }
            if (coordinated)
            {
                hooks->Release(CoreTestPoint::SubmitBeforeEndBorrow);
                coordinated = RaceFence(race, context, [&] { return !controller->Snapshot().submitEntered; });
                if (!coordinated)
                {
                    race->Fail(7);
                }
            }
            if (coordinated)
            {
                // EndBorrow has now actually signaled. Hold the driver before
                // its INFINITE wait until Drain has consumed its own event.
                hooks->Release(CoreTestPoint::DrainBeforeWait);
                coordinated = fence(CoreTestPoint::DrainAfterWait, 8);
            }
            if (coordinated)
            {
                hooks->Release(CoreTestPoint::DriverBeforeSubmitWait);
                coordinated = RaceFence(race, context, [&] { return controller->Snapshot().coreDrained; });
                if (!coordinated)
                {
                    race->Fail(9);
                }
            }
            if (!coordinated)
            {
                RescueRace(race, controller, hooks);
            }
            else
            {
                hooks->Release(CoreTestPoint::DrainAfterWait);
            }
            race->coordinatorDone.store(true, std::memory_order_release);
        }

        bool ObserveAuxiliary(winrt::Windows::Foundation::IAsyncAction &action,
                              std::shared_ptr<CoreValidation::Context> const &context)
        {
            if (!action)
            {
                return true;
            }
            try
            {
                if (!Until([&] { return action.Status() != winrt::Windows::Foundation::AsyncStatus::Started; }))
                {
                    return false;
                }
            }
            catch (...)
            {
                context->RecordException(std::current_exception(), L"controller race auxiliary Status");
                return false;
            }
            bool successful = true;
            try
            {
                action.GetResults();
            }
            catch (...)
            {
                context->RecordException(std::current_exception(), L"controller race auxiliary GetResults");
                successful = false;
            }
            try
            {
                action.Close();
            }
            catch (...)
            {
                context->RecordCleanup(winrt::to_hresult().value, L"controller race auxiliary Close");
                successful = false;
            }
            action = nullptr;
            return successful;
        }

        enum class Case
        {
            Edits,
            CancelWins,
            ClaimWins,
            PostingEarly,
            AckStarted,
            StopBeforeEntry,
            StopEntered,
            Retained,
            Utf16Capture,
            PreparingCancel,
            CancelBeforeEntry,
            RequestWeakTail,
            IdleDrainSignal,
            SubmitDrainSignal
        };

        wchar_t const *Name(Case value) noexcept
        {
            switch (value)
            {
            case Case::Edits:
                return L"controller_abc_edits_pending";
            case Case::CancelWins:
                return L"controller_cancel_wins";
            case Case::ClaimWins:
                return L"controller_claim_wins";
            case Case::PostingEarly:
                return L"controller_posting_early_entry";
            case Case::AckStarted:
                return L"controller_ack_before_action_terminal";
            case Case::StopBeforeEntry:
                return L"controller_stop_before_entry";
            case Case::StopEntered:
                return L"controller_stop_entered";
            case Case::Retained:
                return L"controller_retained_pins_driver_tail";
            case Case::Utf16Capture:
                return L"controller_utf16_capture_current";
            case Case::PreparingCancel:
                return L"controller_preparing_cancel_transfer";
            case Case::CancelBeforeEntry:
                return L"controller_cancel_before_entry";
            case Case::RequestWeakTail:
                return L"controller_request_last_weak_tail";
            case Case::IdleDrainSignal:
                return L"controller_idle_drain_signal";
            case Case::SubmitDrainSignal:
                return L"controller_submit_drain_signal";
            }
            return L"controller_unknown";
        }

        winrt::Windows::Foundation::IAsyncAction RunCase(winrt::Windows::UI::Core::CoreDispatcher dispatcher,
                                                         std::shared_ptr<Results> results,
                                                         std::shared_ptr<CoreValidation::Context> context, Case kind)
        {
            Require(dispatcher.HasThreadAccess(), L"controller test entered the real UI dispatcher");
            auto hooks = std::make_shared<CoreControllerTestHooks>();
            bool isDrainRace = kind == Case::IdleDrainSignal || kind == Case::SubmitDrainSignal;
            auto drainRace = isDrainRace ? std::make_shared<DrainRace>() : std::shared_ptr<DrainRace>{};
            if (isDrainRace)
            {
                // Arm before Create can start its one real production driver.
                hooks->Arm(CoreTestPoint::DriverBeforeIdleWait, 10000);
                hooks->Arm(CoreTestPoint::DrainAfterWait, 10000);
            }
            if (kind == Case::SubmitDrainSignal)
            {
                hooks->Arm(CoreTestPoint::DriverBeforeSubmitWait, 10000);
                hooks->Arm(CoreTestPoint::SubmitBeforeEndBorrow, 10000);
                hooks->Arm(CoreTestPoint::DrainBeforeWait, 10000);
            }
            std::shared_ptr<CoreController> controller = CoreController::Create(dispatcher, NOTEPADS_MARKDOWN_CORE_C0, hooks);
            auto lifetime = controller->Lifetime();
            struct Registration
            {
                std::shared_ptr<CoreValidation::Context> context;
                CoreController *controller{};
                ~Registration()
                {
                    Withdraw();
                }
                void Withdraw() noexcept
                {
                    if (controller != nullptr)
                    {
                        context->WithdrawController(std::exchange(controller, nullptr));
                    }
                }
            } registration{context};
            auto Check = [&](bool value, wchar_t const *detail, int32_t status = NOTEPADS_MARKDOWN_CORE_INVALID_STATE)
            {
                context->Arbitrate();
                Require(value, detail, status);
            };
            CoreModelPin retained;
            std::exception_ptr failure;
            winrt::Windows::Foundation::IAsyncAction auxiliary{nullptr};
            winrt::Windows::Foundation::IAsyncAction coordinator{nullptr};
            bool auxiliaryOwnsDrain = false;
            uint32_t observedRaceActions{};
            auto earlyCancelObserved = std::make_shared<std::atomic<bool>>(false);
            bool caseChecked = false;
            try
            {
                context->Arbitrate();
                context->PublishController(controller.get());
                registration.controller = controller.get();
                context->Arbitrate();
                switch (kind)
                {
                case Case::IdleDrainSignal:
                {
                    co_await winrt::resume_background();
                    Check(Reached(context, hooks, CoreTestPoint::DriverBeforeIdleWait),
                          L"real driver held immediately before its initial INFINITE idle wait");
                    auxiliary = winrt::Windows::System::Threading::ThreadPool::RunAsync(
                        [controller, drainRace](winrt::Windows::Foundation::IAsyncAction const &) noexcept
                        {
                            drainRace->drainStarted.store(true, std::memory_order_release);
                            drainRace->drain = controller->Drain();
                            drainRace->drainDone.store(true, std::memory_order_release);
                        });
                    auxiliaryOwnsDrain = true;
                    bool proof = CoordinateIdleDrain(drainRace, controller, hooks, context);
                    bool observed = ObserveAuxiliary(auxiliary, context);
                    if (observed)
                    {
                        ++observedRaceActions;
                    }
                    else
                    {
                        drainRace->Fail(10);
                        RescueRace(drainRace, controller, hooks);
                    }
                    Check(proof && observed && drainRace->failedStep.load() == 0 && !drainRace->rescueUsed.load() &&
                              !hooks->AnyTimedOut(),
                          L"idle close progressed within the fence without Submit/UI/extra Stop or rescue signal");
                    Check(drainRace->drainDone.load(std::memory_order_acquire) && drainRace->drain.coreDrained &&
                              drainRace->drain.driverExited,
                          L"separate CoreDrained and natural driver Status/GetResults observed by the real Drain");
                    caseChecked = true;
                    break;
                }
                case Case::SubmitDrainSignal:
                {
                    co_await winrt::resume_background();
                    Check(Reached(context, hooks, CoreTestPoint::DriverBeforeIdleWait),
                          L"initial driver wait held before any source request");
                    co_await winrt::resume_foreground(dispatcher);
                    // Both helper action owners exist before Submit. The UI only
                    // enters its synchronous source/EndBorrow gate; it never joins.
                    auxiliary = winrt::Windows::System::Threading::ThreadPool::RunAsync(
                        [controller, hooks, drainRace,
                         context](winrt::Windows::Foundation::IAsyncAction const &) noexcept
                        {
                            if (!RaceFence(drainRace, context,
                                           [&] { return hooks->WaitReached(CoreTestPoint::SubmitBeforeEndBorrow, 1); }))
                            {
                                drainRace->Fail(11);
                                RescueRace(drainRace, controller, hooks);
                            }
                            drainRace->drainStarted.store(true, std::memory_order_release);
                            drainRace->drain = controller->Drain();
                            drainRace->drainDone.store(true, std::memory_order_release);
                        });
                    auxiliaryOwnsDrain = true;
                    coordinator = winrt::Windows::System::Threading::ThreadPool::RunAsync(
                        [controller, hooks, drainRace,
                         context](winrt::Windows::Foundation::IAsyncAction const &) noexcept
                        { CoordinateSubmitDrain(drainRace, controller, hooks, context); });
                    auto source = controller->SubmitUtf8(Bytes("borrow wake"), Descriptor(1));
                    Accepted(source);
                    co_await winrt::resume_background();
                    bool coordinatorObserved = ObserveAuxiliary(coordinator, context);
                    bool drainObserved = ObserveAuxiliary(auxiliary, context);
                    observedRaceActions += coordinatorObserved ? 1u : 0u;
                    observedRaceActions += drainObserved ? 1u : 0u;
                    if (!coordinatorObserved || !drainObserved)
                    {
                        drainRace->Fail(12);
                        RescueRace(drainRace, controller, hooks);
                    }
                    Check(
                        coordinatorObserved && drainObserved &&
                            drainRace->coordinatorDone.load(std::memory_order_acquire) &&
                            drainRace->failedStep.load() == 0 && !drainRace->rescueUsed.load() && !hooks->AnyTimedOut(),
                        L"EndBorrow wake survived Drain's prior real wait without a rescue or subsequent source/UI signal");
                    Check(
                        drainRace->drainDone.load(std::memory_order_acquire) && drainRace->drain.coreDrained &&
                            drainRace->drain.driverExited && controller->Snapshot().lastInstalled == 0,
                        L"Stop waited for the entered Submit's actual final borrow exit and observed natural driver exit");
                    caseChecked = true;
                    break;
                }
                case Case::Edits:
                {
                    hooks->Arm(CoreTestPoint::DriverBeforeRun);
                    auto first = controller->SubmitUtf8(Bytes("A"), Descriptor(1));
                    Accepted(first);
                    co_await winrt::resume_background();
                    Check(Reached(context, hooks, CoreTestPoint::DriverBeforeRun), L"A entered the real single driver");
                    co_await winrt::resume_foreground(dispatcher);
                    auto batch = Pod<notepads_markdown_core_edit_batch>();
                    batch.before = Descriptor(1).source;
                    batch.after = Descriptor(2).source;
                    batch.count = 1;
                    batch.row_size = sizeof(notepads_markdown_core_edit_row);
                    std::array<notepads_markdown_core_edit_row, 1> rows{{{1, 1, 0, 1}}};
                    CoreCompletionTicket secondTicket, thirdTicket;
                    auto second = controller->SubmitEdits(batch, rows, Bytes("B"), &secondTicket);
                    Accepted(second);
                    Check(secondTicket && secondTicket.Serial() == second.serial && !secondTicket.Done(),
                          L"edit preparation returns its exact pending completion ticket");
                    auto b = controller->Snapshot();
                    Check(b.active == 1 && b.pending == 1 && b.sourceState.source.revision == 2,
                          L"B commits S while A is the sole active parse");
                    batch.before = Descriptor(2).source;
                    batch.after = Descriptor(3).source;
                    rows[0] = {2, 2, 0, 1};
                    auto third = controller->SubmitEdits(batch, rows, Bytes("C"), &thirdTicket);
                    Accepted(third);
                    Check(secondTicket.Done() && secondTicket.Status() == NOTEPADS_MARKDOWN_CORE_CANCELLED,
                          L"coalesced edit parse signals the old receipt without rolling back its source edit");
                    secondTicket.Cancel();
                    Check(thirdTicket && thirdTicket.Serial() == third.serial && !thirdTicket.Done(),
                          L"canceling the superseded edit receipt leaves its successor alive");
                    auto rejectedTicket = thirdTicket;
                    batch.count = 2;
                    auto rejected = controller->SubmitEdits(batch, rows, Bytes("C"), &rejectedTicket);
                    Check(rejected.status == NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT && !rejectedTicket &&
                              rejected.sourceState.state_valid == 0 && !thirdTicket.Done(),
                          L"local row-count rejection clears its output ticket without touching the latest source");
                    auto c = controller->Snapshot();
                    Check(c.active == 1 && c.pending == 1 && c.pendingSerial == third.serial && c.retired == 1 &&
                              c.lifetime.requestObjects <= 2,
                          L"C actually destroys B's old parse ticket before creating its own");
                    hooks->Release(CoreTestPoint::DriverBeforeRun);
                    co_await winrt::resume_background();
                    Check(ObserveUntil(context, [&] { return controller->Snapshot().lastInstalled == third.serial; }),
                          L"latest C model installed through the real dispatcher");
                    Check(ObserveUntil(context, [&] { return thirdTicket.Done(); }) &&
                              thirdTicket.Status() == NOTEPADS_MARKDOWN_CORE_OK,
                          L"the exact latest edit ticket observes its real successful terminal");
                    co_await winrt::resume_foreground(dispatcher);
                    retained = controller->PinInstalled();
                    MainEquals(retained, "ABC", context);
                    co_await winrt::resume_background();
                    Check(controller->Snapshot().runCount == 2, L"only A and C ran; B was synchronously retired");
                    caseChecked = true;
                    break;
                }
                case Case::CancelWins:
                case Case::ClaimWins:
                case Case::StopEntered:
                {
                    auto point = kind == Case::ClaimWins ? CoreTestPoint::UiAfterClaim : CoreTestPoint::UiBeforeClaim;
                    hooks->Arm(point);
                    auto submitted = controller->SubmitUtf8(Bytes("gate"), Descriptor(1));
                    Accepted(submitted);
                    co_await winrt::resume_background();
                    Check(Reached(context, hooks, point), L"real UI publication entered the controlled gate");
                    if (kind == Case::StopEntered)
                    {
                        controller->Stop();
                        auto held = controller->Snapshot();
                        Check(held.installEntered && !held.coreDrained,
                              L"Stop keeps Core borrowed until the already Entered UI segment acknowledges");
                    }
                    else
                    {
                        controller->RequestCancel();
                    }
                    hooks->Release(point);
                    if (kind == Case::ClaimWins)
                    {
                        Check(ObserveUntil(context,
                                           [&] { return controller->Snapshot().lastInstalled == submitted.serial; }),
                              L"Claim winner installs; late Cancel does not write its Rust flag");
                        Check(!controller->Snapshot().canceled,
                              L"Claim-losing cancellation has no accepted cancellation authority");
                    }
                    else if (kind == Case::CancelWins)
                    {
                        Check(ObserveUntil(context, [&] { return controller->Snapshot().retired == 1; }),
                              L"cancel winner is acknowledged and actually retired");
                        Check(controller->Snapshot().lastInstalled == 0, L"cancel winner cannot publish");
                    }
                    caseChecked = true;
                    break;
                }
                case Case::PostingEarly:
                case Case::AckStarted:
                {
                    // Hold the actual returned post action before registration, and
                    // hold its actual UI body after acknowledgement. No fake post.
                    hooks->Arm(CoreTestPoint::DriverAfterPost);
                    hooks->Arm(CoreTestPoint::UiAfterAck);
                    auto submitted = controller->SubmitUtf8(Bytes("post"), Descriptor(1));
                    Accepted(submitted);
                    co_await winrt::resume_background();
                    Check(Reached(context, hooks, CoreTestPoint::DriverAfterPost),
                          L"real CoreDispatcher post returned");
                    Check(Reached(context, hooks, CoreTestPoint::UiAfterAck),
                          L"real reusable handler ran before post registration");
                    auto early = controller->Snapshot();
                    Check(early.wakePhase == CoreWakePhase::Posting && early.wakeAck && early.postingEntries == 1,
                          L"Posting early handler acknowledges without recycling the unique wake");
                    hooks->Release(CoreTestPoint::DriverAfterPost);
                    Check(ObserveUntil(context, [&] { return controller->Snapshot().ackWhileStarted != 0; }),
                          L"driver observes actual Started after body acknowledgement");
                    auto held = controller->Snapshot();
                    Check(held.wakePhase == CoreWakePhase::InFlight && held.wakeAck,
                          L"acknowledgement does not clear an actual Started action");
                    hooks->Release(CoreTestPoint::UiAfterAck);
                    Check(
                        ObserveUntil(context, [&] { return controller->Snapshot().wakePhase == CoreWakePhase::Idle; }),
                        L"permanent timed observation detects terminal without a new controller event");
                    Check(controller->Snapshot().lastInstalled == submitted.serial,
                          L"actual model publication survived early entry");
                    caseChecked = true;
                    break;
                }
                case Case::StopBeforeEntry:
                case Case::CancelBeforeEntry:
                {
                    hooks->Arm(CoreTestPoint::UiBeforeEntry);
                    Accepted(controller->SubmitUtf8(Bytes("late"), Descriptor(1)));
                    co_await winrt::resume_background();
                    Check(Reached(context, hooks, CoreTestPoint::UiBeforeEntry),
                          L"actual platform handler retained before Core entry");
                    if (kind == Case::StopBeforeEntry)
                    {
                        controller->Stop();
                        auto drain = controller->Drain();
                        Check(drain.coreDrained && drain.driverExited,
                              L"Stop drains Core and real driver without waiting for unentered UI tail");
                    }
                    else
                    {
                        controller->RequestCancel();
                        Check(ObserveUntil(context, [&] { return controller->Snapshot().retired == 1; }),
                              L"ordinary RequestCancel retires AwaitUI without the blocked, unentered handler");
                        Check(!controller->Snapshot().driverExited,
                              L"ordinary cancellation preserves the one resident driver");
                    }
                    auto held = controller->Snapshot();
                    Check(held.lastInstalled == 0 && held.rejectedBeforeEntry == 1 && held.lifetime.uiDelegates == 1 &&
                              !held.lifetime.fullyReleased,
                          L"retained real handler tail is reported and cannot publish after Stop");
                    // Release before final cleanup. The late closure sees closed and
                    // must never touch the already destroyed Rust session or job.
                    hooks->Release(CoreTestPoint::UiBeforeEntry);
                    caseChecked = true;
                    break;
                }
                case Case::RequestWeakTail:
                {
                    hooks->RetainRequestWeak(true);
                    auto submitted = controller->SubmitUtf8(Bytes("weak"), Descriptor(1));
                    Accepted(submitted);
                    co_await winrt::resume_background();
                    Check(ObserveUntil(context,
                                       [&]
                                       {
                                           auto state = controller->Snapshot();
                                           return state.retired == 1 && state.wakePhase == CoreWakePhase::Idle;
                                       }),
                          L"request strong owners actually retired");
                    auto held = lifetime.Snapshot();
                    Check(
                        hooks->RetainedRequestExpired() && held.requestObjects == 0 && held.requestControlBlocks == 1,
                        L"expired request weak owner keeps the charged allocate_shared block alive after job destruction");
                    hooks->ReleaseRequestWeak();
                    Check(lifetime.Snapshot().requestControlBlocks == 0,
                          L"last weak release actually HeapFrees before Rust allocation lease destruction");
                    caseChecked = true;
                    break;
                }
                case Case::Retained:
                {
                    auto first = controller->SubmitUtf8(Bytes("old"), Descriptor(1));
                    Accepted(first);
                    co_await winrt::resume_background();
                    Check(ObserveUntil(context, [&] { return controller->Snapshot().lastInstalled == first.serial; }),
                          L"first real model installed");
                    co_await winrt::resume_foreground(dispatcher);
                    retained = controller->PinInstalled();
                    auto second = controller->SubmitUtf8(Bytes("new"), Descriptor(2));
                    Accepted(second);
                    co_await winrt::resume_background();
                    Check(ObserveUntil(context, [&] { return controller->Snapshot().lastInstalled == second.serial; }),
                          L"second real model replaced current installation");
                    MainEquals(retained, "old", context);
                    caseChecked = true;
                    break;
                }
                case Case::Utf16Capture:
                {
                    std::array<uint16_t, 4> input{{0x4E2D, 0xD83D, 0xDE00, 0x0000}};
                    auto first = controller->SubmitUtf16(input, Descriptor(1));
                    Accepted(first);
                    co_await winrt::resume_background();
                    Check(ObserveUntil(context, [&] { return controller->Snapshot().lastInstalled == first.serial; }),
                          L"strict real UTF16 input installed");
                    co_await winrt::resume_foreground(dispatcher);
                    auto captured = controller->CaptureCurrent();
                    Accepted(captured);
                    Check(captured.sourceState.source_accepted == 0 && captured.sourceState.source.revision == 1,
                          L"capture_current retains S without claiming a new source commit");
                    co_await winrt::resume_background();
                    Check(
                        ObserveUntil(context, [&] { return controller->Snapshot().lastInstalled == captured.serial; }),
                        L"captured current source installs through same production path");
                    co_await winrt::resume_foreground(dispatcher);
                    std::array<uint16_t, 1> invalid{{0xD800}};
                    auto rejected = controller->SubmitUtf16(invalid, Descriptor(2));
                    Check(rejected.status == NOTEPADS_MARKDOWN_CORE_INVALID_UTF16 && rejected.error.offset == 0 &&
                              rejected.sourceState.state_valid == 1 && rejected.sourceState.source.revision == 1,
                          L"invalid UTF16 returns u16 offset and preserves the accepted source", rejected.status);
                    // This deliberate primary is the expected typed rejection, not
                    // a request to hide or reset the controller's first failure.
                    caseChecked = true;
                    break;
                }
                case Case::PreparingCancel:
                {
                    hooks->Arm(CoreTestPoint::PreparingAfterStart);
                    // This test helper and its completion owner exist before Submit.
                    // It only injects a cancel at the real bootstrap gate.
                    auxiliary = winrt::Windows::System::Threading::ThreadPool::RunAsync(
                        [hooks, controller,
                         earlyCancelObserved](winrt::Windows::Foundation::IAsyncAction const &) noexcept
                        {
                            if (hooks->WaitReached(CoreTestPoint::PreparingAfterStart, 2000))
                            {
                                controller->RequestCancel();
                                earlyCancelObserved->store(true, std::memory_order_release);
                            }
                            hooks->Release(CoreTestPoint::PreparingAfterStart);
                        });
                    auto rejected = controller->SubmitUtf8(Bytes("early"), Descriptor(1));
                    Check(earlyCancelObserved->load(std::memory_order_acquire) && rejected.status == NOTEPADS_MARKDOWN_CORE_CANCELLED,
                          L"Preparing early intent reaches the same original flag before native owner/source work",
                          rejected.status);
                    Check(controller->Snapshot().installedCount == 0,
                          L"Preparing cancellation has no publication authority");
                    caseChecked = true;
                    break;
                }
                }
            }
            catch (...)
            {
                failure = std::current_exception();
            }
            // Cleanup uses this already existing frame even after assertion/ABI/
            // WinRT failure; no post-submit observer or cleanup worker is created.
            try
            {
                co_await winrt::resume_background();
            }
            catch (...)
            {
                auto schedulingStatus = winrt::to_hresult().value;
                // A failed thread-pool submission leaves this frame on its current
                // thread. Release even infinite UI gates before unwinding; never
                // wait, Drain or allocate another cleanup worker on this path.
                hooks->ReleaseAll();
                controller->Stop();
                auto snapshot = controller->Snapshot();
                auto expectedPrimary = kind == Case::Utf16Capture ? NOTEPADS_MARKDOWN_CORE_INVALID_UTF16 : NOTEPADS_MARKDOWN_CORE_OK;
                if (snapshot.primaryStatus != NOTEPADS_MARKDOWN_CORE_OK && snapshot.primaryStatus != expectedPrimary)
                {
                    context->RecordPrimary(CoreValidation::FaultKind::UnexpectedStatus, snapshot.primaryStatus,
                                           Name(kind));
                }
                if (snapshot.primaryHresult != 0)
                {
                    context->RecordPrimary(CoreValidation::FaultKind::UnexpectedException, snapshot.primaryHresult,
                                           Name(kind));
                }
                context->RecordCleanup(snapshot.cleanupStatus, Name(kind));
                context->RecordCleanup(snapshot.cleanupHresult, Name(kind));
                if (failure)
                {
                    try
                    {
                        std::rethrow_exception(failure);
                    }
                    catch (Failure const &detail)
                    {
                        if (detail.cleanupOnly)
                        {
                            context->RecordCleanup(detail.status, detail.detail);
                        }
                        else if (!(detail.cancellationSensitive && context->CancellationRequested()))
                        {
                            context->RecordPrimary(CoreValidation::FaultKind::Assertion, detail.status, Name(kind));
                        }
                    }
                    catch (winrt::hresult_error const &detail)
                    {
                        auto status = detail.code().value;
                        bool cancellationArtifact =
                            context->CancellationRequested() &&
                            (status == E_ABORT || status == static_cast<int32_t>(HRESULT_FROM_WIN32(ERROR_CANCELLED)) ||
                             (status == E_ILLEGAL_METHOD_CALL && snapshot.closed));
                        if (!cancellationArtifact)
                        {
                            context->RecordException(failure, Name(kind));
                        }
                    }
                    catch (...)
                    {
                        context->RecordException(failure, Name(kind));
                    }
                }
                // Preserve the original body/controller fault before this later
                // cleanup scheduling failure. No terminal endpoint is claimed.
                context->RecordCleanup(schedulingStatus, L"controller cleanup resume_background");
                context->Arbitrate();
                throw;
            }
            if (drainRace && (failure || !caseChecked))
            {
                drainRace->Fail(13);
                RescueRace(drainRace, controller, hooks);
                Until([&] { return drainRace->rescueDone.load(std::memory_order_acquire); }, 2500);
            }
            else
            {
                hooks->ReleaseAll();
            }
            if (!auxiliaryOwnsDrain)
            {
                controller->Stop();
            }
            bool coordinatorClean = ObserveAuxiliary(coordinator, context);
            bool auxiliaryClean = ObserveAuxiliary(auxiliary, context);
            if (!coordinatorClean || !auxiliaryClean)
            {
                context->RecordCleanupInvariant(L"controller auxiliary action terminal fence");
                if (drainRace)
                {
                    drainRace->Fail(14);
                    RescueRace(drainRace, controller, hooks);
                }
            }
            auto beforeDrain = controller->Snapshot();
            CoreDrainResult drain;
            if (beforeDrain.driverExited)
            {
                drain = {true,
                         beforeDrain.coreDrained,
                         beforeDrain.driverExited,
                         beforeDrain.canceled,
                         beforeDrain.primaryStatus,
                         beforeDrain.cleanupStatus,
                         beforeDrain.primaryHresult,
                         beforeDrain.cleanupHresult};
            }
            else if (auxiliaryOwnsDrain)
            {
                // Never invoke a competing Drain or wait indefinitely for a failed
                // watchdog. An unobserved helper remains a reported pending tail.
                if (drainRace->drainDone.load(std::memory_order_acquire))
                {
                    drain = drainRace->drain;
                }
            }
            else
            {
                drain = controller->Drain();
            }
            auto finalSnapshot = controller->Snapshot();
            results->lastDrain = drain;
            registration.Withdraw();
            if (!failure && context->CancellationRequested())
            {
                try
                {
                    context->Arbitrate();
                }
                catch (...)
                {
                    failure = std::current_exception();
                }
            }
            if (!failure)
            {
                try
                {
                    Check(caseChecked, L"case made its required real observations");
                    Check(coordinatorClean && auxiliaryClean,
                          L"all auxiliary actions reached real terminal/GetResults/Close");
                    Check(drain.offUi && drain.coreDrained && drain.driverExited && finalSnapshot.active == 0 &&
                              finalSnapshot.pending == 0,
                          L"actual Core cleanup and natural driver action terminal observed separately");
                    Check(finalSnapshot.driverStartCount == 1 && drain.cleanupStatus == NOTEPADS_MARKDOWN_CORE_OK &&
                              drain.primaryHresult == 0 && drain.cleanupHresult == 0,
                          L"exactly one real driver and no hidden cleanup/platform fault");
                    auto expectedPrimary = kind == Case::Utf16Capture ? NOTEPADS_MARKDOWN_CORE_INVALID_UTF16 : NOTEPADS_MARKDOWN_CORE_OK;
                    Check(drain.primaryStatus == expectedPrimary, L"first primary status remains visible",
                          drain.primaryStatus);
                    if (kind == Case::Retained)
                    {
                        MainEquals(retained, "old", context);
                        Check(!lifetime.Snapshot().fullyReleased && lifetime.Snapshot().modelPins == 1,
                              L"CoreDrained and DriverExited do not claim externally retained model release");
                    }
                }
                catch (...)
                {
                    failure = std::current_exception();
                }
            }
            controller.reset();
            retained.Reset();
            hooks->ReleaseRequestWeak();
            bool released = Until([&] { return lifetime.Snapshot().fullyReleased; });
            results->lastLifetime = lifetime.Snapshot();
            bool lateCleanupFailed =
                results->lastLifetime.cleanupStatus != NOTEPADS_MARKDOWN_CORE_OK || results->lastLifetime.cleanupHresult != 0;
            CoreValidation::CheckResult check{
                Name(kind), !failure && released && !lateCleanupFailed, NOTEPADS_MARKDOWN_CORE_OK,
                released
                    ? L"Real production TU/Rust/ThreadPool/CoreDispatcher; tracked native owners released. Observation ticket/storage remain live; no Rust quota zero or physical-memory claim. Controlled gates prove races only."
                    : L"Core/driver observations retained; actual platform/native owner tail still pending. FullyReleased is not claimed."};
            if (!failure)
            {
                check.detail += L" CoreDrained=" + std::to_wstring(drain.coreDrained) + L",DriverExited=" +
                                std::to_wstring(drain.driverExited) + L",driverStarts=" +
                                std::to_wstring(finalSnapshot.driverStartCount) + L",runs=" +
                                std::to_wstring(finalSnapshot.runCount) + L",installs=" +
                                std::to_wstring(finalSnapshot.installedCount) + L",retired=" +
                                std::to_wstring(finalSnapshot.retired) + L",postingEntries=" +
                                std::to_wstring(finalSnapshot.postingEntries) + L",ackStarted=" +
                                std::to_wstring(finalSnapshot.ackWhileStarted) + L",rejectedBeforeEntry=" +
                                std::to_wstring(finalSnapshot.rejectedBeforeEntry) + L",trackedReleased=" +
                                std::to_wstring(results->lastLifetime.fullyReleased) + L",observationTickets=" +
                                std::to_wstring(results->lastLifetime.observationTickets) + L",observationBlocks=" +
                                std::to_wstring(results->lastLifetime.observationAllocationBlocks) +
                                L",finalCleanupStatus=" + std::to_wstring(results->lastLifetime.cleanupStatus) +
                                L",finalCleanupHresult=" + std::to_wstring(results->lastLifetime.cleanupHresult);
            }
            bool cancellationArtifact = false;
            bool cleanupOnly = false;
            bool localAssertion = false;
            if (!failure && lateCleanupFailed)
            {
                check.status = results->lastLifetime.cleanupStatus != NOTEPADS_MARKDOWN_CORE_OK ? results->lastLifetime.cleanupStatus
                                                                                 : results->lastLifetime.cleanupHresult;
            }
            if (failure)
            {
                try
                {
                    std::rethrow_exception(failure);
                }
                catch (Failure const &detail)
                {
                    check.status = detail.status;
                    check.detail = detail.detail;
                    cancellationArtifact = detail.cancellationSensitive && context->CancellationRequested();
                    cleanupOnly = detail.cleanupOnly;
                    localAssertion = true;
                }
                catch (winrt::hresult_error const &detail)
                {
                    check.status = detail.code().value;
                    check.detail = detail.message().c_str();
                    cancellationArtifact = context->CancellationRequested() &&
                                           (check.status == E_ABORT ||
                                            check.status == static_cast<int32_t>(HRESULT_FROM_WIN32(ERROR_CANCELLED)) ||
                                            (check.status == E_ILLEGAL_METHOD_CALL && finalSnapshot.closed));
                }
                catch (...)
                {
                    check.status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                    check.detail =
                        L"Unexpected native controller test exception; same existing frame performed Stop/Drain.";
                }
            }
            if (drainRace)
            {
                check.detail += L" raceAuxiliaryTerminals=" + std::to_wstring(observedRaceActions) +
                                L",raceFailedStep=" + std::to_wstring(drainRace->failedStep.load()) +
                                L",raceRescueUsed=" + std::to_wstring(drainRace->rescueUsed.load()) +
                                L",raceGateTimeout=" + std::to_wstring(hooks->AnyTimedOut()) +
                                L",auxiliaryCleanupTerminal=" + std::to_wstring(auxiliaryClean) +
                                L",coordinatorCleanupTerminal=" + std::to_wstring(coordinatorClean) + L",CoreDrained=" +
                                std::to_wstring(drain.coreDrained) + L",DriverExited=" +
                                std::to_wstring(drain.driverExited);
            }
            auto expectedPrimary = kind == Case::Utf16Capture ? NOTEPADS_MARKDOWN_CORE_INVALID_UTF16 : NOTEPADS_MARKDOWN_CORE_OK;
            // Controller errors were recorded when they occurred, before Stop's
            // later cancellation. Preserve those even when the external Task is
            // canceled; expected cancellation-induced assertions are not primaries.
            if (drain.primaryStatus != NOTEPADS_MARKDOWN_CORE_OK && drain.primaryStatus != expectedPrimary)
            {
                context->RecordPrimary(CoreValidation::FaultKind::UnexpectedStatus, drain.primaryStatus, Name(kind));
            }
            if (drain.primaryHresult != 0)
            {
                context->RecordPrimary(CoreValidation::FaultKind::UnexpectedException, drain.primaryHresult,
                                       Name(kind));
            }
            if (drain.cleanupStatus != NOTEPADS_MARKDOWN_CORE_OK)
            {
                context->RecordCleanup(drain.cleanupStatus, Name(kind));
            }
            if (drain.cleanupHresult != 0)
            {
                context->RecordCleanup(drain.cleanupHresult, Name(kind));
            }
            context->RecordCleanup(results->lastLifetime.cleanupStatus, Name(kind));
            context->RecordCleanup(results->lastLifetime.cleanupHresult, Name(kind));
            if (!released)
            {
                context->RecordCleanupInvariant(Name(kind));
            }
            if (failure && !cancellationArtifact && !cleanupOnly)
            {
                if (localAssertion)
                {
                    context->RecordPrimary(CoreValidation::FaultKind::Assertion, check.status, Name(kind));
                }
                else
                {
                    // Context recognizes its own ArbitrationFailure and preserves
                    // the original cleanup record instead of upgrading it to primary.
                    context->RecordException(failure, Name(kind));
                }
            }
            results->checks.push_back(std::move(check));
            context->Arbitrate();
            co_await winrt::resume_foreground(dispatcher);
        }
    }

    namespace
    {
        enum class PreviewTicketCase
        {
            CleanupFence,
            ExactCancel,
            Replacement,
            Stop,
            ParseFault,
            PrepareFault
        };
        wchar_t const *PreviewName(PreviewTicketCase kind)
        {
            switch (kind)
            {
            case PreviewTicketCase::CleanupFence:
                return L"preview_ticket_cleanup_fence";
            case PreviewTicketCase::ExactCancel:
                return L"preview_ticket_exact_cancel";
            case PreviewTicketCase::Replacement:
                return L"preview_ticket_replacement";
            case PreviewTicketCase::Stop:
                return L"preview_ticket_stop";
            case PreviewTicketCase::ParseFault:
                return L"preview_ticket_parse_first_fault";
            case PreviewTicketCase::PrepareFault:
                return L"preview_ticket_prepare_first_fault";
            }
            return L"preview_ticket_unknown";
        }

        winrt::Windows::Foundation::IAsyncAction RunPreviewTicketCase(
            winrt::Windows::UI::Core::CoreDispatcher dispatcher, std::shared_ptr<Results> results,
            std::shared_ptr<CoreValidation::Context> context, PreviewTicketCase kind)
        {
            auto hooks = std::make_shared<CoreControllerTestHooks>();
            auto gate = kind == PreviewTicketCase::CleanupFence ? CoreTestPoint::RequestBeforeCleanup
                                                                : CoreTestPoint::DriverBeforeRun;
            if (kind != PreviewTicketCase::PrepareFault)
                hooks->Arm(gate, 10000);
            auto controller = CoreController::CreatePreview(dispatcher, hooks);
            auto lifetime = controller->Lifetime();
            std::array<CoreCompletionTicket, 3> tickets;
            std::exception_ptr failure;
            bool registered{};
            auto descriptor = Descriptor(1);
            descriptor.profile = NOTEPADS_MARKDOWN_CORE_G3;
            int32_t expectedPrimary = NOTEPADS_MARKDOWN_CORE_OK;
            try
            {
                context->Arbitrate();
                context->PublishController(controller.get());
                registered = true;
                std::string input = "# native preview\n\n- [x] complete\n";
                if (kind == PreviewTicketCase::ParseFault)
                {
                    // Ordinary valid Markdown beyond the explicit 256 nesting cap.
                    input.clear();
                    for (size_t i = 0; i < 300; ++i)
                        input += "> ";
                    input += "deep";
                    expectedPrimary = NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT;
                }
                else if (kind == PreviewTicketCase::PrepareFault)
                {
                    input.assign(1, static_cast<char>(0xff));
                    expectedPrimary = NOTEPADS_MARKDOWN_CORE_INVALID_UTF8;
                }
                auto submitted = controller->SubmitUtf8(Bytes(input), descriptor, &tickets[0]);
                if (kind == PreviewTicketCase::PrepareFault)
                {
                    Require(submitted.status == NOTEPADS_MARKDOWN_CORE_INVALID_UTF8 && tickets[0] && tickets[0].Done(),
                            L"preparation failure owns and completes its exact ticket", submitted.status);
                    tickets[0].Cancel();
                    Require(tickets[0].Status() == NOTEPADS_MARKDOWN_CORE_INVALID_UTF8 &&
                                tickets[0].Error().status == NOTEPADS_MARKDOWN_CORE_INVALID_UTF8 &&
                                ::WaitForSingleObjectEx(tickets[0].EventHandle(), 0, FALSE) == WAIT_OBJECT_0,
                            L"preparation first fault survives Cancel and has a signaled terminal event");
                }
                else
                {
                    Accepted(submitted);
                    Require(tickets[0] && tickets[0].Serial() == submitted.serial,
                            L"ticket serial identifies only its originating request");
                    co_await winrt::resume_background();
                    Require(Reached(context, hooks, gate), L"actual preview worker reached the controlled fence");
                    Require(!tickets[0].Done() &&
                                ::WaitForSingleObjectEx(tickets[0].EventHandle(), 0, FALSE) == WAIT_TIMEOUT,
                            L"no ticket publishes done before request cleanup");
                    if (kind == PreviewTicketCase::CleanupFence)
                    {
                        Require(controller->Snapshot().lastInstalled == submitted.serial,
                                L"model installation alone does not complete a ticket");
                    }
                    else if (kind == PreviewTicketCase::ExactCancel)
                    {
                        tickets[0].Cancel();
                        Require(!tickets[0].Done(),
                                L"Cancel does not falsely signal before the worker releases its borrow");
                    }
                    else if (kind == PreviewTicketCase::Stop)
                    {
                        controller->Stop();
                        Require(!tickets[0].Done(), L"Stop waits for the actual held worker cleanup");
                    }
                    else if (kind == PreviewTicketCase::Replacement)
                    {
                        co_await winrt::resume_foreground(dispatcher);
                        descriptor.source.revision = 2;
                        Accepted(controller->SubmitUtf8(Bytes("second"), descriptor, &tickets[1]));
                        descriptor.source.revision = 3;
                        auto third = controller->SubmitUtf8(Bytes("third"), descriptor, &tickets[2]);
                        Accepted(third);
                        Require(tickets[1].Done() && tickets[1].Status() == NOTEPADS_MARKDOWN_CORE_CANCELLED,
                                L"withdrawn pending request signals its own canceled terminal");
                        tickets[1].Cancel();
                        Require(!tickets[2].Done(), L"an old ticket cannot cancel or complete its successor");
                        co_await winrt::resume_background();
                    }
                    hooks->Release(gate);
                    Require(ObserveUntil(context, [&] { return tickets[0].Done(); }),
                            L"the originating ticket reaches a real terminal");
                    auto expected = (kind == PreviewTicketCase::ExactCancel || kind == PreviewTicketCase::Stop ||
                                     kind == PreviewTicketCase::Replacement)
                                        ? NOTEPADS_MARKDOWN_CORE_CANCELLED
                                        : expectedPrimary;
                    Require(tickets[0].Status() == expected && tickets[0].Error().status == expected,
                            L"ticket preserves the expected original terminal status", tickets[0].Status());
                    auto firstStatus = tickets[0].Status();
                    tickets[0].Cancel();
                    Require(tickets[0].Status() == firstStatus, L"post-terminal Cancel cannot rewrite the first fault");
                    if (kind == PreviewTicketCase::Replacement)
                    {
                        Require(ObserveUntil(context, [&] { return tickets[2].Done(); }),
                                L"latest request completes while the superseded ticket stays alive");
                        Require(tickets[2].Status() == NOTEPADS_MARKDOWN_CORE_OK &&
                                    controller->Snapshot().lastInstalled == tickets[2].Serial(),
                                L"precise old cancellation preserves the latest successful model", tickets[2].Status());
                    }
                }
            }
            catch (...)
            {
                failure = std::current_exception();
            }
            hooks->ReleaseAll();
            if (registered)
                context->WithdrawController(controller.get());
            controller->Stop();
            co_await winrt::resume_background();
            auto drain = controller->Drain();
            results->lastDrain = drain;
            controller.reset();
            hooks->ReleaseRequestWeak();
            if (!failure)
            {
                try
                {
                    Require(drain.offUi && drain.coreDrained && drain.driverExited &&
                                drain.cleanupStatus == NOTEPADS_MARKDOWN_CORE_OK && drain.cleanupHresult == 0 &&
                                drain.primaryHresult == 0 && drain.primaryStatus == expectedPrimary,
                            L"preview controller naturally drains without replacing its primary fault",
                            drain.primaryStatus);
                    Require(tickets[0].Done() && lifetime.Snapshot().completionControlBlocks != 0 &&
                                !lifetime.Snapshot().fullyReleased,
                            L"a ticket across Drain retains its accounted native record until its last owner");
                }
                catch (...)
                {
                    failure = std::current_exception();
                }
            }
            for (auto &ticket : tickets)
                ticket = {};
            bool released = Until([&] { return lifetime.Snapshot().fullyReleased; });
            auto final = lifetime.Snapshot();
            results->lastLifetime = final;
            CoreValidation::CheckResult check{
                PreviewName(kind),
                !failure && released && final.completionControlBlocks == 0 && final.cleanupStatus == NOTEPADS_MARKDOWN_CORE_OK &&
                    final.cleanupHresult == 0,
                NOTEPADS_MARKDOWN_CORE_OK,
                L"Real preview/G3 ThreadPool/CoreDispatcher request; exact terminal event, same-job native record, "
                L"Stop/Drain and last-ticket release observed. Controlled fences are not natural cancellation latency or physical-memory evidence."};
            if (failure)
            {
                try
                {
                    std::rethrow_exception(failure);
                }
                catch (Failure const &error)
                {
                    check.status = error.status;
                    check.detail = error.detail;
                }
                catch (winrt::hresult_error const &error)
                {
                    check.status = error.code().value;
                    check.detail = error.message().c_str();
                }
                catch (...)
                {
                    check.status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                    check.detail =
                        L"Unexpected preview ticket test exception; existing owner still stopped and drained.";
                }
            }
            else if (!check.passed)
            {
                check.status = NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
                check.detail = L"Preview ticket/native owners or late cleanup remained after bounded observation.";
            }
            if (!check.passed)
                context->RecordPrimary(CoreValidation::FaultKind::Assertion, check.status, PreviewName(kind));
            context->RecordCleanup(drain.cleanupStatus, PreviewName(kind));
            context->RecordCleanup(drain.cleanupHresult, PreviewName(kind));
            context->RecordCleanup(final.cleanupStatus, PreviewName(kind));
            context->RecordCleanup(final.cleanupHresult, PreviewName(kind));
            if (!released)
                context->RecordCleanupInvariant(PreviewName(kind));
            results->checks.push_back(std::move(check));
            context->Arbitrate();
            co_await winrt::resume_foreground(dispatcher);
        }
    }

    namespace
    {
        winrt::Windows::Foundation::IAsyncAction RunPreviewGeometryCase(
            winrt::Windows::UI::Core::CoreDispatcher dispatcher, std::shared_ptr<Results> results,
            std::shared_ptr<CoreValidation::Context> context)
        {
            constexpr auto name = L"preview_viewport_padding_coordinates";
            auto controller = CoreController::CreatePreview(dispatcher);
            auto lifetime = controller->Lifetime();
            CoreCompletionTicket ticket;
            CoreModelPin model;
            std::shared_ptr<PreviewCancellation> cancel;
            std::shared_ptr<NativeDocumentRenderer> renderer;
            std::shared_ptr<PreviewFrame> full, visible, topPadding;
            std::exception_ptr failure;
            bool registered{};
            try
            {
                context->PublishController(controller.get());
                registered = true;
                auto descriptor = Descriptor(1);
                descriptor.profile = NOTEPADS_MARKDOWN_CORE_G3;
                Accepted(controller->SubmitUtf8(
                    Bytes("row 0\n\nrow 1\n\nrow 2\n\nrow 3\n\nrow 4\n\nrow 5\n\nrow 6\n\nrow 7\n\nrow 8\n"),
                    descriptor, &ticket));
                co_await winrt::resume_background();
                Require(ObserveUntil(context, [&] { return ticket.Done(); }) && ticket.Status() == NOTEPADS_MARKDOWN_CORE_OK,
                        L"actual G3 model installed for viewport coordinate control", ticket.Status());
                co_await winrt::resume_foreground(dispatcher);
                model = controller->PinInstalled();
                Require(static_cast<bool>(model), L"viewport control owns the real installed model");
                auto summary = Pod<notepads_markdown_core_model_summary_result>();
                auto error = Pod<notepads_markdown_core_error>();
                Require(notepads_markdown_core_model_summary(model.Get(), &summary, &error) == NOTEPADS_MARKDOWN_CORE_OK,
                        L"viewport control reads complete real model identity");
                notepads_markdown_preview_key key{};
                key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
                key.model = summary.identity;
                key.layout_generation = key.theme_generation = key.measurement_generation = 1;
                key.width = 400;
                key.indent = 28;
                key.gap = 12;
                PreviewTheme theme;
                theme.padding = 200;
                co_await winrt::resume_background();
                cancel = std::make_shared<PreviewCancellation>(model.Get());
                renderer = NativeDocumentRenderer::Create(std::move(model), cancel, key, std::move(theme));
                std::atomic<uint64_t> latest{1};
                full = renderer->CreateFrame(0, 2000, 1, latest);
                Require(full && full->totalBlocks == 9,
                        L"reference Frame contains the nine actual DWrite-measured paragraphs");
                uint32_t target = NOTEPADS_MARKDOWN_PREVIEW_NONE;
                for (auto const &block : full->Blocks())
                {
                    if (block.block.y >= 100 && block.block.y + block.block.height <= 200)
                    {
                        target = block.block.node;
                        break;
                    }
                }
                Require(target != NOTEPADS_MARKDOWN_PREVIEW_NONE,
                        L"actual paragraph lies wholly in doc [100,200), outside the old erroneous overscan [200,600)");
                latest.store(2, std::memory_order_release);
                visible = renderer->CreateFrame(300, 200, 2, latest);
                Require(visible && visible->viewportY == 300 && visible->viewportHeight == 200,
                        L"Frame preserves the original content offset for drawing");
                bool found{};
                for (auto const &block : visible->Blocks())
                    found = found || block.block.node == target;
                Require(found,
                        L"content offset 300/padding 200/height 200 includes the real visible doc [100,300) paragraph");
                auto shifted = visible->Reposition(325, 200);
                Require(shifted && shifted->viewportY == 325 && &shifted->Blocks() == &visible->Blocks(),
                        L"scroll inside the measured interval shares the same layouts at the new offset");
                auto lower = renderer->CreateFrame(450, 100, 2, latest);
                Require(lower && !lower->Reposition(350, 100),
                        L"a viewport extending beyond the measured interval requires a new frame");
                latest.store(3, std::memory_order_release);
                topPadding = renderer->CreateFrame(0, 100, 3, latest);
                Require(topPadding && topPadding->Blocks().empty(),
                        L"a viewport wholly inside top padding queries an empty document interval despite overscan");
                Require(!topPadding->Reposition(125, 100),
                        L"an empty padding frame cannot stand in for newly visible content");
                auto first = renderer->CreateFrame(125, 100, 3, latest);
                Require(first && !first->Blocks().empty() && !first->Reposition(0, 100),
                        L"scrolling into top padding requires a new empty frame");
                auto bottomPadding = full->totalHeight + 201;
                auto last = renderer->CreateFrame(bottomPadding - 125, 100, 3, latest);
                Require(last && !last->Blocks().empty() && !last->Reposition(bottomPadding, 100),
                        L"scrolling into bottom padding requires a new frame");
                auto bottom = renderer->CreateFrame(bottomPadding, 100, 3, latest);
                Require(bottom && bottom->Blocks().empty(),
                        L"a viewport wholly inside bottom padding stays empty despite overscan");
            }
            catch (...)
            {
                failure = std::current_exception();
            }
            if (registered)
                context->WithdrawController(controller.get());
            controller->Stop();
            co_await winrt::resume_background();
            full.reset();
            visible.reset();
            topPadding.reset();
            renderer.reset();
            cancel.reset();
            model.Reset();
            auto drain = controller->Drain();
            results->lastDrain = drain;
            controller.reset();
            ticket = {};
            bool released = Until([&] { return lifetime.Snapshot().fullyReleased; });
            auto final = lifetime.Snapshot();
            results->lastLifetime = final;
            CoreValidation::CheckResult check{
                name,
                !failure && released && drain.coreDrained && drain.driverExited && drain.primaryStatus == NOTEPADS_MARKDOWN_CORE_OK &&
                    drain.cleanupStatus == NOTEPADS_MARKDOWN_CORE_OK && drain.primaryHresult == 0 && drain.cleanupHresult == 0 &&
                    final.cleanupStatus == NOTEPADS_MARKDOWN_CORE_OK && final.cleanupHresult == 0,
                NOTEPADS_MARKDOWN_CORE_OK,
                L"Real G3 model and NativeDocumentRenderer/DWrite; document-space viewport subtracts top padding, "
                L"clips a negative start, preserves drawing offset, and releases Frames/model/ticket after Stop/Drain."};
            if (failure)
            {
                try
                {
                    std::rethrow_exception(failure);
                }
                catch (Failure const &error)
                {
                    check.status = error.status;
                    check.detail = error.detail;
                }
                catch (PreviewFailure const &error)
                {
                    check.status = error.status;
                    check.detail = L"Actual preview geometry call failed.";
                }
                catch (winrt::hresult_error const &error)
                {
                    check.status = error.code().value;
                    check.detail = error.message().c_str();
                }
                catch (...)
                {
                    check.status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                    check.detail = L"Unexpected native viewport control failure.";
                }
            }
            else if (!check.passed)
                check.status = NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
            if (!check.passed)
                context->RecordPrimary(CoreValidation::FaultKind::Assertion, check.status, name);
            context->RecordCleanup(drain.cleanupStatus, name);
            context->RecordCleanup(drain.cleanupHresult, name);
            context->RecordCleanup(final.cleanupStatus, name);
            context->RecordCleanup(final.cleanupHresult, name);
            if (!released)
                context->RecordCleanupInvariant(name);
            results->checks.push_back(std::move(check));
            context->Arbitrate();
            co_await winrt::resume_foreground(dispatcher);
        }
    }

    winrt::Windows::Foundation::IAsyncAction RunPreviewTicketsAsync(
        winrt::Windows::UI::Core::CoreDispatcher const &dispatcher, std::shared_ptr<Results> const &results,
        std::shared_ptr<CoreValidation::Context> const &context)
    {
        if (!dispatcher || !dispatcher.HasThreadAccess() || !results || !context)
            throw winrt::hresult_invalid_argument();
        auto ui = dispatcher;
        auto output = results;
        auto owner = context;
        output->checks.reserve(output->checks.size() + 7);
        constexpr std::array cases{PreviewTicketCase::CleanupFence, PreviewTicketCase::ExactCancel,
                                   PreviewTicketCase::Replacement,  PreviewTicketCase::Stop,
                                   PreviewTicketCase::ParseFault,   PreviewTicketCase::PrepareFault};
        for (auto kind : cases)
        {
            owner->Arbitrate();
            co_await RunPreviewTicketCase(ui, output, owner, kind);
        }
        co_await RunPreviewGeometryCase(ui, output, owner);
    }

    winrt::Windows::Foundation::IAsyncAction RunAsync(winrt::Windows::UI::Core::CoreDispatcher const &dispatcher,
                                                      std::shared_ptr<Results> const &results,
                                                      std::shared_ptr<CoreValidation::Context> const &context)
    {
        if (!dispatcher || !dispatcher.HasThreadAccess() || !results || !context)
        {
            throw winrt::hresult_invalid_argument();
        }
        // Copy borrowed arguments into this pre-submit frame before the first await.
        auto ui = dispatcher;
        auto output = results;
        auto owner = context;
        output->checks.reserve(output->checks.size() + 14);
        output->untested.push_back(
            L"Actual closed CoreDispatcher / synchronous RunAsync initialization failure: no reproducible real platform fixture; not tested and not passed.");
        output->untested.push_back(
            L"Native allocation/HeapFree fault injection, serial/generation exhaustion and reentrant notification: not exercised by this real UWP suite.");
        constexpr std::array cases{Case::Edits,           Case::CancelWins,        Case::ClaimWins,
                                   Case::PostingEarly,    Case::AckStarted,        Case::StopBeforeEntry,
                                   Case::StopEntered,     Case::Retained,          Case::Utf16Capture,
                                   Case::PreparingCancel, Case::CancelBeforeEntry, Case::RequestWeakTail,
                                   Case::IdleDrainSignal, Case::SubmitDrainSignal};
        for (auto kind : cases)
        {
            owner->Arbitrate();
            co_await RunCase(ui, output, owner, kind);
        }
    }
}
