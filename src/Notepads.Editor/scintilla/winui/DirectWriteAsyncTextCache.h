// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once

#include <d2d1_3.h>
#include <d3d11.h>
#include <dxgi1_2.h>
#include <dwrite.h>
#include <wrl/client.h>
#include <winrt/base.h>

#include <algorithm>
#include <array>
#include <atomic>
#include <cmath>
#include <cstdint>
#include <memory>
#include <mutex>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

namespace Scintilla::Internal {

// Optional, font-owned lookahead. Only immutable layouts and CPU pixels cross
// threads between the editor and the worker. Worker graphics are separate
// from the UI factory, which serializes DrawTextLayout across its contexts.
class DirectWriteAsyncTextCache {
    template<class T> using Ptr = Microsoft::WRL::ComPtr<T>;
    struct Key {
        IDWriteTextLayout *layout{};
        std::wstring_view text;
        float width{}, height{};
        D2D1_COLOR_F colour{};
        float phaseX{}, phaseY{};
        D2D1_TEXT_ANTIALIAS_MODE textAA{};
        D2D1_ANTIALIAS_MODE geometryAA{};
        LUID adapter{};
        D3D_FEATURE_LEVEL featureLevel{};
        bool operator==(const Key &other) const noexcept {
            // Text identity is opt-in for this immutable font's completely
            // configured drawing-layout producer. Other callers retain the
            // stronger object-identity contract, including arbitrary layouts.
            const bool sameLayout = !text.empty() && !other.text.empty() ?
                text == other.text && width == other.width && height == other.height :
                text.empty() == other.text.empty() && layout == other.layout;
            return sameLayout && colour.r == other.colour.r &&
                colour.g == other.colour.g && colour.b == other.colour.b && colour.a == other.colour.a &&
                phaseX == other.phaseX && phaseY == other.phaseY &&
                textAA == other.textAA && geometryAA == other.geometryAA &&
                adapter.LowPart == other.adapter.LowPart && adapter.HighPart == other.adapter.HighPart &&
                featureLevel == other.featureLevel;
        }
    };
    struct Payload {
        Key key;
        Ptr<IDWriteTextLayout> layout;
        std::wstring text;
        Ptr<IDXGIAdapter> adapter;
        size_t textUnits{};
        D2D1_POINT_2F origin{};
        D2D1_MATRIX_3X2_F transform{};
        std::atomic<bool> complete{};
        std::vector<BYTE> pixels;
        UINT width{}, height{};
        float left{}, top{};
    };
    struct Entry {
        std::shared_ptr<Payload> payload;
        // Pending work reserves its complete per-entry budget. A completed
        // charge is sampled once on the UI owner before budget eviction.
        size_t charge{};
        Ptr<ID2D1Bitmap1> uploaded;
        uint64_t used{};
        bool accounted{};
    };
    static constexpr size_t MaximumEntries = 64;
    static constexpr size_t MaximumBytes = 4 * 1024 * 1024;
    static constexpr size_t MaximumEntryBytes = 2 * 1024 * 1024;
    static constexpr size_t MaximumTextUnits = 64 * 1024;
    static constexpr unsigned MaximumWorkers = 2;
    static inline std::atomic<unsigned> workers{};
    struct WorkerPermit {
        bool held = true;
        void Release() noexcept {
            if (held) { held = false; --workers; }
        }
        ~WorkerPermit() noexcept { Release(); }
    };
    struct WorkerGraphics {
        Ptr<ID3D11Device> d3d;
        Ptr<ID2D1DeviceContext> target;
        LUID adapter{};
        D3D_FEATURE_LEVEL featureLevel{};

