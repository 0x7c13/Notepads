// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Text/NativeScriptTextRenderer.h"
#include "Text/NativeScriptLayout.h"
#include "Rendering/NativeTextBitmapCache.h"
#include "Rendering/NativeDocumentRenderer.h"
#include <dwrite_3.h>
#include <d2d1_3.h>
#include <cstring>
#include <array>
#include <numeric>
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
#include <limits>
#endif

namespace Notepads::Markdown::Native
{
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    class NativeTextBitmapCache::DiagnosticBranchTimer final
    {
        DiagnosticBranchScope *_scope{};
        Statistics &_statistics;
        size_t _branch{};
        uint64_t _start{}, _bitmapMidpoint{}, _logicalStart{}, _logicalEnd{};
        uint32_t _textIndex{}, _bitmapSource{};
        uint64_t _bitmapApiEnd{}, _bitmapCpuKernel{}, _bitmapCpuUser{}, _bitmapCpu{}, _bitmapPrewarmSequence{};
        DWORD _bitmapCpuThread{};
        bool _bitmapCpuBeginValid{}, _bitmapCpuValid{}, _bitmapQuietPrewarm{}, _bitmapNoCountedPrewarm{};
        static bool ReadCpu(uint64_t &kernel, uint64_t &user) noexcept
        {
            FILETIME creation{}, exit{}, kernelTime{}, userTime{};
            if (!::GetThreadTimes(::GetCurrentThread(), &creation, &exit, &kernelTime, &userTime)) return false;
            kernel = (static_cast<uint64_t>(kernelTime.dwHighDateTime) << 32) | kernelTime.dwLowDateTime;
            user = (static_cast<uint64_t>(userTime.dwHighDateTime) << 32) | userTime.dwLowDateTime;
            return true;
        }
        bool ObservingBitmap() const noexcept
        {
            return _scope && _bitmapSource && _scope->_cache && _scope->_cache->_diagnosticBranchScope == _scope;
        }
        static uint64_t Tick() noexcept
        {
            LARGE_INTEGER value{};
            return ::QueryPerformanceCounter(&value) && value.QuadPart > 0 ?
                static_cast<uint64_t>(value.QuadPart) : 0;
        }
      public:
        enum Branch : size_t { Take, Build, Upload, Replay };
        DiagnosticBranchTimer(NativeTextBitmapCache &cache, Branch branch) noexcept :
            _scope(cache._diagnosticBranchScope), _statistics(cache._statistics), _branch(branch)
        {
            if (!_scope) return;
            auto &calls = _statistics.branchCalls[_branch];
            if (calls == (std::numeric_limits<uint64_t>::max)())
            {
                _scope->_valid = false;
                _statistics.branchOverflow = true;
            }
            else ++calls;
            _start = Tick();
            if (!_start || _start < _scope->_lastBoundary || _scope->_active)
                _scope->_valid = false;
            if (_start > _scope->_lastBoundary) _scope->_lastBoundary = _start;
            ++_scope->_active;
        }
        void ObserveBitmap(uint32_t textIndex, uint64_t logicalStart, uint64_t logicalEnd, uint32_t source) noexcept
        {
            if (!_scope) return;
            _textIndex = textIndex;
            _logicalStart = logicalStart;
            _logicalEnd = logicalEnd;
            _bitmapSource = source;
        }
        void BitmapMidpoint() noexcept
        {
            if (!ObservingBitmap()) return;
            _bitmapPrewarmSequence = NativeTextBitmapCache::DiagnosticPrewarmSequence();
            const auto active = NativeTextBitmapCache::DiagnosticPrewarmWorkers();
            const auto sequence = NativeTextBitmapCache::DiagnosticPrewarmSequence();
            _bitmapQuietPrewarm = !active && sequence == _bitmapPrewarmSequence &&
                sequence != (std::numeric_limits<uint64_t>::max)();
            _bitmapCpuThread = ::GetCurrentThreadId();
            _bitmapCpuBeginValid = ReadCpu(_bitmapCpuKernel, _bitmapCpuUser);
            _bitmapMidpoint = Tick(); // API wall starts after observer setup.
        }
        void BitmapEnd() noexcept
        {
            if (!ObservingBitmap()) return;
            _bitmapApiEnd = Tick(); // First boundary after DrawBitmap returns.
            uint64_t kernel{}, user{};
            const auto cpuEndValid = ReadCpu(kernel, user);
            const auto sequence = NativeTextBitmapCache::DiagnosticPrewarmSequence();
            const auto sameThread = _bitmapCpuThread == ::GetCurrentThreadId();
            _bitmapNoCountedPrewarm = sameThread && _bitmapQuietPrewarm && sequence == _bitmapPrewarmSequence;
            if (sameThread && _bitmapCpuBeginValid && cpuEndValid &&
                kernel >= _bitmapCpuKernel && user >= _bitmapCpuUser)
            {
                const auto kernelDelta = kernel - _bitmapCpuKernel, userDelta = user - _bitmapCpuUser;
                constexpr auto maximum = DiagnosticBitmapReplay::MaximumJsonInteger;
                if (kernelDelta <= maximum && userDelta <= maximum - kernelDelta)
                {
                    _bitmapCpu = kernelDelta + userDelta;
                    _bitmapCpuValid = true;
                }
            }
        }
        void Stop() noexcept
        {
            auto scope = std::exchange(_scope, nullptr);
            if (!scope) return;
            const auto end = Tick();
            if (!scope->_active || !end || end < scope->_lastBoundary) scope->_valid = false;
            if (scope->_active) --scope->_active;
            if (end > scope->_lastBoundary) scope->_lastBoundary = end;
            if (_bitmapSource)
                scope->_bitmapReplay.Complete(_start, _bitmapMidpoint, end, _textIndex, _logicalStart, _logicalEnd,
                    _bitmapSource, scope->_valid && scope->_cache && scope->_cache->_diagnosticBranchScope == scope,
                    _bitmapApiEnd, _bitmapCpu, _bitmapCpuValid, _bitmapNoCountedPrewarm);
            if (!_start || !end || end < _start)
            {
                scope->_valid = false;
                return;
            }
            auto &ticks = _statistics.branchTicks[_branch];
            const auto elapsed = end - _start;
            if (elapsed > (std::numeric_limits<uint64_t>::max)() - ticks)
            {
                scope->_valid = false;
                _statistics.branchOverflow = true;
                return;
            }
            ticks += elapsed;
        }
        ~DiagnosticBranchTimer() noexcept { Stop(); }
        DiagnosticBranchTimer(DiagnosticBranchTimer const &) = delete;
        DiagnosticBranchTimer &operator=(DiagnosticBranchTimer const &) = delete;
    };
    NativeTextBitmapCache::DiagnosticBranchScope::DiagnosticBranchScope(NativeTextBitmapCache *cache,
        DiagnosticPrewarmTarget const *priorPrewarmTarget) noexcept : _cache(cache), _priorPrewarmTarget(priorPrewarmTarget)
    {
        if (!_cache) return;
        _previous = std::exchange(_cache->_diagnosticBranchScope, this);
        // Reentrant blocks share cumulative counters, so their outer deltas
        // overlap. Preserve the numbers but explicitly reject attribution.
        if (_previous) _previous->_valid = _valid = false;
    }
    bool NativeTextBitmapCache::DiagnosticBranchScope::RecordPrewarmTarget(DiagnosticPrewarmTarget const &target) noexcept
    {
        const auto prior = _priorPrewarmTarget && _priorPrewarmTarget->observations ? _priorPrewarmTarget : nullptr;
        if (!_prewarmTarget.observations)
        {
            _prewarmTarget = target;
            if (prior) _prewarmTarget.duplicates = prior->SameTarget(target) ? 1u : 0u;
            if (_previous) _prewarmTarget.flags |= DiagnosticPrewarmTarget::ScopeInvalid;
            return !prior && !_previous; // At most one store observation in this Paint.
        }
        DiagnosticPrewarmTarget extra;
        extra.observations = 1;
        extra.duplicates = (prior ? prior : &_prewarmTarget)->SameTarget(target) ? 1u : 0u;
        _prewarmTarget.AddCounts(extra);
        return false;
    }
    void NativeTextBitmapCache::DiagnosticBranchScope::Begin(uint64_t start) noexcept
    {
        if (!_cache) return;
        _lastBoundary = start;
        if (!start) _valid = false;
    }
    void NativeTextBitmapCache::DiagnosticBranchScope::Stop(uint64_t end) noexcept
    {
        auto cache = std::exchange(_cache, nullptr);
        if (!cache) return;
        if (_active || !end || end < _lastBoundary || cache->_diagnosticBranchScope != this) _valid = false;
        // Outcome bits are operation proof only while attribution is valid.
        if (!_valid && _prewarmTarget.observations)
        {
            _prewarmTarget.flags &= ~(DiagnosticPrewarmTarget::ReadyLookupAttempted |
                DiagnosticPrewarmTarget::ReadyPayloadAcquired | DiagnosticPrewarmTarget::BitmapUploaded |
                DiagnosticPrewarmTarget::ReadyReplayed);
            _prewarmTarget.flags |= DiagnosticPrewarmTarget::ScopeInvalid;
        }
        cache->_diagnosticBranchScope = _previous;
    }
    NativeTextBitmapCache::DiagnosticBranchScope::~DiagnosticBranchScope() noexcept { Stop(_lastBoundary); }
#endif
    namespace
    {
        constexpr uint64_t MaximumBytes = 128ull * 1024 * 1024;
        constexpr uint64_t MaximumEntryBytes = 16ull * 1024 * 1024;
        constexpr uint32_t MaximumCommandEntries = 256;
        constexpr uint64_t MaximumCommandTextUnits = 32768;
        constexpr uint32_t MaximumCommandEntryTextUnits = 512;
        constexpr uint64_t MaximumPrewarmEntryBytes = 4ull * 1024 * 1024;
        constexpr size_t DrawPrewarmSlots = 4;
        constexpr size_t LoadingPrewarmSlots = 12;
        constexpr size_t OrdinaryPrewarmSlots = 3;
        constexpr size_t CommandPrewarmSlots = 2;
        constexpr size_t TablePrewarmSlots = 6;
        std::atomic<uint32_t> prewarmWorkers{};
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        // Admission witness only; never reset by tracing, Clear or replacement.
        // Saturation and non-lock-free atomics make quiescence unknown.
        std::atomic<uint64_t> prewarmAdmissionSequence{};
        void NotePrewarmAdmission() noexcept
        {
            if (!prewarmAdmissionSequence.is_lock_free() || !prewarmWorkers.is_lock_free()) return;
            auto sequence = prewarmAdmissionSequence.load(std::memory_order_seq_cst);
            while (sequence != (std::numeric_limits<uint64_t>::max)() &&
                !prewarmAdmissionSequence.compare_exchange_weak(sequence, sequence + 1,
                    std::memory_order_seq_cst)) { }
        }
#endif
        struct RepaintBatch
        {
            std::array<NativeTextBitmapCache::RequestRepaint, DrawPrewarmSlots + LoadingPrewarmSlots + OrdinaryPrewarmSlots + CommandPrewarmSlots + TablePrewarmSlots> callbacks;
            size_t count{};
            void Add(NativeTextBitmapCache::RequestRepaint &callback) noexcept
            {
                if (callback && count < callbacks.size()) callbacks[count++] = std::move(callback);
            }
            ~RepaintBatch()
            {
                for (size_t i = 0; i < count; ++i)
                    try { callbacks[i](); } catch (...) {}
            }
        };
        struct RenderParameters
        {
            bool present{};
            uint32_t versions{};
            float gamma{}, contrast{}, clearType{}, grayContrast{};
            DWRITE_PIXEL_GEOMETRY pixels{};
            DWRITE_RENDERING_MODE mode{};
            DWRITE_GRID_FIT_MODE gridFit{};
            DWRITE_RENDERING_MODE1 mode1{};
            bool operator==(RenderParameters const &) const = default;
        };
        RenderParameters Parameters(IDWriteRenderingParams *parameters)
        {
            RenderParameters result;
            if (!parameters) return result;
            result.present = true;
            result.gamma = parameters->GetGamma();
            result.contrast = parameters->GetEnhancedContrast();
            result.clearType = parameters->GetClearTypeLevel();
            result.pixels = parameters->GetPixelGeometry();
            result.mode = parameters->GetRenderingMode();
            winrt::com_ptr<IDWriteRenderingParams> owner;
            owner.copy_from(parameters);
            if (auto value = owner.try_as<IDWriteRenderingParams1>())
            {
                result.versions |= 1;
                result.grayContrast = value->GetGrayscaleEnhancedContrast();
            }
            if (auto value = owner.try_as<IDWriteRenderingParams2>())
            {
                result.versions |= 2;
                result.gridFit = value->GetGridFitMode();
            }
            if (auto value = owner.try_as<IDWriteRenderingParams3>())
            {
                result.versions |= 4;
                result.mode1 = value->GetRenderingMode1();
            }
            if (!std::isfinite(result.gamma) || !std::isfinite(result.contrast) ||
                !std::isfinite(result.clearType) || !std::isfinite(result.grayContrast))
                throw winrt::hresult_error(E_INVALIDARG);
            return result;
        }
        struct Key
        {
            std::array<uint8_t, 16> document{};
            uint64_t revision{}, generation{}, theme{}, start{}, end{};
            uint32_t profile{}, extensions{}, node{}, textIndex{};
            double width{};
            float maximumWidth{}, maximumHeight{}, dpi{};
            D2D1_ALPHA_MODE alpha{};
            D2D1_TEXT_ANTIALIAS_MODE textAA{};
            D2D1_ANTIALIAS_MODE geometryAA{};
            RenderParameters parameters;
            std::array<float, 4> foreground{};
            std::array<uint8_t, 4> linkForeground{};
            double phaseX{}, phaseY{};
            double originPhaseX{}, originPhaseY{}, transformPhaseX{}, transformPhaseY{};
            bool operator==(Key const &) const = default;
            bool SameFailureEpoch(Key other) const noexcept
            {
                other.phaseX = phaseX;
                other.phaseY = phaseY;
                other.originPhaseX = originPhaseX;
                other.originPhaseY = originPhaseY;
                other.transformPhaseX = transformPhaseX;
                other.transformPhaseY = transformPhaseY;
                return *this == other;
            }
        };
        // Scratch targets must be detached on every exit, including failed EndDraw.
        struct ScratchTarget
        {
            ID2D1DeviceContext *context;
            bool drawing{};
            explicit ScratchTarget(ID2D1DeviceContext *value) noexcept : context(value) {}
            void Begin() noexcept { context->BeginDraw(); drawing = true; }
            void End()
            {
                drawing = false;
                const auto status = context->EndDraw();
                context->SetTarget(nullptr);
                winrt::check_hresult(status);
            }
            ~ScratchTarget()
            {
                if (drawing) (void)context->EndDraw();
                context->SetTarget(nullptr);
            }
        };
        struct RestoreTransform
        {
            ID2D1DeviceContext *context;
            D2D1_MATRIX_3X2_F transform;
            explicit RestoreTransform(ID2D1DeviceContext *value) noexcept : context(value)
            {
                context->GetTransform(&transform);
            }
            ~RestoreTransform() noexcept { context->SetTransform(transform); }
        };
        bool FractionalScale(float dpi) noexcept
        {
            const auto scale = double{dpi} / 96.0;
            return scale != std::floor(scale);
        }
        // A synchronous, stack-only observer, like the script-layout font
        // probe. Read DWrite's actual fallback glyph runs rather than assuming
        // that the layout's base font size applies to every face and span.
        class IntegralPixelRuns final : public IDWriteTextRenderer
        {
            double _scale;
          public:
            bool found{}, integral{true};
            explicit IntegralPixelRuns(double scale) noexcept : _scale(scale) {}
            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **output) noexcept override
            {
                if (!output) return E_POINTER;
                *output = nullptr;
                if (iid != __uuidof(IUnknown) && iid != __uuidof(IDWritePixelSnapping) &&
                    iid != __uuidof(IDWriteTextRenderer)) return E_NOINTERFACE;
                *output = static_cast<IDWriteTextRenderer *>(this);
                AddRef();
                return S_OK;
            }
            ULONG STDMETHODCALLTYPE AddRef() noexcept override { return 1; }
            ULONG STDMETHODCALLTYPE Release() noexcept override { return 1; }
            HRESULT STDMETHODCALLTYPE IsPixelSnappingDisabled(void *, BOOL *value) noexcept override
            {
                if (!value) return E_POINTER;
                *value = TRUE;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetCurrentTransform(void *, DWRITE_MATRIX *value) noexcept override
            {
                if (!value) return E_POINTER;
                *value = {1, 0, 0, 1, 0, 0};
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetPixelsPerDip(void *, FLOAT *value) noexcept override
            {
                if (!value) return E_POINTER;
                *value = 1;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE DrawGlyphRun(void *, FLOAT, FLOAT, DWRITE_MEASURING_MODE,
                DWRITE_GLYPH_RUN const *run, DWRITE_GLYPH_RUN_DESCRIPTION const *, IUnknown *) noexcept override
            {
                if (!run || !run->fontFace) return E_POINTER;
                const auto pixels = double{run->fontEmSize} * _scale;
                found = true;
                integral &= std::isfinite(pixels) && pixels > 0 && pixels == std::floor(pixels);
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE DrawUnderline(void *, FLOAT, FLOAT, DWRITE_UNDERLINE const *, IUnknown *) noexcept override { return S_OK; }
            HRESULT STDMETHODCALLTYPE DrawStrikethrough(void *, FLOAT, FLOAT, DWRITE_STRIKETHROUGH const *, IUnknown *) noexcept override { return S_OK; }
            HRESULT STDMETHODCALLTYPE DrawInlineObject(void *, FLOAT, FLOAT, IDWriteInlineObject *, BOOL, BOOL, IUnknown *) noexcept override { return E_NOTIMPL; }
        };
        bool HasIntegralPixelRuns(IDWriteTextLayout *layout, double scale) noexcept
        {
            IntegralPixelRuns probe{scale};
            return SUCCEEDED(layout->Draw(nullptr, &probe, 0, 0)) && probe.found && probe.integral;
        }
    }

    struct NativeTextBitmapCache::State
    {
        static constexpr uint16_t Capacity = 2048;

        struct Entry
        {
            std::weak_ptr<MainTextSnapshot const> owner;
            Key key;
            // Exactly one resident payload is present; failed epochs have neither.
            winrt::com_ptr<ID2D1Bitmap1> bitmap;
            winrt::com_ptr<ID2D1CommandList> commands;
            double left{}, top{}; // Outward pixel bounds relative to floor of the source origin in pixels.
            double recordedFloorX{}, recordedFloorY{}; // Combined origin/transform physical-pixel floors.
            uint64_t bytes{}, stamp{};
            uint32_t commandTextUnits{};
            uint16_t next{};
            bool used{}, failed{};
        };
        std::array<Entry, Capacity> entries;
        // One-based slot numbers keep zero as the end of each collision chain.
        std::array<uint16_t, Capacity> heads{};
        winrt::com_ptr<ID2D1Device> device;
        winrt::com_ptr<IUnknown> deviceIdentity;
        winrt::com_ptr<ID2D1DeviceContext> scratch;
        winrt::com_ptr<IDWriteRenderingParams> parameters;
        ID2D1DeviceContext *source{}; // Borrowed only for the current synchronous draw.
        D2D1_MATRIX_3X2_F transform{};
        Key sourceKey;
        D2D1_POINT_2F drawOrigin{};
        uint64_t clock{};
        bool ready{};
        std::weak_ptr<NativeTextPrewarm> prewarm;
        winrt::com_ptr<IDXGIAdapter1> adapter;
        bool adapterChecked{}, canPrepareCommands{};
        LUID adapterLuid{};

        uint16_t &Head(Key const &key) noexcept
        {
            return heads[(key.node * 31u + key.textIndex) % heads.size()];
        }
        void Insert(Entry &entry, Key const &key) noexcept
        {
            auto &head = Head(key);
            entry.key = key;
            entry.used = true;
            entry.next = head;
            head = static_cast<uint16_t>(&entry - entries.data() + 1);
        }
        void Drop(Entry &entry, Statistics &statistics) noexcept
        {
            auto *link = &Head(entry.key);
            while (&entries[*link - 1] != &entry)
                link = &entries[*link - 1].next;
            *link = entry.next;
            if (entry.bitmap)
                statistics.bytes -= entry.bytes;
            if (entry.commands)
            {
                --statistics.commandEntries;
                statistics.commandTextUnits -= entry.commandTextUnits;
            }
            if (entry.bitmap || entry.commands)
                --statistics.entries;
            entry = {};
        }
        void Sweep(Statistics &statistics) noexcept
        {
            for (auto &entry : entries)
                if (entry.used && entry.owner.expired()) Drop(entry, statistics);
        }
        Entry &Slot(uint64_t bytes, Statistics &statistics, uint32_t commandTextUnits = 0) noexcept
        {
            auto oldest = [&]() -> Entry &
            {
                return *std::min_element(entries.begin(), entries.end(), [](Entry const &a, Entry const &b)
                { return a.stamp < b.stamp; });
            };
            while (statistics.bytes > MaximumBytes - bytes)
            {
                Entry *victim{};
                for (auto &entry : entries)
                    if (entry.bitmap && (!victim || entry.stamp < victim->stamp)) victim = &entry;
                Drop(*victim, statistics);
            }
            while (commandTextUnits && (statistics.commandEntries >= MaximumCommandEntries ||
                   statistics.commandTextUnits > MaximumCommandTextUnits - commandTextUnits))
            {
                Entry *victim{};
                for (auto &entry : entries)
                    if (entry.commands && (!victim || entry.stamp < victim->stamp)) victim = &entry;
                Drop(*victim, statistics);
            }
            auto available = std::find_if(entries.begin(), entries.end(), [](Entry const &entry) { return !entry.used; });
            if (available != entries.end()) return *available;
            auto &entry = oldest();
            Drop(entry, statistics);
            return entry;
        }
        bool Capture(ID2D1DeviceContext *context)
        {
            ready = false;
            source = context;
            parameters = nullptr;
            if (context->GetUnitMode() != D2D1_UNIT_MODE_DIPS ||
                context->GetPrimitiveBlend() != D2D1_PRIMITIVE_BLEND_SOURCE_OVER) return false;
            context->GetTransform(&transform);
            if (transform._11 != 1 || transform._12 != 0 || transform._21 != 0 || transform._22 != 1 ||
                !std::isfinite(transform._31) || !std::isfinite(transform._32)) return false;
            float dpiX{}, dpiY{};
            context->GetDpi(&dpiX, &dpiY);
            if (!(dpiX > 0) || !std::isfinite(dpiX) || dpiX != dpiY) return false;
            const auto format = context->GetPixelFormat();
            if (format.format != DXGI_FORMAT_B8G8R8A8_UNORM) return false;
            if (format.alphaMode != D2D1_ALPHA_MODE_PREMULTIPLIED && format.alphaMode != D2D1_ALPHA_MODE_STRAIGHT &&
                format.alphaMode != D2D1_ALPHA_MODE_IGNORE) return false;
            winrt::com_ptr<ID2D1Image> target;
            context->GetTarget(target.put());
            auto bitmap = target.try_as<ID2D1Bitmap1>();
            if (!bitmap) return false;
            winrt::com_ptr<ID2D1ColorContext> color;
            bitmap->GetColorContext(color.put());
            if (color) return false;
            context->GetTextRenderingParams(parameters.put());
            auto values = Parameters(parameters.get());
            if (values.mode < DWRITE_RENDERING_MODE_DEFAULT || values.mode > DWRITE_RENDERING_MODE_OUTLINE ||
                ((values.versions & 4) && (values.mode1 < DWRITE_RENDERING_MODE1_DEFAULT ||
                 values.mode1 > DWRITE_RENDERING_MODE1_NATURAL_SYMMETRIC_DOWNSAMPLED))) return false;
            if (values.mode == DWRITE_RENDERING_MODE_OUTLINE ||
                ((values.versions & 4) && values.mode1 == DWRITE_RENDERING_MODE1_OUTLINE)) return false;
            auto aa = context->GetTextAntialiasMode();
            if (aa == D2D1_TEXT_ANTIALIAS_MODE_DEFAULT)
            {
                if (values.mode == DWRITE_RENDERING_MODE_ALIASED ||
                    ((values.versions & 4) && values.mode1 == DWRITE_RENDERING_MODE1_ALIASED))
                    aa = D2D1_TEXT_ANTIALIAS_MODE_ALIASED;
                else if (format.alphaMode == D2D1_ALPHA_MODE_PREMULTIPLIED || format.alphaMode == D2D1_ALPHA_MODE_STRAIGHT)
                    aa = D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE;
            }
            if (aa != D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE && aa != D2D1_TEXT_ANTIALIAS_MODE_ALIASED) return false;
            sourceKey = {};
            sourceKey.dpi = dpiX;
            sourceKey.alpha = format.alphaMode;
            sourceKey.textAA = aa;
            sourceKey.geometryAA = context->GetAntialiasMode();
            sourceKey.parameters = values;
            ready = true;
            return true;
        }
        winrt::com_ptr<ID2D1Bitmap1> Build(Key const &key, IDWriteTextLayout *layout,
                                          ID2D1SolidColorBrush *foreground, D2D1_POINT_2F origin,
                                          double &left, double &top, uint64_t &bytes,
                                          winrt::com_ptr<ID2D1CommandList> *retainedCommands = nullptr,
                                          uint64_t maximumBytes = MaximumEntryBytes,
                                          NativeScriptDrawing const *script = nullptr, ScriptPaint const *scriptPaint = nullptr)
        {
            if (!scratch) winrt::check_hresult(device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, scratch.put()));
            scratch->SetDpi(key.dpi, key.dpi);
            scratch->SetUnitMode(D2D1_UNIT_MODE_DIPS);
            // Keep DWrite's actual origin and transform allocation unchanged.
            // Equal summed fractional phases alone do not imply equal grid fitting.
            scratch->SetTransform(transform);
            scratch->SetPrimitiveBlend(D2D1_PRIMITIVE_BLEND_SOURCE_OVER);
            scratch->SetAntialiasMode(key.geometryAA);
            scratch->SetTextAntialiasMode(key.textAA);
            scratch->SetTextRenderingParams(parameters.get()); // nullptr explicitly resets old scratch state.
            const auto scale = double{key.dpi} / 96.0;
            auto drawText = [&]
            {
                if (script) script->Draw(scratch.get(), layout, origin, *scriptPaint);
                else scratch->DrawTextLayout(origin, layout, foreground,
                    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT | D2D1_DRAW_TEXT_OPTIONS_CLIP);
            };
            winrt::com_ptr<ID2D1CommandList> list;
            winrt::check_hresult(scratch->CreateCommandList(list.put()));
            {
                ScratchTarget target{scratch.get()};
                scratch->SetTarget(list.get());
                target.Begin();
                // Never Clear a command list or bake the first viewport's clip.
                drawText();
                target.End();
            }
            winrt::check_hresult(list->Close());
            if (retainedCommands)
            {
                // Replay these already transformed glyph commands on the original
                // target. Their resources have separate count/text-complexity limits.
                *retainedCommands = std::move(list);
                return {};
            }
            // Recorded commands already contain the source transform. Bounds
            // and the raster pass must not apply that transform a second time.
            scratch->SetTransform(D2D1::Matrix3x2F::Identity());
            D2D1_RECT_F bounds{};
            winrt::check_hresult(scratch->GetImageLocalBounds(list.get(), &bounds));
            if (!std::isfinite(bounds.left) || !std::isfinite(bounds.top) ||
                !std::isfinite(bounds.right) || !std::isfinite(bounds.bottom) ||
                bounds.left >= bounds.right || bounds.top >= bounds.bottom) throw winrt::hresult_error(E_INVALIDARG);
            // At fractional scales, command-list playback can change glyph
            // hinting (125%, 175% and 250% have fixed independent regressions).
            // Use it only for outward bounds, then rasterize the actual layout.
            // Its crop translation must preserve both the DIP and physical
            // pixel phases: four whole DIPs are five whole pixels at 125%.
            // Integer scales, including the measured 2x path, stay unchanged.
            const auto directRaster = script || FractionalScale(key.dpi);
            D2D1_MATRIX_3X2_F rasterTransform{};
            if (directRaster)
            {
                if (key.dpi != std::floor(key.dpi) || key.dpi > 65535.0f)
                    throw winrt::hresult_error(E_NOTIMPL);
                const double dipStep = 96u / std::gcd(96u, static_cast<uint32_t>(key.dpi));
                auto leftDip = (std::floor(double{bounds.left} / dipStep) - 2) * dipStep;
                auto topDip = (std::floor(double{bounds.top} / dipStep) - 2) * dipStep;
                rasterTransform = transform;
                auto translatedX = double{transform._31} - leftDip;
                auto translatedY = double{transform._32} - topDip;
                // An uncropped nonnegative axis preserves a small fractional
                // SIS translation exactly. It still obeys the same bitmap and
                // byte limits, so it cannot retain an unbounded blank prefix.
                if (double{static_cast<float>(translatedX)} != translatedX && bounds.left >= 0)
                {
                    leftDip = 0;
                    translatedX = transform._31;
                }
                if (double{static_cast<float>(translatedY)} != translatedY && bounds.top >= 0)
                {
                    topDip = 0;
                    translatedY = transform._32;
                }
                rasterTransform._31 = static_cast<float>(translatedX);
                rasterTransform._32 = static_cast<float>(translatedY);
                // If neither a safe crop nor a bounded uncropped prefix can
                // preserve the input, retain the source draw.
                if (double{rasterTransform._31} != translatedX || double{rasterTransform._32} != translatedY)
                    throw winrt::hresult_error(E_INVALIDARG);
                left = leftDip * scale;
                top = topDip * scale;
                if (left != std::floor(left) || top != std::floor(top))
                    throw winrt::hresult_error(E_INVALIDARG);
            }
            else
            {
                left = std::floor(double{bounds.left} * scale) - 2;
                top = std::floor(double{bounds.top} * scale) - 2;
            }
            const auto width = std::ceil(double{bounds.right} * scale) + 2 - left;
            const auto height = std::ceil(double{bounds.bottom} * scale) + 2 - top;
            const auto maximum = scratch->GetMaximumBitmapSize();
            if (!(width > 0) || !(height > 0) || width > maximum || height > maximum ||
                width * height > maximumBytes / 4) throw winrt::hresult_error(E_INVALIDARG);
            bytes = static_cast<uint64_t>(width) * static_cast<uint64_t>(height) * 4;
            winrt::com_ptr<ID2D1Bitmap1> bitmap;
            const auto properties = D2D1::BitmapProperties1(D2D1_BITMAP_OPTIONS_TARGET,
                D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), key.dpi, key.dpi);
            winrt::check_hresult(scratch->CreateBitmap(D2D1::SizeU(static_cast<uint32_t>(width), static_cast<uint32_t>(height)),
                                                       nullptr, 0, properties, bitmap.put()));
            {
                ScratchTarget target{scratch.get()};
                scratch->SetTarget(bitmap.get());
                scratch->SetTransform(directRaster ? rasterTransform :
                    D2D1::Matrix3x2F::Translation(static_cast<float>(-left / scale), static_cast<float>(-top / scale)));
                target.Begin();
                scratch->Clear(D2D1::ColorF(0, 0, 0, 0));
                if (directRaster)
                    drawText();
                else
                    scratch->DrawImage(list.get(), nullptr, nullptr, D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                                       D2D1_COMPOSITE_MODE_SOURCE_OVER);
                target.End();
            }
            return bitmap;
        }
        void Replay(ID2D1DeviceContext *context, Entry &entry, D2D1_POINT_2F origin, bool promote = true
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            , DiagnosticBranchTimer *diagnosticReplay = nullptr
#endif
        ) noexcept
        {
            const auto scale = double{entry.key.dpi} / 96.0;
            if (entry.commands)
            {
                const auto floorX = std::floor((double{origin.x} + transform._31) * scale);
                const auto floorY = std::floor((double{origin.y} + transform._32) * scale);
                RestoreTransform restore{context};
                // The list already includes its original SIS transform. Apply only
                // the integral physical displacement, never that source transform twice.
                context->SetTransform(D2D1::Matrix3x2F::Translation(
                    static_cast<float>((floorX - entry.recordedFloorX) / scale),
                    static_cast<float>((floorY - entry.recordedFloorY) / scale)));
                context->DrawImage(entry.commands.get(), nullptr, nullptr, D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                                   D2D1_COMPOSITE_MODE_SOURCE_OVER);
            }
            else
            {
                const auto size = entry.bitmap->GetPixelSize();
                const auto left = double{origin.x} + (entry.left - entry.key.phaseX) / scale;
                const auto top = double{origin.y} + (entry.top - entry.key.phaseY) / scale;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                const auto destination = D2D1::RectF(static_cast<float>(left), static_cast<float>(top),
                    static_cast<float>(left + size.width / scale), static_cast<float>(top + size.height / scale));
                if (diagnosticReplay) diagnosticReplay->BitmapMidpoint();
                context->DrawBitmap(entry.bitmap.get(), destination, 1, D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR);
                if (diagnosticReplay) diagnosticReplay->BitmapEnd();
#else
                context->DrawBitmap(entry.bitmap.get(), D2D1::RectF(static_cast<float>(left), static_cast<float>(top),
                    static_cast<float>(left + size.width / scale), static_cast<float>(top + size.height / scale)),
                    1, D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR);
#endif
            }
            if (promote) entry.stamp = ++clock;
        }
    };

