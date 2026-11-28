// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
#include "Diagnostics/NativeStartupTrace.h"
#endif
#include "Resources/Diagrams/NativeDiagramRenderer.h"
#include <cmath>
#include <utility>

namespace Notepads::Markdown::Native
{
    DiagramDrawCache::DiagramDrawCache(DiagramDrawCache &&other) noexcept
        : owner(std::move(other.owner)), context(std::move(other.context)), device(std::move(other.device)),
          stream(std::move(other.stream)),
          document(std::move(other.document)), deviceEpoch(other.deviceEpoch),
          status(other.status.load(std::memory_order_acquire))
    {
        other.deviceEpoch = 0;
        other.status.store(S_OK, std::memory_order_release);
    }
    DiagramDrawCache &DiagramDrawCache::operator=(DiagramDrawCache &&other) noexcept
    {
        if (this != &other)
        {
            Reset();
            owner = std::move(other.owner);
            context = std::move(other.context);
            device = std::move(other.device);
            stream = std::move(other.stream);
            document = std::move(other.document);
            deviceEpoch = other.deviceEpoch;
            status.store(other.status.load(std::memory_order_acquire), std::memory_order_release);
            other.deviceEpoch = 0;
            other.status.store(S_OK, std::memory_order_release);
        }
        return *this;
    }
    void DiagramDrawCache::Reset() noexcept
    {
        document = nullptr;
        stream = nullptr;
        context = nullptr;
        device = nullptr;
        owner.Reset();
        deviceEpoch = 0;
        status.store(S_OK, std::memory_order_release);
    }

    int32_t NativeDiagramDrawing::Place(NativeDiagramPresentation const &payload, float available,
                                        DiagramPlacement &placement) noexcept
    {
        placement = {};
        if (payload.State() != NativeDiagramState::Ready || !payload.Bytes() || !payload.ByteCount() ||
            payload.ByteCount() > 4 * 1024 * 1024)
            return NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
        auto width = static_cast<float>(payload.Width());
        auto height = static_cast<float>(payload.Height());
        if (!std::isfinite(width) || !std::isfinite(height) || !std::isfinite(available) || width <= 0 || height <= 0 ||
            available <= 0 || width > 1.0e7f || height > 1.0e7f)
            return NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT;
        auto scale = (std::min)(1.0f, available / width);
        placement = {width * scale, height * scale, height * scale, scale};
        if (!std::isfinite(placement.width) || !std::isfinite(placement.height) || placement.width <= 0 ||
            placement.height <= 0)
        {
            placement = {};
            return NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT;
        }
        return NOTEPADS_MARKDOWN_CORE_OK;
    }

    bool NativeDiagramDrawing::Inspect(DiagramPresentationHandle const &payload, D2D1_RECT_F const &viewport,
                                       DiagramInspectionTransform const &transform,
                                       DiagramInspectionPlacement &result) noexcept
    {
        result = {};
        if (!payload || payload->State() != NativeDiagramState::Ready)
            return false;
        auto width = viewport.right - viewport.left;
        auto height = viewport.bottom - viewport.top;
        auto const naturalWidth = static_cast<float>(payload->Width());
        auto const naturalHeight = static_cast<float>(payload->Height());
        if (!std::isfinite(width) || !std::isfinite(height) || width <= 0 || height <= 0 ||
            !std::isfinite(naturalWidth) || !std::isfinite(naturalHeight) ||
            naturalWidth <= 0 || naturalHeight <= 0 || naturalWidth > 1.0e7f || naturalHeight > 1.0e7f ||
            !std::isfinite(transform.zoom) || transform.zoom < DiagramInspectionTransform::MinimumZoom ||
            transform.zoom > DiagramInspectionTransform::MaximumZoom ||
            !std::isfinite(transform.x) || !std::isfinite(transform.y) ||
            transform.x < 0 || transform.x > 1 || transform.y < 0 || transform.y > 1)
            return false;

        auto const scale = (std::min)({1.0f, width / naturalWidth, height / naturalHeight}) * transform.zoom;
        auto const imageWidth = naturalWidth * scale;
        auto const imageHeight = naturalHeight * scale;
        result.clip = viewport;
        result.diagram = {imageWidth, imageHeight, imageHeight, scale};
        result.scrollWidth = (std::max)(0.0f, imageWidth - width);
        result.scrollHeight = (std::max)(0.0f, imageHeight - height);
        auto alignment = [](float available, float image, float fraction) noexcept
        {
            auto space = available - image;
            return space >= 0 ? space / 2.0f : space * fraction;
        };
        result.origin = {viewport.left + alignment(width, imageWidth, transform.x),
                         viewport.top + alignment(height, imageHeight, transform.y)};
        return std::isfinite(result.origin.x) && std::isfinite(result.origin.y) && scale > 0;
    }