        void Reset() noexcept {
            target.Reset();
            d3d.Reset();
        }
        bool Matches(const Key &key) const noexcept {
            return target && adapter.LowPart == key.adapter.LowPart &&
                adapter.HighPart == key.adapter.HighPart && featureLevel == key.featureLevel &&
                SUCCEEDED(d3d->GetDeviceRemovedReason());
        }
    };
    struct WorkerGraphicsPool {
        struct Slot {
            std::atomic<bool> inUse{};
            WorkerGraphics graphics;
        };
        struct SlotUse {
            Slot &slot;
            ~SlotUse() noexcept { slot.inUse.store(false, std::memory_order_release); }
        };

        std::array<Slot, MaximumWorkers> slots;
        std::atomic<size_t> creations{};

        static std::shared_ptr<WorkerGraphicsPool> Share() {
            // Keep up to two contexts across font owners and queued callbacks.
            // A static weak reference never retains idle graphics at unload.
            static std::mutex mutex;
            static std::weak_ptr<WorkerGraphicsPool> shared;
            std::lock_guard lock(mutex);
            auto pool = shared.lock();
            if (!pool) shared = pool = std::make_shared<WorkerGraphicsPool>();
            return pool;
        }

        void Rasterize(Payload &entry) noexcept {
            for (auto &slot : slots) {
                if (slot.inUse.exchange(true, std::memory_order_acquire)) continue;
                const SlotUse use{slot};
                auto &graphics = slot.graphics;
                try {
                    if (!graphics.Matches(entry.key)) {
                        graphics.Reset();
                        graphics = CreateWorkerGraphics(entry);
                        if (!graphics.target) return;
                        creations.fetch_add(1, std::memory_order_relaxed);
                    }
                    Render(entry, graphics.target.Get());
                    winrt::check_hresult(graphics.d3d->GetDeviceRemovedReason());
                } catch (...) {
                    // Discard a removed device or a context left in an unknown
                    // state by a failed recording, draw or readback.
                    graphics.Reset();
                    std::vector<BYTE>{}.swap(entry.pixels);
                }
                return;
            }
        }
    };
    struct WorkerContext {
        std::shared_ptr<Payload> entry;
        std::shared_ptr<WorkerGraphicsPool> graphics;
        HMODULE library{};
        ~WorkerContext() noexcept {
            if (library) FreeLibrary(library);
        }
    };
    std::vector<Entry> entries;
    std::shared_ptr<WorkerGraphicsPool> workerGraphics;
    Ptr<ID2D1Device> uploadDevice;
    uint64_t clock{};

    void ObserveDevice(ID2D1DeviceContext *context) noexcept {
        if (!context) return;
        Ptr<ID2D1Device> device;
        context->GetDevice(&device);
        if (uploadDevice.Get() == device.Get()) return;
        for (auto &entry : entries) entry.uploaded.Reset();
        uploadDevice = std::move(device);
    }

    void Reconcile() noexcept {
        size_t bytes = 0;
        for (auto &entry : entries) {
            if (!entry.payload->complete.load(std::memory_order_acquire)) continue;
            entry.accounted = true;
            entry.charge = entry.payload->pixels.size();
            bytes += entry.charge;
        }
        while (bytes > MaximumBytes) {
            auto oldest = entries.end();
            for (auto it = entries.begin(); it != entries.end(); ++it)
                if (it->accounted &&
                    (oldest == entries.end() || it->used < oldest->used)) oldest = it;
            if (oldest == entries.end()) break;
            bytes -= oldest->charge;
            entries.erase(oldest);
        }
    }