    // Independent measurement layouts share a single worker per store.
    // Bitmaps use an independent device. The two loading command slots use
    // separate contexts on a protected render device, since recorded resources
    // must stay in its resource domain. Workers retain no view or UI context.
    // Twenty-five bitmap slots at four MiB each bound stored pixels to 100 MiB,
    // plus one temporary raster and readback for the active worker.
    struct NativeTextPrewarm : std::enable_shared_from_this<NativeTextPrewarm>
    {
        struct PreparedText
        {
            Key key;
            LUID adapter{};
            std::vector<uint8_t> data;
            winrt::com_ptr<ID2D1CommandList> commands;
            winrt::com_ptr<IUnknown> deviceIdentity;
            D2D1_SIZE_U size{};
            double left{}, top{};
        };
        struct Item
        {
            Key key;
            winrt::com_ptr<IDWriteTextLayout> layout;
            // A sealed script layout owns no Main/model/renderer or UI graphics object.
            // Its permanent effects and geometry are read-only after Seal.
            std::shared_ptr<NativeScriptDrawing const> script;
            double x{}, y{};
            float localX{}, localY{};
            bool positioned{}, finished{}, working{}, nearby{}, fromLoad{}, phaseCaptured{}, commands{};
            uint64_t geometryVersion{};
            NativeTextBitmapCache::RequestRepaint repaint;
            std::shared_ptr<PreparedText> prepared;