    HRESULT NativeDiagramDrawing::Draw(DiagramPresentationHandle const &payload, DiagramPlacement const &placement,
                                       ID2D1DeviceContext *context, float left, float top,
                                       DiagramDrawCache &cache, D2D1_RECT_F const *inspectionClip) noexcept
    {
        if (!payload || !context || payload->State() != NativeDiagramState::Ready || !std::isfinite(left) ||
            !std::isfinite(top) || !std::isfinite(placement.scale) || placement.scale <= 0 ||
            placement.scale > (inspectionClip ? DiagramInspectionTransform::MaximumZoom : 1.0f) ||
            placement.width != static_cast<float>(payload->Width()) * placement.scale ||
            placement.height != static_cast<float>(payload->Height()) * placement.scale ||
            placement.baseline != placement.height)
            return E_INVALIDARG;
        if (inspectionClip && (!std::isfinite(inspectionClip->left) || !std::isfinite(inspectionClip->top) ||
                               !std::isfinite(inspectionClip->right) || !std::isfinite(inspectionClip->bottom) ||
                               inspectionClip->right <= inspectionClip->left || inspectionClip->bottom <= inspectionClip->top))
            return E_INVALIDARG;
        winrt::com_ptr<IUnknown> identity;
        auto status = context->QueryInterface(__uuidof(IUnknown), identity.put_void());
        if (FAILED(status))
            return status;
        winrt::com_ptr<ID2D1Device> device;
        context->GetDevice(device.put());
        if (!device)
            return E_UNEXPECTED;
        winrt::com_ptr<IUnknown> deviceIdentity;
        status = device->QueryInterface(__uuidof(IUnknown), deviceIdentity.put_void());
        if (FAILED(status))
            return status;
        winrt::com_ptr<ID2D1DeviceContext5> svgContext;
        const auto svgStatus = context->QueryInterface(__uuidof(ID2D1DeviceContext5), svgContext.put_void());
        // SVG resources can move between contexts created by the same D2D
        // device. A failed creation still retries on a different context.
        const bool successful = cache.document && cache.stream && SUCCEEDED(svgStatus) &&
            SUCCEEDED(cache.status.load(std::memory_order_acquire));
        if (cache.device.get() != deviceIdentity.get() || cache.owner.operator->() != payload.operator->() ||
            cache.deviceEpoch != payload->Key().deviceEpoch ||
            (cache.context.get() != identity.get() && !successful))
        {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            StartupTrace::UiScope createTrace{StartupTrace::Phase::UiDiagramCreate,
                StartupTrace::Phase::UiDiagramCreateEnd, 0};
            try
            {
                const uint64_t reason = (cache.context.get() != identity.get() ? 1u : 0u) |
                    (cache.owner.operator->() != payload.operator->() ? 2u : 0u) |
                    (cache.deviceEpoch != payload->Key().deviceEpoch ? 4u : 0u) |
                    (cache.device.get() != deviceIdentity.get() ? 8u : 0u);
                StartupTrace::MarkFor(createTrace.trace, StartupTrace::Phase::DiagramCacheMiss, reason);
            }
            catch (...) { }
#endif
            cache.Reset();
            cache.owner = payload;
            cache.context = identity;
            cache.device = deviceIdentity;
            cache.deviceEpoch = payload->Key().deviceEpoch;
            status = svgStatus;
            if (SUCCEEDED(status))
            {
                winrt::com_ptr<ID2D1Factory> factory;
                context->GetFactory(factory.put());
                winrt::com_ptr<ID2D1Multithread> threading;
                status = factory->QueryInterface(__uuidof(ID2D1Multithread), threading.put_void());
                if (SUCCEEDED(status) && !threading->GetMultithreadProtected())
                    status = E_INVALIDARG;
            }
            if (SUCCEEDED(status))
                status = NativeDiagramArtifact::OpenReadStream(payload, cache.stream.put());
            if (SUCCEEDED(status))
                status = svgContext->CreateSvgDocument(
                    cache.stream.get(),
                    D2D1::SizeF(static_cast<float>(payload->Width()), static_cast<float>(payload->Height())),
                    cache.document.put());
            cache.status.store(status, std::memory_order_release);
        }
        status = cache.status.load(std::memory_order_acquire);
        if (FAILED(status))
            return status;
        if (!cache.document)
            return E_UNEXPECTED;
        if (FAILED(svgStatus))
            return svgStatus;
        // Do not retain a superseded SIS context after a successful handoff.
        cache.context = identity;
        D2D1_MATRIX_3X2_F original;
        context->GetTransform(&original);
        if (inspectionClip)
            context->PushAxisAlignedClip(*inspectionClip, D2D1_ANTIALIAS_MODE_ALIASED);
        context->SetTransform(D2D1::Matrix3x2F::Scale(placement.scale, placement.scale) *
                              D2D1::Matrix3x2F::Translation(left, top) * original);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        {
            StartupTrace::UiScope paintTrace{StartupTrace::Phase::UiDiagramPaint,
                StartupTrace::Phase::UiDiagramPaintEnd, 0};
            svgContext->DrawSvgDocument(cache.document.get());
        }
#else
        svgContext->DrawSvgDocument(cache.document.get());
#endif
        context->SetTransform(original);
        if (inspectionClip)
            context->PopAxisAlignedClip();
        return S_OK;
    }
}