    static bool CaptureTarget(ID2D1RenderTarget *target, Key &key, D2D1_MATRIX_3X2_F &transform,
        Ptr<ID2D1DeviceContext> &context, Ptr<IDXGIAdapter> &adapter) noexcept {
        if (!target || FAILED(target->QueryInterface(IID_PPV_ARGS(&context)))) return false;
        float dpiX{}, dpiY{};
        context->GetDpi(&dpiX, &dpiY);
        if (dpiX != 96 || dpiY != 96 || context->GetUnitMode() != D2D1_UNIT_MODE_DIPS ||
            context->GetPrimitiveBlend() != D2D1_PRIMITIVE_BLEND_SOURCE_OVER) return false;
        const auto format = context->GetPixelFormat();
        if (format.format != DXGI_FORMAT_B8G8R8A8_UNORM || format.alphaMode != D2D1_ALPHA_MODE_PREMULTIPLIED) return false;
        Ptr<IDWriteRenderingParams> parameters;
        context->GetTextRenderingParams(&parameters);
        if (parameters) return false;
        Ptr<ID2D1Image> image;
        context->GetTarget(&image);
        Ptr<ID2D1Bitmap1> bitmap;
        if (!image || FAILED(image.As(&bitmap))) return false;
        Ptr<ID2D1ColorContext> colourContext;
        bitmap->GetColorContext(&colourContext);
        if (colourContext) return false;
        context->GetTransform(&transform);
        if (transform._11 != 1 || transform._12 != 0 || transform._21 != 0 || transform._22 != 1 ||
            !std::isfinite(transform._31) || !std::isfinite(transform._32) ||
            std::floor(transform._31) != transform._31 || std::floor(transform._32) != transform._32 ||
            std::abs(transform._31) > 1000000 || std::abs(transform._32) > 1000000) return false;
        auto aa = context->GetTextAntialiasMode();
        if (aa == D2D1_TEXT_ANTIALIAS_MODE_DEFAULT) aa = D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE;
        if (aa != D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE && aa != D2D1_TEXT_ANTIALIAS_MODE_ALIASED) return false;
        Ptr<ID2D1Device> device;
        context->GetDevice(&device);
        Ptr<ID2D1Device2> modern;
        Ptr<IDXGIDevice> dxgi;
        Ptr<IDXGIAdapter1> adapter1;
        Ptr<ID3D11Device> d3d;
        DXGI_ADAPTER_DESC1 description{};
        if (FAILED(device.As(&modern)) || FAILED(modern->GetDxgiDevice(&dxgi)) ||
            FAILED(dxgi->GetAdapter(&adapter)) || FAILED(adapter.As(&adapter1)) ||
            FAILED(adapter1->GetDesc1(&description)) || (description.Flags & DXGI_ADAPTER_FLAG_SOFTWARE) ||
            FAILED(dxgi.As(&d3d)) || FAILED(d3d->GetDeviceRemovedReason())) return false;
        key.textAA = aa;
        key.geometryAA = context->GetAntialiasMode();
        key.adapter = description.AdapterLuid;
        key.featureLevel = d3d->GetFeatureLevel();
        return true;
    }

    static bool Capture(ID2D1RenderTarget *target, IDWriteTextLayout *layout,
        ID2D1SolidColorBrush *brush, D2D1_POINT_2F origin, Key &key,
        D2D1_MATRIX_3X2_F &transform, Ptr<ID2D1DeviceContext> &context, Ptr<IDXGIAdapter> &adapter,
        std::wstring_view text) noexcept {
        if (!target || !layout || !brush || !std::isfinite(origin.x) || !std::isfinite(origin.y) ||
            std::abs(origin.x) > 1000000 || std::abs(origin.y) > 1000000 ||
            !CaptureTarget(target, key, transform, context, adapter)) return false;
        D2D1_MATRIX_3X2_F brushTransform{};
        brush->GetTransform(&brushTransform);
        if (brush->GetOpacity() != 1 || brushTransform._11 != 1 || brushTransform._12 != 0 ||
            brushTransform._21 != 0 || brushTransform._22 != 1 || brushTransform._31 != 0 || brushTransform._32 != 0) return false;
        key.layout = layout;
        key.text = text;
        key.width = layout->GetMaxWidth();
        key.height = layout->GetMaxHeight();
        key.colour = brush->GetColor();
        key.phaseX = origin.x - std::floor(origin.x);
        key.phaseY = origin.y - std::floor(origin.y);
        return true;
    }

