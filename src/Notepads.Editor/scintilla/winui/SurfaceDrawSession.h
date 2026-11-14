// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#ifndef SURFACEDRAWSESSION_H
#define SURFACEDRAWSESSION_H

#include <d2d1.h>
#include <windows.ui.xaml.media.dxinterop.h>
#include <wrl/client.h>

namespace Scintilla::Internal {

// Own one successful XAML surface BeginDraw and its optional Direct2D draw.
// Local Surface objects must be destroyed before this session so their clips
// are removed before the outer clip. Strong references preserve the objects
// actually begun if a synchronous editor callback replaces graphics devices.
class SurfaceDrawSession final {
	Microsoft::WRL::ComPtr<ISurfaceImageSourceNativeWithD2D> surface;
	Microsoft::WRL::ComPtr<ID2D1RenderTarget> target;
	bool outerClip = false;
	bool finished = false;
	HRESULT result = S_OK;

public:
	explicit SurfaceDrawSession(ISurfaceImageSourceNativeWithD2D *surface_) noexcept : surface(surface_) {}
	SurfaceDrawSession(const SurfaceDrawSession &) = delete;
	SurfaceDrawSession &operator=(const SurfaceDrawSession &) = delete;
	~SurfaceDrawSession() { (void)Finish(); }

	void BeginD2D(ID2D1RenderTarget *target_) noexcept {
		target = target_;
		target->BeginDraw();
	}

	void PushClip(D2D1_RECT_F clip) noexcept {
		target->PushAxisAlignedClip(clip, D2D1_ANTIALIAS_MODE_ALIASED);
		outerClip = true;
	}

	[[nodiscard]] HRESULT Finish() noexcept {
		if (finished) return result;
		finished = true;
		if (target) {
			if (outerClip) target->PopAxisAlignedClip();
			result = target->EndDraw();
			target.Reset();
		}
		const HRESULT surfaceResult = surface->EndDraw();
		surface.Reset();
		if (SUCCEEDED(result)) result = surfaceResult;
		return result;
	}
};

}

#endif