            bool NeedsRaster() const noexcept
            {
                return layout && positioned && !finished && (!fromLoad || phaseCaptured);
            }
        };
        ModuleLifetime module;
        std::mutex mutex;
        std::weak_ptr<MainTextSnapshot const> owner;
        std::array<Item, DrawPrewarmSlots + LoadingPrewarmSlots + OrdinaryPrewarmSlots + CommandPrewarmSlots + TablePrewarmSlots> items;
        notepads_markdown_preview_key epochKey{};
        std::array<float, 4> foreground{};
        std::array<uint8_t, 4> linkForeground{};
        double scale{};
        D2D1_POINT_2F drawOrigin{};
        double drawViewportY{};
        bool originKnown{};
        Key renderSource;
        LUID renderAdapter{};
        winrt::com_ptr<IUnknown> commandDeviceIdentity;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        std::shared_ptr<NativeTextBitmapCache::PreparationGate> preparationGate;
#endif
        uint64_t epoch{};
        bool initialized{}, cancelled{}, running{};

#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        // Comparison values: unavailable0, exact1, phase-only2, non-phase3.
        // The next six bits identify differing phase fields in declaration order.
        static uint32_t DiagnosticKeyComparison(Key const &actual, Key const &requested) noexcept
        {
            const uint32_t phases = (actual.phaseX != requested.phaseX ? 1u : 0u) |
                (actual.phaseY != requested.phaseY ? 2u : 0u) |
                (actual.originPhaseX != requested.originPhaseX ? 4u : 0u) |
                (actual.originPhaseY != requested.originPhaseY ? 8u : 0u) |
                (actual.transformPhaseX != requested.transformPhaseX ? 16u : 0u) |
                (actual.transformPhaseY != requested.transformPhaseY ? 32u : 0u);
            return (actual == requested ? 1u : actual.SameFailureEpoch(requested) ? 2u : 3u) | (phases << 2);
        }
        void DiagnosticSnapshot(NativeTextBitmapCache::DiagnosticPrewarmTarget &target, Key const &key,
            Key const &source, std::shared_ptr<MainTextSnapshot const> const &main,
            bool adapterAvailable, LUID adapter)
        {
            std::lock_guard guard{mutex};
            target.storeFlags = 1u | (initialized ? 1u << 1 : 0u) | (cancelled ? 1u << 2 : 0u) |
                (owner.lock() == main ? 1u << 3 : 0u) | (originKnown ? 1u << 4 : 0u) |
                (running ? 1u << 5 : 0u) | (adapterAvailable ? 1u << 6 : 0u) |
                (adapterAvailable && renderAdapter.LowPart == adapter.LowPart && renderAdapter.HighPart == adapter.HighPart ? 1u << 7 : 0u) |
                (originKnown && renderSource == source ? 1u << 8 : 0u) |
                (std::equal(std::begin(epochKey.model.document), std::end(epochKey.model.document), key.document.begin()) &&
                 epochKey.model.revision == key.revision && epochKey.model.generation == key.generation &&
                 epochKey.model.profile == key.profile && epochKey.model.extensions == key.extensions &&
                 epochKey.theme_generation == key.theme && epochKey.width == key.width ? 1u << 9 : 0u) |
                (scale == double{source.dpi} / 96 ? 1u << 10 : 0u);
            target.storeEpoch = epoch;
            for (size_t index = 0; index < items.size(); ++index)
            {
                auto const &item = items[index];
                const auto occupied = item.layout || item.finished;
                if (occupied && (index < DrawPrewarmSlots ||
                    (index >= DrawPrewarmSlots + LoadingPrewarmSlots &&
                     index < DrawPrewarmSlots + LoadingPrewarmSlots + OrdinaryPrewarmSlots)))
                    target.occupancy |= 1u << index;
                if (!occupied || item.key.node != key.node || item.key.textIndex != key.textIndex) continue;
                const auto firstMatch = !target.matchedSlots;
                target.matchedSlots |= 1u << index;
                if (!firstMatch) continue; // Ambiguity is explicit; never OR states from different items.
                target.geometryVersion = item.geometryVersion;
                target.itemFlags = (item.layout ? 1u : 0u) | (item.positioned ? 1u << 1 : 0u) |
                    (item.fromLoad ? 1u << 2 : 0u) | (item.phaseCaptured ? 1u << 3 : 0u) |
                    (item.working ? 1u << 4 : 0u) | (item.finished ? 1u << 5 : 0u) |
                    (item.prepared ? 1u << 6 : 0u) | (item.commands ? 1u << 7 : 0u) | (item.script ? 1u << 8 : 0u);
                if (originKnown && item.positioned && scale > 0 && std::isfinite(scale))
                    target.itemKeyComparison = DiagnosticKeyComparison(DrawKey(item, renderSource, drawOrigin, scale), key);
                if (item.prepared)
                {
                    auto const &prepared = *item.prepared; // Borrow only; no payload/shared_ptr copy.
                    if (!prepared.data.empty()) target.itemFlags |= 1u << 9;
                    if (adapterAvailable && prepared.adapter.LowPart == adapter.LowPart && prepared.adapter.HighPart == adapter.HighPart)
                        target.itemFlags |= 1u << 10;
                    target.readyKeyComparison = DiagnosticKeyComparison(prepared.key, key);
                }
            }
            target.flags |= NativeTextBitmapCache::DiagnosticPrewarmTarget::ObservationComplete;
        }
#endif
        static bool ExactScale(double value) noexcept
        {
            int exponent{};
            return value > 0 && std::isfinite(value) && std::frexp(value, &exponent) == 0.5;
        }
        void Cancel(uint64_t expectedEpoch = 0) noexcept
        {
            RepaintBatch notifications;
            std::lock_guard guard{mutex};
            if (expectedEpoch && epoch != expectedEpoch) return;
            cancelled = true;
            commandDeviceIdentity = nullptr;
            ++epoch;
            for (auto &item : items) notifications.Add(item.repaint);
            items = {};
        }
        std::shared_ptr<PreparedText> Take(Key const &key, std::shared_ptr<MainTextSnapshot const> const &main,
            LUID adapter, NativeTextBitmapCache::RequestRepaint const *repaint, bool *pending, bool loadingOnly,
            bool commands, IUnknown *deviceIdentity) noexcept
        {
            std::lock_guard guard{mutex};
            if (cancelled || owner.lock() != main || (commands && commandDeviceIdentity.get() != deviceIdentity)) return {};
            for (auto &item : items)
            {
                if ((item.fromLoad ? !item.phaseCaptured : loadingOnly) || item.commands != commands) continue;
                if (item.prepared && item.prepared->key == key &&
                    item.prepared->adapter.LowPart == adapter.LowPart && item.prepared->adapter.HighPart == adapter.HighPart &&
                    (!commands || item.prepared->deviceIdentity.get() == deviceIdentity))
                    return item.fromLoad ? item.prepared : std::move(item.prepared);
                if (repaint && *repaint && pending && running && item.layout && item.positioned &&
                    !item.prepared && (!item.finished || item.working) && renderAdapter.LowPart == adapter.LowPart &&
                    renderAdapter.HighPart == adapter.HighPart && DrawKey(item, renderSource, drawOrigin, scale) == key)
                {
                    try { if (!item.repaint) item.repaint = *repaint; }
                    catch (...) { return {}; }
                    *pending = true;
                    return {};
                }
            }
            return {};
        }
        // Reuse only completed ordinary pixels outside normal cache admission.
        // Loading slots share their payload; draw slots transfer it as Take does.
        std::shared_ptr<PreparedText> TakeReadyOrdinary(Key const &key,
            std::shared_ptr<MainTextSnapshot const> const &main, LUID adapter) noexcept
        {
            std::lock_guard guard{mutex};
            if (cancelled || owner.lock() != main) return {};
            auto takeRange = [&](size_t first, size_t last, bool fromLoad) -> std::shared_ptr<PreparedText>
            {
                for (auto index = first; index < last; ++index)
                {
                    auto &item = items[index];
                    if (item.fromLoad != fromLoad || (fromLoad && !item.phaseCaptured) ||
                        item.script || item.commands || !item.prepared) continue;
                    if (item.prepared->key != key || item.prepared->adapter.LowPart != adapter.LowPart ||
                        item.prepared->adapter.HighPart != adapter.HighPart) continue;
                    return fromLoad ? item.prepared : std::move(item.prepared);
                }
                return {};
            };
            if (auto prepared = takeRange(0, DrawPrewarmSlots, false)) return prepared;
            const auto ordinary = DrawPrewarmSlots + LoadingPrewarmSlots;
            return takeRange(ordinary, ordinary + OrdinaryPrewarmSlots, true);
        }
        bool OfferScript(Key const &key, PreviewTextVisual const &text,
            std::shared_ptr<MainTextSnapshot const> const &main, bool nearby) noexcept
        {
            RepaintBatch notifications;
            std::lock_guard guard{mutex};
            if (cancelled || !originKnown || owner.lock() != main ||
                key.alpha != D2D1_ALPHA_MODE_PREMULTIPLIED || key.textAA != D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE ||
                key.parameters.present || key.dpi != renderSource.dpi || key.alpha != renderSource.alpha ||
                key.textAA != renderSource.textAA || key.geometryAA != renderSource.geometryAA ||
                key.parameters != renderSource.parameters ||
                key.transformPhaseX != 0 || key.transformPhaseY != 0 ||
                key.phaseX != key.originPhaseX || key.phaseY != key.originPhaseY) return false;
            const auto drawEnd = items.begin() + DrawPrewarmSlots;
            auto loaded = std::find_if(drawEnd, items.end(), [&](auto const &item)
                { return item.layout && item.phaseCaptured && item.key == key; });
            if (loaded != items.end())
            {
                if (!nearby) loaded->nearby = false;
                return !running && !loaded->finished;
            }
            auto slot = std::find_if(items.begin(), drawEnd, [&](auto const &item)
                { return item.layout && item.key.node == key.node && item.key.textIndex == key.textIndex; });
            if (slot != drawEnd)
            {
                if (!slot->script || (nearby && !slot->nearby)) return false;
                if (slot->key == key)
                {
                    // A genuinely visible request outranks queued nearby work.
                    if (!nearby) slot->nearby = false;
                    return !running && !slot->finished;
                }
                // The current exact phase supersedes this script's old request.
                // A copied worker observes the version change before publication.
                notifications.Add(slot->repaint);
            }
            // Preserve ordinary and unrelated not-yet-uploaded payloads.
            else
            {
                slot = std::find_if(items.begin(), drawEnd, [](auto const &item)
                    { return !item.layout || (item.finished && !item.working && !item.prepared); });
                if (slot == drawEnd && !nearby)
                {
                    // Visible work may reclaim only an unpromoted nearby script.
                    // Prefer a queued/ready item over an already running raster.
                    slot = std::find_if(items.begin(), drawEnd, [](auto const &item)
                        { return item.script && item.nearby && !item.working; });
                    if (slot == drawEnd)
                        slot = std::find_if(items.begin(), drawEnd, [](auto const &item)
                            { return item.script && item.nearby; });
                    if (slot != drawEnd) notifications.Add(slot->repaint);
                }
            }
            if (slot == drawEnd) return false;
            const auto version = slot->geometryVersion + 1;
            *slot = {};
            slot->geometryVersion = version;
            slot->key = key;
            slot->layout = text.layout;
            slot->script = text.scriptDrawing;
            slot->nearby = nearby;
            slot->positioned = true;
            return !running;
        }
        static std::pair<double, double> LoadingPhase(Item const &item, double blockX, double blockY,
            D2D1_POINT_2F origin, double viewportY, double scale) noexcept
        {
            const auto x = (origin.x + static_cast<float>(blockX)) + item.localX;
            const auto rawY = (origin.y + static_cast<float>(blockY - viewportY)) + item.localY;
            const auto units = scale * 64;
            const auto y = static_cast<float>(std::round(double{rawY} * units) / units);
            const auto pixelX = double{x} * scale;
            const auto pixelY = double{y} * scale;
            return {pixelX - std::floor(pixelX), pixelY - std::floor(pixelY)};
        }
        static D2D1_POINT_2F TextOrigin(Item const &item, D2D1_POINT_2F origin, double physicalScale) noexcept
        {
            if (item.script || item.fromLoad) return {static_cast<float>(item.key.originPhaseX / physicalScale),
                                     static_cast<float>(item.key.originPhaseY / physicalScale)};
            const auto units = std::exp2(std::ceil(std::log2(physicalScale * 64)));
            const auto normalizedY = std::round((double{origin.y} + item.y + item.localY) * units) / units;
            return {(origin.x + static_cast<float>(item.x)) + item.localX, static_cast<float>(
                normalizedY - std::floor(normalizedY * physicalScale) / physicalScale)};
        }
        static Key DrawKey(Item const &item, Key const &source, D2D1_POINT_2F origin, double physicalScale) noexcept
        {
            if (item.script || item.fromLoad) return item.key;
            auto key = item.key;
            key.dpi = source.dpi;
            key.alpha = source.alpha;
            key.textAA = source.textAA;
            key.geometryAA = source.geometryAA;
            key.parameters = source.parameters;
            const auto textOrigin = TextOrigin(item, origin, physicalScale);
            const auto pixelX = double{textOrigin.x} * physicalScale;
            const auto pixelY = double{textOrigin.y} * physicalScale;
            key.phaseX = key.originPhaseX = pixelX - std::floor(pixelX);
            key.phaseY = key.originPhaseY = pixelY - std::floor(pixelY);
            key.transformPhaseX = key.transformPhaseY = 0;
            return key;
        }
        void Run(Key source, winrt::com_ptr<IDXGIAdapter1> adapter, winrt::com_ptr<ID2D1Device> commandDevice,
                 uint64_t requestEpoch) noexcept
        {
            struct Release
            {
                NativeTextPrewarm *owner;
                ~Release()
                {
                    RepaintBatch notifications;
                    std::lock_guard guard{owner->mutex};
                    owner->running = false;
                    --prewarmWorkers;
                    for (auto &item : owner->items)
                    {
                        item.working = false;
                        notifications.Add(item.repaint);
                    }
                }
            } release{this};
            const auto apartment = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
            struct Uninitialize
            {
                HRESULT result;
                ~Uninitialize() { if (SUCCEEDED(result)) CoUninitialize(); }
            } uninitialize{apartment};
            try
            {
                winrt::check_hresult(apartment);
                {
                    std::lock_guard guard{mutex};
                    if (cancelled || owner.expired() || epoch != requestEpoch) return;
                }
                winrt::com_ptr<ID3D11Device> d3d;
                winrt::check_hresult(D3D11CreateDevice(adapter.get(), D3D_DRIVER_TYPE_UNKNOWN, nullptr,
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION, d3d.put(), nullptr, nullptr));
                winrt::com_ptr<ID2D1Factory1> factory;
                winrt::check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, factory.put()));
                auto raster = std::make_unique<NativeTextBitmapCache::State>();
                winrt::check_hresult(factory->CreateDevice(d3d.as<IDXGIDevice>().get(), raster->device.put()));
                raster->transform = D2D1::Matrix3x2F::Identity();
                std::unique_ptr<NativeTextBitmapCache::State> commandRaster;
                if (commandDevice)
                {
                    commandRaster = std::make_unique<NativeTextBitmapCache::State>();
                    commandRaster->device = std::move(commandDevice);
                    commandRaster->deviceIdentity = commandRaster->device.as<IUnknown>();
                    commandRaster->transform = D2D1::Matrix3x2F::Identity();
                }
                DXGI_ADAPTER_DESC1 adapterDescription{};
                winrt::check_hresult(adapter->GetDesc1(&adapterDescription));
                for (;;)
                {
                    Item work;
                    size_t index = items.size();
                    uint64_t expected{};
                    double physicalScale{};
                    D2D1_POINT_2F origin{};
                    {
                        std::lock_guard guard{mutex};
                        if (cancelled || owner.expired() || epoch != requestEpoch || scale != double{source.dpi} / 96) return;
                        expected = epoch;
                        physicalScale = scale;
                        origin = drawOrigin;
                        auto ready = [](Item const &item) { return item.NeedsRaster(); };
                        auto next = std::find_if(items.begin(), items.end(),
                            [&](Item const &item) { return ready(item) && !item.nearby; });
                        if (next == items.end()) next = std::find_if(items.begin(), items.end(), ready);
                        if (next != items.end())
                        {
                            next->finished = true;
                            next->working = true;
                            work = *next;
                            index = static_cast<size_t>(next - items.begin());
                        }
                    }
                    if (index == items.size()) return;
                    try
                    {
                        if (work.commands && !commandRaster) throw winrt::hresult_error(E_NOTIMPL);
                        auto &target = work.commands ? *commandRaster : *raster;
                        auto result = std::make_shared<PreparedText>();
                        auto &key = result->key;
                        key = DrawKey(work, source, origin, physicalScale);
                        const auto textOrigin = TextOrigin(work, origin, physicalScale);
                        const auto pixelX = double{textOrigin.x} * physicalScale;
                        const auto pixelY = double{textOrigin.y} * physicalScale;
                        if (!target.scratch)
                            winrt::check_hresult(target.device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE,
                                                                                  target.scratch.put()));
                        winrt::com_ptr<ID2D1SolidColorBrush> brush;
                        winrt::check_hresult(target.scratch->CreateSolidColorBrush(D2D1::ColorF(
                            key.foreground[0], key.foreground[1], key.foreground[2], key.foreground[3]), brush.put()));
                        double left{}, top{};
                        uint64_t bytes{};
                        ScriptPaint scriptPaint;
                        scriptPaint.foreground = brush.get();
                        winrt::com_ptr<ID2D1SolidColorBrush> link;
                        if (work.script)
                        {
                            auto const &c = key.linkForeground;
                            winrt::check_hresult(target.scratch->CreateSolidColorBrush(D2D1::ColorF(
                                c[0] / 255.0f, c[1] / 255.0f, c[2] / 255.0f, c[3] / 255.0f), link.put()));
                            scriptPaint.link = link.get();
                        }
                        auto bitmap = target.Build(key, work.layout.get(), brush.get(), textOrigin, left, top, bytes,
                            work.commands ? &result->commands : nullptr, MaximumPrewarmEntryBytes, work.script.get(), &scriptPaint);
                        if (!work.commands && (!bytes || bytes > MaximumPrewarmEntryBytes)) continue;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                        std::shared_ptr<NativeTextBitmapCache::PreparationGate> gate;
#endif
                        {
                            std::lock_guard guard{mutex};
                            if (cancelled || epoch != expected || owner.expired() ||
                                items[index].geometryVersion != work.geometryVersion) continue;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                            gate = preparationGate;
#endif
                        }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                        if (gate)
                        {
                            gate->entered.store(true, std::memory_order_release);
                            const auto deadline = GetTickCount64() + 5000;
                            bool stale{};
                            while (!gate->released.load(std::memory_order_acquire) && GetTickCount64() < deadline)
                            {
                                {
                                    std::lock_guard guard{mutex};
                                    stale = cancelled || epoch != expected || owner.expired() ||
                                        items[index].geometryVersion != work.geometryVersion;
                                }
                                if (stale) break;
                                Sleep(1);
                            }
                            if (stale) continue;
                            if (gate->failBeforePublish.load(std::memory_order_acquire))
                                throw winrt::hresult_error(E_FAIL);
                        }
#endif
                        result->adapter = adapterDescription.AdapterLuid;
                        if (work.commands)
                            result->deviceIdentity = target.deviceIdentity;
                        else
                        {
                            result->size = bitmap->GetPixelSize();
                            result->left = left - std::floor(pixelX);
                            result->top = top - std::floor(pixelY);
                            const auto properties = D2D1::BitmapProperties1(
                                D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                                D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), key.dpi, key.dpi);
                            winrt::com_ptr<ID2D1Bitmap1> readback;
                            winrt::check_hresult(raster->scratch->CreateBitmap(result->size, nullptr, 0, properties, readback.put()));
                            winrt::check_hresult(readback->CopyFromBitmap(nullptr, bitmap.get(), nullptr));
                            result->data.resize(static_cast<size_t>(bytes));
                            D2D1_MAPPED_RECT mapped{};
                            winrt::check_hresult(readback->Map(D2D1_MAP_OPTIONS_READ, &mapped));
                            struct Unmap
                            {
                                ID2D1Bitmap1 *bitmap;
                                ~Unmap() { (void)bitmap->Unmap(); }
                            } unmap{readback.get()};
                            for (UINT32 row = 0; row < result->size.height; ++row)
                                std::memcpy(result->data.data() + static_cast<size_t>(row) * result->size.width * 4,
                                            mapped.bits + static_cast<size_t>(row) * mapped.pitch, result->size.width * 4);
                        }
                        std::lock_guard guard{mutex};
                        if (!cancelled && epoch == expected && !owner.expired() &&
                            items[index].geometryVersion == work.geometryVersion)
                        {
                            // Keep only this independent measurement layout
                            // so a new real surface-origin phase can be prepared.
                            items[index].prepared = std::move(result);
                        }
                    }
                    catch (...) { /* Optional preparation never changes the authoritative draw. */ }
                    {
                        RepaintBatch notifications;
                        std::lock_guard guard{mutex};
                        if (epoch == expected && items[index].geometryVersion == work.geometryVersion)
                        {
                            items[index].working = false;
                            notifications.Add(items[index].repaint);
                        }
                    }
                }
            }
            catch (...)
            {
                // A failure from an older device/source must not cancel the
                // current preparation epoch installed while this worker ran.
                Cancel(requestEpoch);
            }
        }
    };

    std::shared_ptr<NativeTextPrewarm> NativeTextBitmapCache::CreatePrewarm() noexcept
    {
        try { return std::make_shared<NativeTextPrewarm>(); }
        catch (...) { return {}; }
    }
    void NativeTextBitmapCache::BeginPrewarm(std::shared_ptr<NativeTextPrewarm> const &prewarm,
        notepads_markdown_preview_key const &key, PreviewTheme const &theme,
        std::shared_ptr<MainTextSnapshot const> const &main) noexcept
    {
        if (!prewarm || !main) return;
        RepaintBatch notifications;
        std::lock_guard guard{prewarm->mutex};
        if (prewarm->initialized && prewarm->owner.lock() == main &&
            std::memcmp(&prewarm->epochKey.model, &key.model, sizeof(key.model)) == 0 &&
            prewarm->epochKey.theme_generation == key.theme_generation && prewarm->epochKey.width == key.width &&
            prewarm->scale == theme.physicalScale) return;
        prewarm->initialized = true;
        prewarm->cancelled = !NativeTextPrewarm::ExactScale(theme.physicalScale);
        ++prewarm->epoch;
        for (auto &item : prewarm->items) notifications.Add(item.repaint);
        prewarm->items = {};
        prewarm->owner = main;
        prewarm->epochKey = key;
        prewarm->scale = theme.physicalScale;
        prewarm->originKnown = false;
        prewarm->commandDeviceIdentity = nullptr;
        auto color = theme.foreground;
        prewarm->foreground = {color.R / 255.0f, color.G / 255.0f, color.B / 255.0f, color.A / 255.0f};
        const auto link = theme.linkForeground;
        prewarm->linkForeground = {link.R, link.G, link.B, link.A};
    }
    void NativeTextBitmapCache::CollectPrewarm(std::shared_ptr<NativeTextPrewarm> const &prewarm,
                                               PreviewBlockVisual const &block) noexcept
    {
        if (!prewarm) return;
        try
        {
            std::lock_guard guard{prewarm->mutex};
            if (prewarm->cancelled) return;
            auto main = prewarm->owner.lock();
            if (!main) return;
            for (size_t index = 0; index < block.texts.size(); ++index)
            {
                auto const &text = block.texts[index];
                const auto loadingScript = block.block.kind == 1 && block.texts.size() == 1 &&
                    text.scriptDrawing && text.scripts && text.scripts->RubyParts().empty() &&
                    (prewarm->scale == 1 || prewarm->scale == 2);
                const auto narrowTableText = block.block.kind == 6 && text.layout && text.prewarmCandidate &&
                    !text.scriptDrawing && !text.scripts && text.links.empty() && text.utf16Length >= 16 &&
                    text.layout->GetMaxWidth() <= 128 && (prewarm->scale == 1 || prewarm->scale == 2);
                const auto openingOrdinary = block.block.kind == 1 && block.texts.size() == 1 &&
                    text.bitmapCandidate && !text.scriptDrawing && !text.scripts && text.links.empty() &&
                    (prewarm->scale == 1 || prewarm->scale == 2);
                if (!text.layout || !text.matchesMainText ||
                    (!loadingScript && !openingOrdinary && !narrowTableText && (!text.prewarmCandidate || text.scriptDrawing || text.scripts ||
                        !text.links.empty() || !text.inlineCode.empty())) || !text.images.empty() || !text.math.empty() ||
                    !text.diagrams.empty() || !text.utf16Length || text.utf16Length > 512 ||
                    !(text.height > 0) || text.height > 2048 || !std::isfinite(text.height) ||
                    text.logicalEnd > main->Length() || text.logicalEnd - text.logicalStart != text.utf16Length) continue;
                auto content = main->Text().substr(static_cast<size_t>(text.logicalStart), text.utf16Length);
                const auto supplementary = std::count_if(content.begin(), content.end(),
                    [](wchar_t value) { return value >= 0xd800 && value <= 0xdbff; });
                // Fill only the short-ZWJ gap below the existing >=8 route.
                // Keep longer/general supplementary text and all slot budgets unchanged.
                const auto loadingBitmap = openingOrdinary && text.prewarmCandidate &&
                    ((!supplementary && content.find(L'\ufe0f') != std::wstring_view::npos) ||
                     (content.size() <= 128 && supplementary >= 2 && supplementary < 8 &&
                      content.find(L'\u200d') != std::wstring_view::npos));
                const auto loadingCommand = openingOrdinary && !text.prewarmCandidate && supplementary > 0;
                const auto loadingOrdinary = loadingBitmap || loadingCommand || narrowTableText;
                if (!loadingOrdinary && supplementary < (loadingScript ? 1 : 8)) continue;
                if (!loadingScript && !loadingOrdinary && !text.prewarmCandidate) continue;
                if (std::any_of(prewarm->items.begin(), prewarm->items.end(), [&](auto const &item)
                    { return (item.layout || item.finished) && item.key.node == block.block.node && item.key.textIndex == index; })) continue;
                const auto split = prewarm->items.begin() + DrawPrewarmSlots;
                const auto ordinary = split + LoadingPrewarmSlots;
                const auto command = ordinary + OrdinaryPrewarmSlots;
                const auto table = command + CommandPrewarmSlots;
                const auto start = narrowTableText ? table : loadingCommand ? command : loadingBitmap ? ordinary :
                    loadingScript ? split : prewarm->items.begin();
                const auto end = narrowTableText ? prewarm->items.end() : loadingCommand ? table : loadingBitmap ? command :
                    loadingScript ? ordinary : split;
                auto available = std::find_if(start, end,
                    [](auto const &item) { return !item.layout && !item.finished; });
                if (available == end) continue;
                auto &item = *available;
                auto &key = item.key;
                std::copy(std::begin(prewarm->epochKey.model.document), std::end(prewarm->epochKey.model.document), key.document.begin());
                key.revision = prewarm->epochKey.model.revision;
                key.generation = prewarm->epochKey.model.generation;
                key.profile = prewarm->epochKey.model.profile;
                key.extensions = prewarm->epochKey.model.extensions;
                key.theme = prewarm->epochKey.theme_generation;
                key.width = prewarm->epochKey.width;
                key.node = block.block.node;
                key.textIndex = static_cast<uint32_t>(index);
                key.start = text.logicalStart;
                key.end = text.logicalEnd;
                key.maximumWidth = text.layout->GetMaxWidth();
                key.maximumHeight = text.layout->GetMaxHeight();
                key.foreground = prewarm->foreground;
                item.layout = text.layout;
                if (loadingScript || loadingOrdinary)
                {
                    item.commands = loadingCommand;
                    if (!text.links.empty()) key.linkForeground = prewarm->linkForeground;
                    item.script = text.scriptDrawing;
                    item.fromLoad = true;
                    item.nearby = true;
                }
                item.localX = text.x;
                item.localY = text.y;
            }
        }
        catch (...) { /* Optional candidates are bounded independently of layout. */ }
    }
    void NativeTextBitmapCache::PositionPrewarm(std::shared_ptr<NativeTextPrewarm> const &prewarm,
        std::span<notepads_markdown_preview_block const> blocks) noexcept
    {
        if (!prewarm) return;
        RepaintBatch notifications;
        std::lock_guard guard{prewarm->mutex};
        if (prewarm->cancelled) return;
        for (auto const &block : blocks)
            for (auto &item : prewarm->items)
                if (item.layout && (!item.script || item.fromLoad) && item.key.node == block.node)
                {
                    if (item.fromLoad && item.phaseCaptured && (item.x != block.x || item.y != block.y))
                    {
                        const auto [x, y] = NativeTextPrewarm::LoadingPhase(item, block.x, block.y,
                            prewarm->drawOrigin, prewarm->drawViewportY, prewarm->scale);
                        if (x != item.key.originPhaseX || y != item.key.originPhaseY)
                        {
                            notifications.Add(item.repaint);
                            ++item.geometryVersion;
                            item.phaseCaptured = item.finished = item.working = false;
                            item.nearby = true;
                            item.prepared.reset();
                        }
                    }
                    if (!item.fromLoad && prewarm->originKnown && item.positioned)
                    {
                        const auto previous = NativeTextPrewarm::DrawKey(item, prewarm->renderSource,
                                                                          prewarm->drawOrigin, prewarm->scale);
                        item.x = block.x;
                        item.y = block.y;
                        // Integral physical translations keep the same actual
                        // raster input. A changed phase invalidates only this
                        // item, including an older result still being drawn.
                        if (previous != NativeTextPrewarm::DrawKey(item, prewarm->renderSource,
                                                                 prewarm->drawOrigin, prewarm->scale))
                        {
                            notifications.Add(item.repaint);
                            ++item.geometryVersion;
                            item.finished = item.working = false;
                            item.prepared.reset();
                        }
                    }
                    item.x = block.x;
                    item.y = block.y;
                    item.positioned = std::isfinite(item.x) && std::isfinite(item.y);
                }
    }
    void NativeTextBitmapCache::PrepareFrame(PreviewFrame const &frame, D2D1_POINT_2F drawOrigin) noexcept
    {
        if (!_state || !_state->ready || !frame.textPrewarm ||
            !std::isfinite(drawOrigin.x) || !std::isfinite(drawOrigin.y)) return;
        auto &state = *_state;
        state.drawOrigin = drawOrigin;
        const auto scale = double{state.sourceKey.dpi} / 96;
        if (!NativeTextPrewarm::ExactScale(scale) || state.sourceKey.alpha != D2D1_ALPHA_MODE_PREMULTIPLIED ||
            state.sourceKey.textAA != D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE || state.sourceKey.parameters.present ||
            std::floor(double{state.transform._31} * scale) != double{state.transform._31} * scale ||
            std::floor(double{state.transform._32} * scale) != double{state.transform._32} * scale) return;
        RepaintBatch notifications;
        try
        {
            if (!state.adapterChecked)
            {
                state.adapterChecked = true;
                auto device = state.device.try_as<ID2D1Device2>();
                if (!device) return;
                winrt::com_ptr<IDXGIDevice> dxgi;
                winrt::check_hresult(device->GetDxgiDevice(dxgi.put()));
                winrt::com_ptr<IDXGIAdapter> adapter;
                winrt::check_hresult(dxgi->GetAdapter(adapter.put()));
                auto typed = adapter.as<IDXGIAdapter1>();
                DXGI_ADAPTER_DESC1 description{};
                winrt::check_hresult(typed->GetDesc1(&description));
                if (!(description.Flags & DXGI_ADAPTER_FLAG_SOFTWARE))
                {
                    state.adapter = std::move(typed);
                    state.adapterLuid = description.AdapterLuid;
                    winrt::com_ptr<ID2D1Factory> factory;
                    state.device->GetFactory(factory.put());
                    if (auto threading = factory.try_as<ID2D1Multithread>())
                        state.canPrepareCommands = threading->GetMultithreadProtected() != FALSE;
                }
            }
            if (!state.adapter) return;
            auto store = frame.textPrewarm;
            std::lock_guard guard{store->mutex};
            if (store->cancelled || store->scale != scale || store->owner.lock() != frame.main) return;
            const auto commandIdentity = state.canPrepareCommands ? state.deviceIdentity.get() : nullptr;
            if (!store->originKnown || store->renderSource != state.sourceKey ||
                store->commandDeviceIdentity.get() != commandIdentity ||
                store->renderAdapter.LowPart != state.adapterLuid.LowPart ||
                store->renderAdapter.HighPart != state.adapterLuid.HighPart)
            {
                store->originKnown = true;
                store->renderSource = state.sourceKey;
                store->renderAdapter = state.adapterLuid;
                store->commandDeviceIdentity.copy_from(commandIdentity);
                ++store->epoch;
                for (auto &item : store->items)
                {
                    notifications.Add(item.repaint);
                    item.phaseCaptured = item.finished = item.working = false;
                    if (item.fromLoad) item.nearby = true;
                    item.prepared.reset();
                }
            }
            else if (store->drawOrigin.x != drawOrigin.x || store->drawOrigin.y != drawOrigin.y)
            {
                // Preserve worker results across motion that leaves their actual
                // raster keys unchanged. Script requests carry their own phase.
                for (auto &item : store->items)
                {
                    if (!item.layout || !item.positioned || item.script) continue;
                    const auto previous = NativeTextPrewarm::DrawKey(item, state.sourceKey, store->drawOrigin, scale);
                    if (previous == NativeTextPrewarm::DrawKey(item, state.sourceKey, drawOrigin, scale)) continue;
                    notifications.Add(item.repaint);
                    ++item.geometryVersion;
                    item.finished = item.working = false;
                    item.prepared.reset();
                }
            }
            store->drawOrigin = drawOrigin;
            store->drawViewportY = frame.viewportY;
            for (auto &item : store->items)
            {
                if (!item.fromLoad || !item.positioned || item.phaseCaptured ||
                    (item.commands && !commandIdentity)) continue;
                auto &key = item.key;
                key.dpi = state.sourceKey.dpi;
                key.alpha = state.sourceKey.alpha;
                key.textAA = state.sourceKey.textAA;
                key.geometryAA = state.sourceKey.geometryAA;
                key.parameters = state.sourceKey.parameters;
                const auto [x, y] = NativeTextPrewarm::LoadingPhase(item, item.x, item.y, drawOrigin, frame.viewportY, scale);
                key.phaseX = key.originPhaseX = x;
                key.phaseY = key.originPhaseY = y;
                key.transformPhaseX = key.transformPhaseY = 0;
                item.phaseCaptured = true;
            }
            if (store->running || std::none_of(store->items.begin(), store->items.end(),
                             [](auto const &item) { return item.NeedsRaster(); })) return;
            auto active = prewarmWorkers.load(std::memory_order_relaxed);
            do { if (active >= 2) return; }
            while (!prewarmWorkers.compare_exchange_weak(active, active + 1, std::memory_order_acq_rel));
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            NotePrewarmAdmission(); // Before dispatch, including a failed dispatch.
#endif
            store->running = true;
            state.prewarm = store;
            try
            {
                winrt::com_ptr<ID2D1Device> commandDevice;
                if (commandIdentity)
                    commandDevice = state.device;
                winrt::Windows::System::Threading::ThreadPool::RunAsync(
                    [store, source = state.sourceKey, adapter = state.adapter, commandDevice, epoch = store->epoch](auto const &)
                    { store->Run(source, adapter, commandDevice, epoch); });
            }
            catch (...) { store->running = false; --prewarmWorkers; }
        }
        catch (...) { /* No asynchronous preparation is required for drawing. */ }
    }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    uint64_t NativeTextBitmapCache::DiagnosticPrewarmSequence() noexcept
    {
        if (!prewarmAdmissionSequence.is_lock_free() || !prewarmWorkers.is_lock_free())
            return (std::numeric_limits<uint64_t>::max)();
        return prewarmAdmissionSequence.load(std::memory_order_seq_cst);
    }
    uint32_t NativeTextBitmapCache::DiagnosticPrewarmWorkers() noexcept
    {
        if (!prewarmAdmissionSequence.is_lock_free() || !prewarmWorkers.is_lock_free())
            return (std::numeric_limits<uint32_t>::max)();
        return prewarmWorkers.load(std::memory_order_seq_cst);
    }
    void NativeTextBitmapCache::SetPreparationGate(PreviewFrame const &frame, std::shared_ptr<PreparationGate> gate) noexcept
    {
        if (!frame.textPrewarm) return;
        std::lock_guard guard{frame.textPrewarm->mutex};
        frame.textPrewarm->preparationGate = std::move(gate);
    }