    static WorkerGraphics CreateWorkerGraphics(const Payload &entry) {
        auto check = [](HRESULT result) { winrt::check_hresult(result); };
        Ptr<ID3D11Device> d3d;
        check(D3D11CreateDevice(entry.adapter.Get(), D3D_DRIVER_TYPE_UNKNOWN, nullptr, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            &entry.key.featureLevel, 1, D3D11_SDK_VERSION, &d3d, nullptr, nullptr));
        Ptr<IDXGIDevice> dxgi;
        check(d3d.As(&dxgi));
        Ptr<IDXGIAdapter> actualAdapter;
        DXGI_ADAPTER_DESC description{};
        check(dxgi->GetAdapter(&actualAdapter));
        check(actualAdapter->GetDesc(&description));
        if (description.AdapterLuid.LowPart != entry.key.adapter.LowPart ||
            description.AdapterLuid.HighPart != entry.key.adapter.HighPart ||
            d3d->GetFeatureLevel() != entry.key.featureLevel) return {};
        Ptr<ID2D1Factory1> factory;
        const D2D1_FACTORY_OPTIONS options{};
        // The exclusive slot can move between threadpool MTA threads.
        check(D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, __uuidof(ID2D1Factory1), &options,
            reinterpret_cast<void **>(factory.GetAddressOf())));
        Ptr<ID2D1Device> device;
        check(factory->CreateDevice(dxgi.Get(), &device));
        device->SetMaximumTextureMemory(MaximumBytes);
        Ptr<ID2D1DeviceContext> target;
        check(device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, &target));
        return {std::move(d3d), std::move(target), description.AdapterLuid, entry.key.featureLevel};
    }

    static void Render(Payload &entry, ID2D1DeviceContext *target) {
        auto check = [](HRESULT result) { winrt::check_hresult(result); };
        target->SetDpi(96, 96);
        target->SetTransform(entry.transform);
        target->SetTextAntialiasMode(entry.key.textAA);
        target->SetAntialiasMode(entry.key.geometryAA);
        Ptr<ID2D1SolidColorBrush> brush;
        check(target->CreateSolidColorBrush(entry.key.colour, &brush));
        Ptr<ID2D1CommandList> commands;
        check(target->CreateCommandList(&commands));
        target->SetTarget(commands.Get());
        target->BeginDraw();
        target->DrawTextLayout(entry.origin, entry.layout.Get(), brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
        const auto recorded = target->EndDraw();
        target->SetTarget(nullptr);
        check(recorded);
        check(commands->Close());
        target->SetTransform(D2D1::Matrix3x2F::Identity());
        D2D1_RECT_F bounds{};
        check(target->GetImageLocalBounds(commands.Get(), &bounds));
        if (!std::isfinite(bounds.left) || !std::isfinite(bounds.top) || !std::isfinite(bounds.right) ||
            !std::isfinite(bounds.bottom) || bounds.left >= bounds.right || bounds.top >= bounds.bottom) return;
        const float left = std::floor(bounds.left) - 2;
        const float top = std::floor(bounds.top) - 2;
        const double width = std::ceil(bounds.right) + 2 - left;
        const double height = std::ceil(bounds.bottom) + 2 - top;
        if (!(width > 0 && height > 0) || width > 4096 || height > 1024 || width * height > MaximumEntryBytes / 4) return;
        const auto properties = D2D1::BitmapProperties1(D2D1_BITMAP_OPTIONS_TARGET,
            D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), 96, 96);
        Ptr<ID2D1Bitmap1> bitmap;
        check(target->CreateBitmap(D2D1::SizeU(static_cast<UINT>(width), static_cast<UINT>(height)), nullptr, 0, properties, &bitmap));
        target->SetTarget(bitmap.Get());
        target->SetTransform(D2D1::Matrix3x2F::Translation(-left, -top));
        target->BeginDraw();
        target->Clear(D2D1::ColorF(0, 0));
        target->DrawImage(commands.Get(), nullptr, nullptr, D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR);
        const auto drawn = target->EndDraw();
        target->SetTarget(nullptr);
        check(drawn);
        auto cpuProperties = properties;
        cpuProperties.bitmapOptions = D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW;
        Ptr<ID2D1Bitmap1> cpu;
        check(target->CreateBitmap(bitmap->GetPixelSize(), nullptr, 0, cpuProperties, &cpu));
        check(cpu->CopyFromBitmap(nullptr, bitmap.Get(), nullptr));
        entry.width = static_cast<UINT>(width);
        entry.height = static_cast<UINT>(height);
        std::vector<BYTE> pixels(static_cast<size_t>(entry.width) * entry.height * 4);
        D2D1_MAPPED_RECT mapped{};
        check(cpu->Map(D2D1_MAP_OPTIONS_READ, &mapped));
        for (UINT row = 0; row < entry.height; ++row)
            std::copy_n(mapped.bits + static_cast<size_t>(row) * mapped.pitch, entry.width * 4,
                pixels.data() + static_cast<size_t>(row) * entry.width * 4);
        check(cpu->Unmap());
        entry.pixels = std::move(pixels);
        entry.left = left - entry.transform._31;
        entry.top = top - entry.transform._32;
    }

    static void CALLBACK RunWorker(PTP_CALLBACK_INSTANCE instance, void *context) noexcept {
        std::unique_ptr<WorkerContext> work{static_cast<WorkerContext *>(context)};
        // The real loader reference was acquired before submission. Transfer it
        // to the OS epilogue: our module counter can reach zero below while the
        // callback still has local destruction and return instructions to run.
        FreeLibraryWhenCallbackReturns(instance, std::exchange(work->library, nullptr));
        (void)CallbackMayRunLong(instance);
        auto entry = std::move(work->entry);
        auto graphics = std::move(work->graphics);
        work.reset();
        const auto initialized = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
        if (SUCCEEDED(initialized)) graphics->Rasterize(*entry);
        entry->complete.store(true, std::memory_order_release);
        // The callback may be the last cache and payload owner. Release both
        // before retiring this apartment and counter.
        graphics.reset();
        entry.reset();
        if (SUCCEEDED(initialized)) CoUninitialize();
        --workers;
        --winrt::get_module_lock();
    }

