// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Resources/Diagrams/NativeDiagramArtifact.h"
#include <d2d1_3.h>
#include <d2d1svg.h>

namespace Notepads::Markdown::Native
{
    struct DiagramPlacement
    {
        float width{}, height{}, baseline{}, scale{};
    };

    struct DiagramInspectionTransform
    {
        static constexpr float MinimumZoom = 1.0f;
        static constexpr float MaximumZoom = 8.0f;
        float zoom{MinimumZoom};
        float x{0.5f}, y{0.5f};

        bool operator==(DiagramInspectionTransform const &) const noexcept = default;
    };

    struct DiagramInspectionPlacement
    {
        DiagramPlacement diagram;
        D2D1_RECT_F clip{};
        D2D1_POINT_2F origin{};
        float scrollWidth{}, scrollHeight{};
    };

    // Only UI Draw mutates this cache. Frames over the same measured blocks
    // share these COM references. The stream/Node stay alive through SVG use.
    struct DiagramDrawCache
    {
        DiagramPresentationHandle owner;
        winrt::com_ptr<IUnknown> context; // Last draw/creation context; failures remain context-scoped.
        winrt::com_ptr<IUnknown> device; // Canonical ID2D1Device resource domain for successful SVGs.
        winrt::com_ptr<IStream> stream;
        winrt::com_ptr<ID2D1SvgDocument> document;
        uint64_t deviceEpoch{};
        // Successful SVGs share only within this D2D device/owner/epoch.
        // Failed creation is retried when the actual context changes.
        std::atomic<int32_t> status{S_OK};
        DiagramDrawCache() noexcept = default;
        // A copied CPU visual starts a fresh UI/device cache.
        DiagramDrawCache(DiagramDrawCache const &) noexcept
        {
        }
        DiagramDrawCache &operator=(DiagramDrawCache const &) noexcept
        {
            Reset();
            return *this;
        }
        DiagramDrawCache(DiagramDrawCache &&other) noexcept;
        DiagramDrawCache &operator=(DiagramDrawCache &&other) noexcept;
        void Reset() noexcept;
    };

    class NativeDiagramDrawing final
    {
      public:
        static int32_t Place(NativeDiagramPresentation const &payload, float availableWidth,
                             DiagramPlacement &placement) noexcept;
        static bool Inspect(DiagramPresentationHandle const &payload, D2D1_RECT_F const &viewport,
                            DiagramInspectionTransform const &transform, DiagramInspectionPlacement &result) noexcept;
        static HRESULT Draw(DiagramPresentationHandle const &payload, DiagramPlacement const &placement,
                            ID2D1DeviceContext *context, float left, float top, DiagramDrawCache &cache,
                            D2D1_RECT_F const *inspectionClip = nullptr) noexcept;
    };
}
