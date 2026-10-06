// Scintilla source code edit control
/** @file SurfaceD2D.h
 ** Definitions for drawing to Direct2D on Windows.
 **/
// Copyright 1998-2011 by Neil Hodgson <neilh@scintilla.org>
// The License.txt file describes the conditions under which this software may be distributed.

#pragma once

namespace Scintilla::Internal {

extern bool LoadD2D() noexcept;
extern void ReleaseD2D() noexcept;
extern ID2D1Factory1 *pD2DFactory;
extern IDWriteFactory1 *pIDWriteFactory;

using DCRenderTarget = winrt::com_ptr<ID2D1DCRenderTarget>;

using D3D11Device = winrt::com_ptr<ID3D11Device1>;

using WriteRenderingParams = winrt::com_ptr<IDWriteRenderingParams1>;

struct RenderingParams {
	WriteRenderingParams defaultRenderingParams;
	WriteRenderingParams customRenderingParams;
};

struct ISetRenderingParams {
	virtual void SetRenderingParams(std::shared_ptr<RenderingParams> renderingParams_) = 0;
};

using BrushSolid = winrt::com_ptr<ID2D1SolidColorBrush>;
using Geometry = winrt::com_ptr<ID2D1PathGeometry>;
using GeometrySink = winrt::com_ptr<ID2D1GeometrySink>;
using StrokeStyle = winrt::com_ptr<ID2D1StrokeStyle>;
using TextLayout = winrt::com_ptr<IDWriteTextLayout>;

BrushSolid BrushSolidCreate(ID2D1RenderTarget *pTarget, COLORREF colour) noexcept;
Geometry GeometryCreate() noexcept;
GeometrySink GeometrySinkCreate(ID2D1PathGeometry *geometry) noexcept;
StrokeStyle StrokeStyleCreate(const D2D1_STROKE_STYLE_PROPERTIES &strokeStyleProperties) noexcept;
TextLayout LayoutCreate(std::wstring_view wsv, IDWriteTextFormat *pTextFormat, FLOAT maxWidth=10000.0F, FLOAT maxHeight=1000.0F) noexcept;

}