public:
    static bool CanRequest() noexcept { return workers.load(std::memory_order_relaxed) < MaximumWorkers; }
    static bool SupportsTarget(ID2D1RenderTarget *target) noexcept {
        Key key;
        D2D1_MATRIX_3X2_F transform;
        Ptr<ID2D1DeviceContext> context;
        Ptr<IDXGIAdapter> adapter;
        return CaptureTarget(target, key, transform, context, adapter);
    }

    struct Statistics {
        size_t entries{}, pending{}, reservedBytes{}, pendingReservedBytes{}, cpuBytes{}, uploadedBytes{}, textUnits{};
        size_t workerDeviceCreations{};
    };
    Statistics Snapshot() const noexcept {
        Statistics result;
        result.entries = entries.size();
        if (workerGraphics) result.workerDeviceCreations = workerGraphics->creations.load(std::memory_order_relaxed);
        for (const auto &entry : entries) {
            result.textUnits += entry.payload->textUnits;
            if (entry.payload->complete.load(std::memory_order_acquire)) {
                result.cpuBytes += entry.payload->pixels.size();
                if (entry.uploaded) result.uploadedBytes += entry.payload->pixels.size();
            } else {
                ++result.pending;
                result.pendingReservedBytes += MaximumEntryBytes;
            }
        }
        result.reservedBytes = result.cpuBytes + result.pendingReservedBytes;
        return result;
    }

    void Request(ID2D1RenderTarget *target, IDWriteTextLayout *layout,
        ID2D1SolidColorBrush *brush, D2D1_POINT_2F origin, size_t textUnits = 4096,
        std::wstring_view text = {}) noexcept {
        try {
            if (!text.empty()) textUnits = text.size();
            if (!textUnits || textUnits > 4096) return;
            Key key;
            D2D1_MATRIX_3X2_F transform;
            Ptr<ID2D1DeviceContext> context;
            Ptr<IDXGIAdapter> adapter;
            if (!Capture(target, layout, brush, origin, key, transform, context, adapter, text)) return;
            for (auto &entry : entries) {
                if (entry.payload->key == key) {
                    entry.used = ++clock;
                    return;
                }
            }
            unsigned active = workers.load();
            do {
                if (active >= MaximumWorkers) return;
            } while (!workers.compare_exchange_weak(active, active + 1));
            WorkerPermit permit;
            Reconcile();
            size_t retainedUnits = 0;
            for (const auto &entry : entries) retainedUnits += entry.payload->textUnits;
            while (entries.size() >= MaximumEntries || retainedUnits > MaximumTextUnits - textUnits) {
                auto oldest = entries.end();
                for (auto it = entries.begin(); it != entries.end(); ++it)
                    if (it->accounted && (oldest == entries.end() || it->used < oldest->used)) oldest = it;
                if (oldest == entries.end()) return;
                retainedUnits -= oldest->payload->textUnits;
                entries.erase(oldest);
            }
            auto entry = std::make_shared<Payload>();
            entry->key = key;
            entry->text = text;
            entry->key.text = entry->text;
            entry->layout = layout;
            entry->adapter = std::move(adapter);
            entry->textUnits = textUnits;
            entry->origin = origin;
            entry->transform = transform;
            auto work = std::make_unique<WorkerContext>();
            work->entry = entry;
            if (!workerGraphics) workerGraphics = WorkerGraphicsPool::Share();
            work->graphics = workerGraphics;
            // By-address acquisition pins whichever module compiled this
            // callback, including the standalone native diagnostic executable.
            if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                reinterpret_cast<LPCWSTR>(&RunWorker), &work->library)) return;
            entries.reserve(MaximumEntries);
            entries.push_back({entry, MaximumEntryBytes, {}, ++clock, false});
            ++winrt::get_module_lock();
            auto submitted = work.release();
            if (TrySubmitThreadpoolCallback(RunWorker, submitted, nullptr)) {
                permit.held = false;
            } else {
                entries.pop_back();
                delete submitted;
                entry.reset();
                permit.Release();
                --winrt::get_module_lock();
            }
        } catch (...) {}
    }

    bool Draw(ID2D1RenderTarget *target, IDWriteTextLayout *layout,
        ID2D1SolidColorBrush *brush, D2D1_POINT_2F origin, std::wstring_view text = {}) noexcept {
        try {
            if (text.size() > 4096) return false;
            Key key;
            D2D1_MATRIX_3X2_F transform;
            Ptr<ID2D1DeviceContext> context;
            Ptr<IDXGIAdapter> adapter;
            const bool captured = Capture(target, layout, brush, origin, key, transform, context, adapter, text);
            ObserveDevice(context.Get());
            if (!captured) return false;
            Reconcile();
            for (auto &slot : entries) {
                const auto &entry = slot.payload;
                if (!(entry->key == key) || !entry->complete.load(std::memory_order_acquire) || entry->pixels.empty()) continue;
                if (!slot.uploaded) {
                    const auto properties = D2D1::BitmapProperties1(D2D1_BITMAP_OPTIONS_NONE,
                        D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), 96, 96);
                    if (FAILED(context->CreateBitmap(D2D1::SizeU(entry->width, entry->height), entry->pixels.data(),
                        entry->width * 4, properties, &slot.uploaded))) return false;
                }
                const float left = entry->left + origin.x - entry->origin.x;
                const float top = entry->top + origin.y - entry->origin.y;
                context->DrawBitmap(slot.uploaded.Get(), D2D1::RectF(left, top, left + entry->width, top + entry->height),
                    1, D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR);
                slot.used = ++clock;
                return true;
            }
        } catch (...) {}
        return false;
    }
};

}
