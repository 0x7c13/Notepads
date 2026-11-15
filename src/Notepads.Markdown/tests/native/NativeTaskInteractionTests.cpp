// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeTaskInteractionTests.h"
#include "Rendering/NativeDocumentRenderer.h"
#include <array>

namespace Notepads::Markdown::Native::TaskInteractionTests
{
    namespace
    {
        void Require(bool valid, wchar_t const *detail)
        {
            if (!valid)
                throw winrt::hresult_error(E_FAIL, detail);
        }

        uint64_t DrawFocus(PreviewFrame const &frame, uint32_t index, bool focused, uint64_t *disabledGray = nullptr)
        {
            winrt::com_ptr<ID2D1Factory1> factory;
            winrt::check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, factory.put()));
            winrt::com_ptr<ID3D11Device> d3d;
            winrt::com_ptr<ID3D11DeviceContext> d3dContext;
            D3D_FEATURE_LEVEL level{};
            winrt::check_hresult(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_WARP, nullptr,
                                                   D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION,
                                                   d3d.put(), &level, d3dContext.put()));
            auto dxgi = d3d.as<IDXGIDevice>();
            winrt::com_ptr<ID2D1Device> device;
            winrt::check_hresult(factory->CreateDevice(dxgi.get(), device.put()));
            winrt::com_ptr<ID2D1DeviceContext> context;
            winrt::check_hresult(device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, context.put()));
            constexpr UINT32 width = 1024, height = 512;
            auto properties =
                D2D1::BitmapProperties1(D2D1_BITMAP_OPTIONS_TARGET,
                                        D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED));
            winrt::com_ptr<ID2D1Bitmap1> target;
            winrt::check_hresult(
                context->CreateBitmap(D2D1::SizeU(width, height), nullptr, 0, properties, target.put()));
            context->SetTarget(target.get());
            context->BeginDraw();
            context->Clear(D2D1::ColorF(D2D1::ColorF::White));
            try
            {
                // A retained nonempty reading selection must not hide task focus.
                NativeDocumentRenderer::Draw(frame, context.get(), 32, 32, 0, 3, {ReadingActionKind::Task, index},
                                             focused);
            }
            catch (...)
            {
                (void)context->EndDraw();
                throw;
            }
            winrt::check_hresult(context->EndDraw());
            context->SetTarget(nullptr);
            properties.bitmapOptions = D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW;
            winrt::com_ptr<ID2D1Bitmap1> readback;
            winrt::check_hresult(
                context->CreateBitmap(D2D1::SizeU(width, height), nullptr, 0, properties, readback.put()));
            winrt::check_hresult(readback->CopyFromBitmap(nullptr, target.get(), nullptr));
            D2D1_MAPPED_RECT mapped{};
            winrt::check_hresult(readback->Map(D2D1_MAP_OPTIONS_READ, &mapped));
            uint64_t pixels{};
            ReadingRect checkbox{};
            if (disabledGray)
            {
                auto const &object = frame.descriptors->objects[index];
                auto rectangles = NativeDocumentRenderer::Rectangles(frame, object.text.start, object.text.end);
                Require(rectangles.size() == 1, L"disabled pixel probe has the actual checkbox rectangle");
                checkbox = rectangles.front();
                *disabledGray = 0;
            }
            for (UINT32 y = 0; y < height; ++y)
            {
                auto row = mapped.bits + static_cast<size_t>(y) * mapped.pitch;
                for (UINT32 x = 0; x < width; ++x)
                {
                    auto pixel = row + static_cast<size_t>(x) * 4;
                    // This fixture's focus color is the only green paint.
                    if (pixel[1] > 170 && pixel[2] < 90 && pixel[0] < 140 && pixel[3] > 200)
                        ++pixels;
                    if (disabledGray && x >= 32 + checkbox.x && x < 32 + checkbox.x + checkbox.width &&
                        y >= 32 + checkbox.y && y < 32 + checkbox.y + checkbox.height &&
                        pixel[0] >= 80 && pixel[0] <= 120 && pixel[1] == pixel[0] && pixel[2] == pixel[0] &&
                        pixel[3] > 200)
                        ++*disabledGray;
                }
            }
            winrt::check_hresult(readback->Unmap());
            return pixels;
        }

        void CheckTransport(PreviewFrame const &frame, Results &results)
        {
            struct Expected
            {
                uint64_t source;
                uint32_t main;
                uint8_t marker;
                bool checked;
            };
            constexpr std::array expected{Expected{13, 4, ' ', false}, Expected{25, 12, 'x', true},
                                          Expected{37, 19, 'X', true}};
            Require(frame.descriptors && frame.descriptors->objects.size() == expected.size(),
                    L"actual parsed task count matches the authored fixture");
            ++results.transportChecks;
            for (size_t index = 0; index < expected.size(); ++index)
            {
                auto const &object = frame.descriptors->objects[index];
                auto const &oracle = expected[index];
                Require(object.kind == ReadingObjectKind::Task && object.hasTaskAction &&
                            object.checked == oracle.checked && object.taskActionStart == oracle.source &&
                            object.taskActionEnd == oracle.source + 3 &&
                            object.taskExpected == std::array<uint8_t, 3>{'[', oracle.marker, ']'} &&
                            object.text.start == oracle.main && object.text.end == oracle.main + 1,
                        L"real Core/FFI task action preserves exact UTF8 expected bytes and separate Main UTF16 range");
                ++results.transportChecks;
                auto rectangles = NativeDocumentRenderer::Rectangles(frame, object.text.start, object.text.end);
                Require(rectangles.size() == 1 && rectangles[0].width > 0 && rectangles[0].height > 0,
                        L"real checkbox has one current painted rectangle");
                auto rect = rectangles[0];
                for (bool right : {false, true})
                {
                    auto hit = NativeDocumentRenderer::HitTest(frame, rect.x + rect.width * (right ? 0.75f : 0.25f),
                                                               rect.y + rect.height * 0.5f);
                    Require(
                        hit && hit->inside && hit->cluster.start == oracle.main &&
                            hit->cluster.end == oracle.main + 1 &&
                            hit->position == oracle.main + static_cast<uint32_t>(right),
                        L"left and right checkbox halves retain the same task cluster although right position is end");
                    ++results.clusterChecks;
                }
            }
        }

        template <typename Change>
        void RejectDescriptor(MainTextSnapshot const &main, ReadingObjectDescriptor const &original, Change &&change,
                              Results &results)
        {
            SemanticDescriptors descriptor{main.Memory(), main.TextGeneration()};
            descriptor.objects.push_back(original);
            change(descriptor.objects.front());
            try
            {
                descriptor.Finish(main);
            }
            catch (ReadingFailure const &failure)
            {
                Require(failure.status == NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE,
                        L"synthetic malformed descriptor is rejected as invalid provenance");
                ++results.descriptorNegativeChecks;
                return;
            }
            throw winrt::hresult_error(E_FAIL, L"malformed task descriptor unexpectedly accepted");
        }
    }

    Results Run(std::shared_ptr<MainTextSnapshot const> const &main, notepads_markdown_core_cancel *sourceFlag)
    {
        Require(main && main->Text() == FixtureMain && sourceFlag,
                L"task interaction tests require this actual fixed source's immutable Main and live flag");
        Results results;
        auto const &memory = main->Memory();
        auto references = memory.use_count();
        {
            auto cancel = std::make_shared<PreviewCancellation>(main->ModelOwner().Get());
            auto reading =
                std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory}, memory, cancel,
                                                         main->SourceGeneration(), main->TextGeneration());
            reading->PublishMain(main);
            notepads_markdown_preview_key key{};
            key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
            key.model = main->Identity();
            key.layout_generation = key.theme_generation = key.measurement_generation = 1;
            key.width = 800;
            key.indent = 24;
            key.gap = 8;
            PreviewTheme theme;
            theme.focusColor = {255, 13, 211, 97};
            std::atomic<uint64_t> latest{1};
            auto renderer = NativeDocumentRenderer::Create({}, cancel, key, theme, reading);
            auto first = renderer->CreateFrame(0, 1000, 1, latest);
            Require(first && first->main == main && first->descriptors &&
                        first->descriptors->textGeneration == main->TextGeneration(),
                    L"task descriptors and real Frame retain the exact immutable text identity");
            CheckTransport(*first, results);
            auto const &task = first->descriptors->objects[0];
            RejectDescriptor(*main, task, [](auto &value) { value.taskActionStart = UINT64_MAX - 1; }, results);
            RejectDescriptor(*main, task, [](auto &value) { ++value.taskActionEnd; }, results);
            RejectDescriptor(*main, task, [](auto &value) { value.taskExpected[0] = '('; }, results);
            RejectDescriptor(*main, task, [](auto &value) { value.taskExpected[1] = 'y'; }, results);
            RejectDescriptor(*main, task, [](auto &value) { value.checked = true; }, results);
            RejectDescriptor(*main, task, [](auto &value) { value.kind = ReadingObjectKind::Image; }, results);
            {
                // An artificial no-action descriptor is a distinct negative
                // contract, never reported as a naturally emitted parser case.
                SemanticDescriptors noAction{memory, main->TextGeneration()};
                noAction.objects.push_back(task);
                noAction.objects.front().hasTaskAction = false;
                noAction.Finish(*main);
                Require(!noAction.objects[0].hasTaskAction && !noAction.objects[0].checked &&
                            std::wstring_view{noAction.Copy(*main, task.text.start, task.text.end)} == L"[ ]",
                        L"synthetic missing action remains a readable unchecked task with no edit authority");
                ++results.descriptorNegativeChecks;
            }
            auto visiblePixels = DrawFocus(*first, 0, true);
            auto hiddenPixels = DrawFocus(*first, 0, false);
            Require(visiblePixels > 0 && hiddenPixels == 0,
                    L"actual WARP task focus uses the theme's focus color only while the control has focus");
            results.themedFocusPixels = results.hiddenFocusHasNoPixels = true;
            auto narrowKey = key;
            narrowKey.width = 275;
            narrowKey.layout_generation = narrowKey.theme_generation = narrowKey.measurement_generation = 2;
            auto largeTheme = theme;
            largeTheme.fontSize = 30;
            largeTheme.lineHeight = 45;
            auto narrowRenderer = NativeDocumentRenderer::Create({}, cancel, narrowKey, largeTheme, reading);
            auto second = narrowRenderer->CreateFrame(0, 1000, 1, latest);
            Require(second && second->descriptors == first->descriptors && second->main == first->main &&
                        second->navigation != first->navigation,
                    L"theme/width remeasurement reuses the same immutable task action authority");
            CheckTransport(*second, results);
            CheckTransport(*first, results);
            results.sameDescriptorAcrossLayouts = true;
        }
        Require(memory.use_count() == references, L"task test renderer/Frame/descriptor owners retire");
        results.ownersReleased = true;
        results.passed = results.transportChecks == 12 && results.clusterChecks == 18 &&
                         results.descriptorNegativeChecks == 7 && results.sameDescriptorAcrossLayouts &&
                         results.themedFocusPixels && results.hiddenFocusHasNoPixels && results.ownersReleased;
        results.detail = L"Real Core/FFI/DWrite UTF8 action checks=" + std::to_wstring(results.transportChecks) +
                         L"; actual left/right task clusters=" + std::to_wstring(results.clusterChecks) +
                         L"; explicit synthetic descriptor negatives=" +
                         std::to_wstring(results.descriptorNegativeChecks) +
                         L"; WARP themed focus/hidden focus and immutable same-source remeasurement checked. "
                         L"No mouse/key/UIA/host Undo runtime qualification is implied.";
        return results;
    }

    std::wstring RunStaticHtmlCheckbox(std::shared_ptr<MainTextSnapshot const> const &main,
                                       notepads_markdown_core_cancel *sourceFlag)
    {
        Require(main && main->Text() == StaticFixtureMain && sourceFlag,
                L"static HTML diagnostics use the actual independently authored Core fixture");
        auto const &memory = main->Memory();
        auto references = memory.use_count();
        uint32_t wireNegatives{}, clusters{}, descriptors{}, copyBoundaryChecks{};
        uint64_t disabledGray{};
        {
            auto cancel = std::make_shared<PreviewCancellation>(main->ModelOwner().Get());
            auto reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                memory, cancel, main->SourceGeneration(), main->TextGeneration());
            reading->PublishMain(main);
            notepads_markdown_preview_key key{};
            key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
            key.model = main->Identity();
            key.layout_generation = key.theme_generation = key.measurement_generation = 1;
            key.width = 800;
            key.indent = 28;
            key.gap = 12;
            PreviewTheme theme;
            theme.foreground = {255, 211, 13, 13};
            theme.borderColor = {255, 97, 97, 97};
            theme.focusColor = {255, 13, 211, 97};
            std::atomic<uint64_t> latest{1};
            auto renderer = NativeDocumentRenderer::Create({}, cancel, key, theme, reading);
            auto frame = renderer->CreateFrame(0, 1000, 1, latest);
            Require(frame && frame->descriptors && frame->descriptors->objects.size() == 5,
                    L"three HTML checkboxes, one inline image and the original GFM task are distinct objects");
            Require(frame->descriptors->objects[1].kind == ReadingObjectKind::Image &&
                        frame->descriptors->objects[1].alt == L"图标",
                    L"safe inline image alt is retained for complete checkbox label export");
            std::array<std::wstring_view, 3> labels{L"中😀 Alpha & one 图标", L"Beta", L"Gamma"};
            constexpr std::array<size_t, 4> taskIndices{0, 2, 3, 4};
            for (size_t index = 0; index < 4; ++index)
            {
                auto const &object = frame->descriptors->objects[taskIndices[index]];
                Require(object.kind == ReadingObjectKind::Task && object.staticHtmlCheckbox == (index < 3) &&
                            object.checked == (index == 0 || index == 2) && object.hasTaskAction == (index == 3),
                        L"HTML boolean state and static edit authority survive real Core/FFI transport");
                if (index < 3)
                    Require(object.taskActionStart == 0 && object.taskActionEnd == 0 &&
                                object.taskExpected == std::array<uint8_t, 3>{} &&
                                std::wstring_view{object.label.data(), object.label.size()} == labels[index],
                            L"HTML checkbox label is complete decoded display text and has no synthetic source action");
                ++descriptors;
                auto rectangles = NativeDocumentRenderer::Rectangles(*frame, object.text.start, object.text.end);
                Require(rectangles.size() == 1 && rectangles[0].width > 0 && rectangles[0].height > 0,
                        L"static HTML checkbox has exactly one nonempty selectable rectangle");
                for (bool right : {false, true})
                {
                    auto rect = rectangles.front();
                    auto hit = NativeDocumentRenderer::HitTest(*frame,
                        rect.x + rect.width * (right ? 0.75f : 0.25f), rect.y + rect.height * 0.5f);
                    Require(hit && hit->inside && hit->cluster.start == object.text.start &&
                                hit->cluster.end == object.text.end &&
                                hit->position == object.text.start + static_cast<uint32_t>(right),
                            L"both checkbox halves preserve exact Main object selection and coverage");
                    ++clusters;
                }
            }
            auto copied = frame->descriptors->Copy(*main, 0, main->Length());
            Require(std::wstring_view{copied.data(), copied.size()} ==
                        L"[x] 中😀 Alpha & one 图标\n[ ] Beta\n[x] Gamma\n[ ] GFM",
                    L"native copy preserves decoded labels and each checked state without raw HTML");
            auto checkCopyBoundaries = [&]
            {
                struct Probe
                {
                    uint32_t start, end;
                    std::wstring_view expected;
                };
                // Fixed independent UTF16 offsets in StaticFixtureMain: task0,
                // image18, task20/task27/task35. A range's end excludes the next
                // object, and an empty/ordinary-text range expands no object.
                constexpr std::array probes{
                    Probe{0, 0, L""}, Probe{0, 1, L"[x]"}, Probe{1, 2, L" "}, Probe{3, 5, L"😀"},
                    Probe{1, 18, L" 中😀 Alpha & one "}, Probe{17, 18, L" "}, Probe{18, 18, L""},
                    Probe{18, 19, L"图标"}, Probe{19, 20, L"\n"}, Probe{18, 20, L"图标\n"},
                    Probe{20, 20, L""}, Probe{20, 21, L"[ ]"}, Probe{21, 26, L" Beta"},
                    Probe{20, 26, L"[ ] Beta"}, Probe{26, 28, L"\n[x]"}, Probe{35, 36, L"[ ]"},
                    Probe{36, 40, L" GFM"}, Probe{35, 40, L"[ ] GFM"}, Probe{40, 40, L""},
                    Probe{18, 28, L"图标\n[ ] Beta\n[x]"}};
                for (auto const &probe : probes)
                {
                    auto value = frame->descriptors->Copy(*main, probe.start, probe.end);
                    Require(std::wstring_view{value.data(), value.size()} == probe.expected,
                            L"bounded object copy retains authored empty/text/object/end-exclusive range semantics");
                    ++copyBoundaryChecks;
                }
            };
            checkCopyBoundaries();
            {
                SemanticDescriptors reversed{memory, main->TextGeneration()};
                for (auto at = frame->descriptors->objects.rbegin(); at != frame->descriptors->objects.rend(); ++at)
                    reversed.objects.push_back(*at);
                reversed.Finish(*main);
                Require(reversed.objects.front().label == L"中😀 Alpha & one 图标" &&
                            reversed.Copy(*main, 18, 28) == L"图标\n[ ] Beta\n[x]" &&
                            reversed.Copy(*main, 0, main->Length()) == copied,
                        L"Finish sorts all object spans before bounded copy and full static label binding");
            }
            Require(DrawFocus(*frame, 0, true, &disabledGray) == 0 && disabledGray > 0 &&
                        DrawFocus(*frame, 4, true) > 0,
                    L"real WARP paint has gray static checkbox ink, no static focus and an ordinary GFM focus ring");

            notepads_markdown_preview_decoration wire{};
            wire.kind = NOTEPADS_MARKDOWN_PREVIEW_DECORATION_STATIC_HTML_CHECKBOX;
            ValidatePreviewDecoration(wire);
            auto rejectWire = [&](auto change)
            {
                auto malformed = wire;
                change(malformed);
                try { ValidatePreviewDecoration(malformed); }
                catch (ReadingFailure const &failure)
                {
                    Require(failure.status == NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE,
                            L"malformed new decoration fails closed as provenance");
                    ++wireNegatives;
                    return;
                }
                throw winrt::hresult_error(E_FAIL, L"malformed static HTML wire decoration was accepted");
            };
            rejectWire([](auto &value) { value.kind = 0; });
            rejectWire([](auto &value) { value.kind = 5; });
            rejectWire([](auto &value) { value.checked = 2; });
            rejectWire([](auto &value) { value.reserved = 1; });
            rejectWire([](auto &value) { value.has_action = 1; });
            rejectWire([](auto &value) { value.action_start = 1; });
            rejectWire([](auto &value) { value.action_end = 1; });
            rejectWire([](auto &value) { value.expected = 1; });
            Results negatives;
            auto const &first = frame->descriptors->objects.front();
            RejectDescriptor(*main, first, [](auto &value) { value.hasTaskAction = true; }, negatives);
            RejectDescriptor(*main, first, [](auto &value) { value.taskActionStart = 1; }, negatives);
            RejectDescriptor(*main, first, [](auto &value) { value.taskExpected[0] = '['; }, negatives);
            RejectDescriptor(*main, first, [](auto &value) { value.kind = ReadingObjectKind::Image; }, negatives);
            Require(negatives.descriptorNegativeChecks == 4, L"static descriptor edit authority cannot be forged");

            auto narrowKey = key;
            narrowKey.width = 180;
            narrowKey.layout_generation = narrowKey.measurement_generation = 2;
            auto narrow = NativeDocumentRenderer::Create({}, cancel, narrowKey, theme, reading);
            auto second = narrow->CreateFrame(0, 1000, 1, latest);
            Require(second && second->main == frame->main && second->descriptors == frame->descriptors,
                    L"narrow wrapping preserves static checkbox snapshot identity and immutable labels");
            Require(notepads_markdown_core_cancel_request(sourceFlag) == NOTEPADS_MARKDOWN_CORE_OK,
                    L"the real original source producer is retired before retained-Frame reads");
            renderer.reset();
            narrow.reset();
            Require(frame->descriptors->objects.front().label == second->descriptors->objects.front().label &&
                        frame->descriptors->Copy(*main, 0, 1) == L"[x]",
                    L"retained Frames own funded static labels after source cancellation and both renderers retire");
            checkCopyBoundaries();
        }
        Require(memory.use_count() == references, L"all static-checkbox label/descriptor/Frame funding owners retire");
        Require(wireNegatives == 8 && descriptors == 4 && clusters == 8 && copyBoundaryChecks == 40,
                L"the exact static HTML transport and fail-closed probes all executed");
        return L" Static HTML: four task descriptors/five objects/eight actual checkbox-half hits; full labels with "
               L"decoded inline image alt instead of U+FFFC, "
               L"checked/unchecked/boolean-false semantics; ordinary GFM action retained; forty authored copy "
               L"boundaries before/after source cancellation; gray WARP "
               L"ink and static focus rejection; eight malformed wire/four descriptor negatives; narrow layout "
               L"and funded retained-Frame labels/owner retirement. UI input and host Undo remain product gates.";
    }
}