#endif
    uint32_t NativeTextBitmapCache::PreparedCount(std::shared_ptr<NativeTextPrewarm> const &prewarm) noexcept
    {
        if (!prewarm) return 0;
        std::lock_guard guard{prewarm->mutex};
        return static_cast<uint32_t>(std::count_if(prewarm->items.begin(), prewarm->items.end(),
                                                [](auto const &item) { return bool{item.prepared}; }));
    }

    NativeTextBitmapCache::NativeTextBitmapCache() noexcept = default;
    NativeTextBitmapCache::~NativeTextBitmapCache() { Clear(); }
    void NativeTextBitmapCache::Clear() noexcept
    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        if (_statistics.branchEpoch == (std::numeric_limits<uint64_t>::max)()) _statistics.branchOverflow = true;
        else ++_statistics.branchEpoch;
#endif
        if (_state)
            if (auto store = _state->prewarm.lock()) store->Cancel();
        _state.reset();
        _statistics.bytes = 0;
        _statistics.entries = 0;
        _statistics.commandEntries = 0;
        _statistics.commandTextUnits = 0;
    }
    NativeTextBitmapCache::Statistics NativeTextBitmapCache::Stats() const noexcept { return _statistics; }
    void NativeTextBitmapCache::Begin(ID2D1DeviceContext *context) noexcept
    {
        try
        {
            if (_state) { _state->ready = false; _state->Sweep(_statistics); }
            if (!context) return;
            winrt::com_ptr<ID2D1Device> device;
            context->GetDevice(device.put());
            auto identity = device.as<IUnknown>();
            if (_state && _state->deviceIdentity.get() != identity.get()) Clear();
            if (!_state) _state = std::make_unique<State>();
            _state->device = std::move(device);
            _state->deviceIdentity = std::move(identity);
            _state->Capture(context);
        }
        catch (...) { if (_state) _state->ready = false; }
    }
    bool NativeTextBitmapCache::TryDraw(ID2D1DeviceContext *context, PreviewFrame const &frame,
                                       PreviewBlockVisual const &block, PreviewTextVisual const &text,
                                       uint32_t textIndex, D2D1_POINT_2F origin,
                                       ID2D1SolidColorBrush *foreground, bool allowBuild, bool promote,
                                       RequestRepaint const *repaint, bool *pending, ID2D1SolidColorBrush *scriptLink,
                                       bool allowPreparation) noexcept
    {
        if (pending) *pending = false;
        const auto script = text.scriptDrawing && text.scripts && text.scripts->RubyParts().empty() && _state &&
            SupportsScriptDpi(_state->sourceKey.dpi, _state->sourceKey.dpi);
        if (!_state || !_state->ready || _state->source != context || !foreground || !frame.main ||
            !text.layout || (!text.bitmapCandidate && !script) || !text.matchesMainText ||
            (!script && (text.scriptDrawing || text.scripts)) ||
            (script && !text.links.empty() && !scriptLink) ||
            !text.images.empty() || !text.math.empty() || !text.diagrams.empty() ||
            !text.utf16Length || text.utf16Length > 4096 ||
            !(text.height > 0) || text.height > 2048 || !std::isfinite(text.height) ||
            text.logicalStart > text.logicalEnd || text.logicalEnd > frame.main->Length() ||
            text.logicalEnd - text.logicalStart != text.utf16Length || foreground->GetOpacity() != 1 ||
            !std::isfinite(origin.x) || !std::isfinite(origin.y))
            return false;
        const auto retainCommands = !script && (!text.inlineCode.empty() || !text.links.empty());
        if (retainCommands && text.utf16Length > MaximumCommandEntryTextUnits) return false;
        auto &state = *_state;
        Key key = state.sourceKey;
        try
        {
            std::copy(std::begin(frame.key.model.document), std::end(frame.key.model.document), key.document.begin());
            key.revision = frame.key.model.revision;
            key.generation = frame.key.model.generation;
            key.profile = frame.key.model.profile;
            key.extensions = frame.key.model.extensions;
            key.theme = frame.key.theme_generation;
            key.width = frame.key.width;
            key.node = block.block.node;
            key.textIndex = textIndex;
            key.start = text.logicalStart;
            key.end = text.logicalEnd;
            key.maximumWidth = text.layout->GetMaxWidth();
            key.maximumHeight = text.layout->GetMaxHeight();
            const auto color = foreground->GetColor();
            if (!std::isfinite(color.r) || !std::isfinite(color.g) || !std::isfinite(color.b) || !std::isfinite(color.a)) return false;
            key.foreground = {color.r, color.g, color.b, color.a};
            if (!text.links.empty())
            {
                const auto link = frame.theme.linkForeground;
                key.linkForeground = {link.R, link.G, link.B, link.A};
            }
            if (!std::isfinite(key.width) || !(key.maximumWidth > 0) || !(key.maximumHeight > 0) ||
                !std::isfinite(key.maximumWidth) || !std::isfinite(key.maximumHeight)) return false;
            const auto scale = double{key.dpi} / 96.0;
            const auto pixelX = (double{origin.x} + state.transform._31) * scale;
            const auto pixelY = (double{origin.y} + state.transform._32) * scale;
            key.phaseX = pixelX - std::floor(pixelX);
            key.phaseY = pixelY - std::floor(pixelY);
            const auto originX = double{origin.x} * scale;
            const auto originY = double{origin.y} * scale;
            const auto transformX = double{state.transform._31} * scale;
            const auto transformY = double{state.transform._32} * scale;
            key.originPhaseX = originX - std::floor(originX);
            key.originPhaseY = originY - std::floor(originY);
            key.transformPhaseX = transformX - std::floor(transformX);
            key.transformPhaseY = transformY - std::floor(transformY);
            bool failedEpoch{};
            for (auto slot = state.Head(key); slot; slot = state.entries[slot - 1].next)
            {
                auto &entry = state.entries[slot - 1];
                if (entry.key.node != key.node || entry.key.textIndex != key.textIndex) continue;
                const auto failed = entry.failed && entry.key.SameFailureEpoch(key);
                const auto hit = (entry.bitmap || entry.commands) && entry.key == key;
                if ((!failed && !hit) || entry.owner.lock() != frame.main) continue;
                failedEpoch |= failed;
                if (hit)
                {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    DiagnosticBranchTimer replayTimer{*this, DiagnosticBranchTimer::Replay};
                    if (block.block.kind == 6 && !entry.commands)
                        replayTimer.ObserveBitmap(textIndex, text.logicalStart, text.logicalEnd, 1);
#endif
                    state.Replay(context, entry, origin, promote
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                        , &replayTimer
#endif
                    );
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    replayTimer.Stop();
#endif
                    ++_statistics.hits;
                    return true;
                }
            }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            DiagnosticPrewarmTarget *diagnosticReadyTarget{}; // Only this first observed TryDraw may record operations.
            // Observe the real false/false ordinary miss after resident/negative
            // lookup. Local failure must never reach the negative-entry catch.
            if (_diagnosticBranchScope && !allowBuild && !promote && block.block.kind == 1 &&
                block.texts.size() == 1 && textIndex == 0 && text.utf16Length <= 128 &&
                !text.scriptDrawing && !text.scripts && !retainCommands && text.links.empty() &&
                text.images.empty() && text.math.empty() && text.diagrams.empty())
            {
                try
                {
                    const auto content = frame.main->Text().substr(static_cast<size_t>(text.logicalStart), text.utf16Length);
                    if (std::count_if(content.begin(), content.end(),
                        [](wchar_t value) { return value >= 0xd800 && value <= 0xdbff; }) >= 8)
                    {
                        DiagnosticPrewarmTarget identity;
                        identity.document = key.document;
                        identity.layoutGeneration = frame.key.layout_generation;
                        identity.revision = key.revision;
                        identity.generation = key.generation;
                        identity.profile = key.profile;
                        identity.extensions = key.extensions;
                        identity.node = key.node;
                        identity.textIndex = key.textIndex;
                        identity.start = key.start;
                        identity.end = key.end;
                        identity.observations = 1;
                        if (_diagnosticBranchScope->RecordPrewarmTarget(identity))
                        {
                            auto &target = _diagnosticBranchScope->_prewarmTarget;
                            if (failedEpoch) target.flags |= DiagnosticPrewarmTarget::FailedEpoch;
                            try
                            {
                                if (frame.textPrewarm)
                                    frame.textPrewarm->DiagnosticSnapshot(target, key, state.sourceKey, frame.main,
                                        bool{state.adapter}, state.adapterLuid);
                                else target.flags |= DiagnosticPrewarmTarget::ObservationComplete;
                                if (_diagnosticBranchScope->Valid() &&
                                    (target.flags & (DiagnosticPrewarmTarget::ObservationComplete |
                                        DiagnosticPrewarmTarget::ObservationFailed | DiagnosticPrewarmTarget::ScopeInvalid)) ==
                                        DiagnosticPrewarmTarget::ObservationComplete)
                                    diagnosticReadyTarget = &target;
                            }
                            catch (...)
                            {
                                // Unknown observation, not a product cache failure.
                                target.flags &= ~DiagnosticPrewarmTarget::ObservationComplete;
                                target.flags |= DiagnosticPrewarmTarget::ObservationFailed;
                            }
                        }
                    }
                }
                catch (...) { } // Text observation also cannot change TryDraw semantics.
            }
#endif
            const auto readyOnlyOrdinary = !allowBuild && !promote && !script && !retainCommands &&
                block.block.kind == 1 && block.texts.size() == 1 && textIndex == 0;
            if (failedEpoch || (!allowBuild && !(script && allowPreparation) && !readyOnlyOrdinary))
                return false;
            // In the fractional-DPI matrix, whole physical-pixel em sizes keep
            // the original command pixels. Fractional em sizes have exhibited
            // different hinting. Certify every actual fallback/style run only
            // on a miss; unsupported runs get the bounded negative epoch and
            // keep their genuine direct draw. 18 DIP at 150% remains cached.
            if (retainCommands && FractionalScale(key.dpi) &&
                !HasIntegralPixelRuns(text.layout.get(), double{key.dpi} / 96.0))
                throw winrt::hresult_error(E_NOTIMPL);
            double left{}, top{};
            uint64_t bytes{};
            winrt::com_ptr<ID2D1CommandList> commands;
            std::shared_ptr<NativeTextPrewarm::PreparedText> prepared;
            const auto loadingOnly = (!allowBuild && script && allowPreparation) || readyOnlyOrdinary;
            const auto ordinaryBackground = retainCommands && text.links.empty() && state.canPrepareCommands &&
                SupportsScriptDpi(key.dpi, key.dpi);
            if ((!retainCommands || ordinaryBackground) && frame.textPrewarm && state.adapter)
            {
                const auto content = frame.main->Text().substr(static_cast<size_t>(text.logicalStart), text.utf16Length);
                const auto expensiveScript = script && text.utf16Length <= 512 &&
                    std::any_of(content.begin(), content.end(), [](wchar_t value) { return value >= 0xd800 && value <= 0xdbff; });
                if (expensiveScript && repaint && *repaint && pending &&
                    frame.textPrewarm->OfferScript(key, text, frame.main, !allowBuild))
                    PrepareFrame(frame, state.drawOrigin);
                // Outside normal admission, ordinary text only consumes ready
                // pixels; scripts retain their loading-stage pending route.
                if (allowBuild || loadingOnly)
                {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    DiagnosticBranchTimer takeTimer{*this, DiagnosticBranchTimer::Take};
#endif
                    if (readyOnlyOrdinary)
                    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                        if (diagnosticReadyTarget) diagnosticReadyTarget->flags |= DiagnosticPrewarmTarget::ReadyLookupAttempted;
#endif
                        prepared = frame.textPrewarm->TakeReadyOrdinary(key, frame.main, state.adapterLuid);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                        if (diagnosticReadyTarget && prepared)
                            diagnosticReadyTarget->flags |= DiagnosticPrewarmTarget::ReadyPayloadAcquired;
#endif
                    }
                    else
                        prepared = frame.textPrewarm->Take(key, frame.main, state.adapterLuid, repaint, pending,
                            loadingOnly || ordinaryBackground, retainCommands, state.deviceIdentity.get());
                }
            }
            if ((!allowBuild && !prepared) || (pending && *pending))
                return false;
            winrt::com_ptr<ID2D1Bitmap1> bitmap;
            if (prepared && prepared->commands)
            {
                commands = prepared->commands;
            }
            else if (prepared)
            {
                const auto properties = D2D1::BitmapProperties1(D2D1_BITMAP_OPTIONS_NONE,
                    D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), key.dpi, key.dpi);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                DiagnosticBranchTimer uploadTimer{*this, DiagnosticBranchTimer::Upload};
#endif
                winrt::check_hresult(context->CreateBitmap(prepared->size, prepared->data.data(),
                    prepared->size.width * 4, properties, bitmap.put()));
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                if (diagnosticReadyTarget) diagnosticReadyTarget->flags |= DiagnosticPrewarmTarget::BitmapUploaded;
                uploadTimer.Stop();
#endif
                left = prepared->left + std::floor(pixelX);
                top = prepared->top + std::floor(pixelY);
                bytes = prepared->data.size();
                ++_statistics.prewarmUploads;
            }
            else
            {
                ScriptPaint scriptPaint;
                scriptPaint.foreground = foreground;
                scriptPaint.link = scriptLink;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                DiagnosticBranchTimer buildTimer{*this, DiagnosticBranchTimer::Build};
#endif
                bitmap = state.Build(key, text.layout.get(), foreground, origin, left, top, bytes,
                                     retainCommands ? &commands : nullptr, MaximumEntryBytes,
                                     script ? text.scriptDrawing.get() : nullptr, &scriptPaint);
            }
            const auto commandTextUnits = retainCommands ? text.utf16Length : 0;
            auto &entry = state.Slot(bytes, _statistics, commandTextUnits);
            entry.owner = frame.main;
            state.Insert(entry, key);
            entry.bitmap = std::move(bitmap);
            entry.commands = std::move(commands);
            entry.left = left - std::floor(pixelX);
            entry.top = top - std::floor(pixelY);
            const auto canonicalCommands = prepared && prepared->commands;
            entry.recordedFloorX = canonicalCommands ? 0 : std::floor(pixelX);
            entry.recordedFloorY = canonicalCommands ? 0 : std::floor(pixelY);
            entry.bytes = bytes;
            entry.commandTextUnits = commandTextUnits;
            _statistics.bytes += bytes;
            if (entry.commands)
            {
                ++_statistics.commandEntries;
                _statistics.commandTextUnits += commandTextUnits;
            }
            ++_statistics.entries;
            ++_statistics.builds;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            DiagnosticBranchTimer replayTimer{*this, DiagnosticBranchTimer::Replay};
            if (block.block.kind == 6 && !entry.commands)
                replayTimer.ObserveBitmap(textIndex, text.logicalStart, text.logicalEnd, 2);
#endif
            state.Replay(context, entry, origin, !loadingOnly || promote
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                , &replayTimer
#endif
            );
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            // Replay is reached only after the uploaded entry is fully installed.
            if (diagnosticReadyTarget) diagnosticReadyTarget->flags |= DiagnosticPrewarmTarget::ReadyReplayed;
            replayTimer.Stop();
#endif
            return true;
        }
        catch (...)
        {
            // A bounded negative entry prevents repeated optional work in this
            // source/style/device epoch. The original renderer remains authoritative.
            auto &entry = state.Slot(0, _statistics);
            entry.owner = frame.main;
            state.Insert(entry, key);
            entry.failed = true;
            entry.stamp = ++state.clock;
            ++_statistics.failures;
            return false;
        }
    }
}
