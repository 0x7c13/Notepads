// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

// A test-only capability facade. It forwards drawing to the actual WIC target,
// exposes its real Context4 and deliberately withholds Context7. This exercises
// the supported legacy branch without a production switch or pretending to run
// an older OS. The synchronous fixture owns this facade's initial reference.
class LegacyTarget final : public ID2D1RenderTarget {
    ComPtr<ID2D1RenderTarget> target;
    ULONG references = 1;
    bool exposeContext4;
public:
    unsigned queries7 = 0, queries4 = 0, wholeLayoutDraws = 0;
    explicit LegacyTarget(ID2D1RenderTarget *target_, bool expose4 = true) : target(target_), exposeContext4(expose4) {}
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id, void **value) override {
        if (!value) return E_POINTER;
        *value = nullptr;
        if (id == __uuidof(ID2D1DeviceContext7)) { ++queries7; return E_NOINTERFACE; }
        if (id == __uuidof(ID2D1DeviceContext4)) {
            ++queries4;
            return exposeContext4 ? target->QueryInterface(id, value) : E_NOINTERFACE;
        }
        if (id != __uuidof(IUnknown) && id != __uuidof(ID2D1Resource) && id != __uuidof(ID2D1RenderTarget)) return E_NOINTERFACE;
        *value = static_cast<ID2D1RenderTarget *>(this);
        AddRef();
        return S_OK;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return ++references; }
    ULONG STDMETHODCALLTYPE Release() override { return --references; }
    void STDMETHODCALLTYPE GetFactory(ID2D1Factory **value) const override { target->GetFactory(value); }
    HRESULT STDMETHODCALLTYPE CreateBitmap(D2D1_SIZE_U size, CONST void *srcData, UINT32 pitch, CONST D2D1_BITMAP_PROPERTIES *bitmapProperties, ID2D1Bitmap **bitmap) override { return target->CreateBitmap(size, srcData, pitch, bitmapProperties, bitmap); }
    HRESULT STDMETHODCALLTYPE CreateBitmapFromWicBitmap(IWICBitmapSource *wicBitmapSource, CONST D2D1_BITMAP_PROPERTIES *bitmapProperties, ID2D1Bitmap **bitmap) override { return target->CreateBitmapFromWicBitmap(wicBitmapSource, bitmapProperties, bitmap); }
    HRESULT STDMETHODCALLTYPE CreateSharedBitmap(REFIID riid, void *data, CONST D2D1_BITMAP_PROPERTIES *bitmapProperties, ID2D1Bitmap **bitmap) override { return target->CreateSharedBitmap(riid, data, bitmapProperties, bitmap); }
    HRESULT STDMETHODCALLTYPE CreateBitmapBrush(ID2D1Bitmap *bitmap, CONST D2D1_BITMAP_BRUSH_PROPERTIES *bitmapBrushProperties, CONST D2D1_BRUSH_PROPERTIES *brushProperties, ID2D1BitmapBrush **bitmapBrush) override { return target->CreateBitmapBrush(bitmap, bitmapBrushProperties, brushProperties, bitmapBrush); }
    HRESULT STDMETHODCALLTYPE CreateSolidColorBrush(CONST D2D1_COLOR_F *color, CONST D2D1_BRUSH_PROPERTIES *brushProperties, ID2D1SolidColorBrush **solidColorBrush) override { return target->CreateSolidColorBrush(color, brushProperties, solidColorBrush); }
    HRESULT STDMETHODCALLTYPE CreateGradientStopCollection(CONST D2D1_GRADIENT_STOP *gradientStops, UINT32 gradientStopsCount, D2D1_GAMMA colorInterpolationGamma, D2D1_EXTEND_MODE extendMode, ID2D1GradientStopCollection **gradientStopCollection) override { return target->CreateGradientStopCollection(gradientStops, gradientStopsCount, colorInterpolationGamma, extendMode, gradientStopCollection); }
    HRESULT STDMETHODCALLTYPE CreateLinearGradientBrush(CONST D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES *linearGradientBrushProperties, CONST D2D1_BRUSH_PROPERTIES *brushProperties, ID2D1GradientStopCollection *gradientStopCollection, ID2D1LinearGradientBrush **linearGradientBrush) override { return target->CreateLinearGradientBrush(linearGradientBrushProperties, brushProperties, gradientStopCollection, linearGradientBrush); }
    HRESULT STDMETHODCALLTYPE CreateRadialGradientBrush(CONST D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES *radialGradientBrushProperties, CONST D2D1_BRUSH_PROPERTIES *brushProperties, ID2D1GradientStopCollection *gradientStopCollection, ID2D1RadialGradientBrush **radialGradientBrush) override { return target->CreateRadialGradientBrush(radialGradientBrushProperties, brushProperties, gradientStopCollection, radialGradientBrush); }
    HRESULT STDMETHODCALLTYPE CreateCompatibleRenderTarget(CONST D2D1_SIZE_F *desiredSize, CONST D2D1_SIZE_U *desiredPixelSize, CONST D2D1_PIXEL_FORMAT *desiredFormat, D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS options, ID2D1BitmapRenderTarget **bitmapRenderTarget) override { return target->CreateCompatibleRenderTarget(desiredSize, desiredPixelSize, desiredFormat, options, bitmapRenderTarget); }
    HRESULT STDMETHODCALLTYPE CreateLayer(CONST D2D1_SIZE_F *size, ID2D1Layer **layer) override { return target->CreateLayer(size, layer); }
    HRESULT STDMETHODCALLTYPE CreateMesh(ID2D1Mesh **mesh) override { return target->CreateMesh(mesh); }
    void STDMETHODCALLTYPE DrawLine(D2D1_POINT_2F point0, D2D1_POINT_2F point1, ID2D1Brush *brush, FLOAT strokeWidth, ID2D1StrokeStyle *strokeStyle) override { return target->DrawLine(point0, point1, brush, strokeWidth, strokeStyle); }
    void STDMETHODCALLTYPE DrawRectangle(CONST D2D1_RECT_F *rect, ID2D1Brush *brush, FLOAT strokeWidth, ID2D1StrokeStyle *strokeStyle) override { return target->DrawRectangle(rect, brush, strokeWidth, strokeStyle); }
    void STDMETHODCALLTYPE FillRectangle(CONST D2D1_RECT_F *rect, ID2D1Brush *brush) override { return target->FillRectangle(rect, brush); }
    void STDMETHODCALLTYPE DrawRoundedRectangle(CONST D2D1_ROUNDED_RECT *roundedRect, ID2D1Brush *brush, FLOAT strokeWidth, ID2D1StrokeStyle *strokeStyle) override { return target->DrawRoundedRectangle(roundedRect, brush, strokeWidth, strokeStyle); }
    void STDMETHODCALLTYPE FillRoundedRectangle(CONST D2D1_ROUNDED_RECT *roundedRect, ID2D1Brush *brush) override { return target->FillRoundedRectangle(roundedRect, brush); }
    void STDMETHODCALLTYPE DrawEllipse(CONST D2D1_ELLIPSE *ellipse, ID2D1Brush *brush, FLOAT strokeWidth, ID2D1StrokeStyle *strokeStyle) override { return target->DrawEllipse(ellipse, brush, strokeWidth, strokeStyle); }
    void STDMETHODCALLTYPE FillEllipse(CONST D2D1_ELLIPSE *ellipse, ID2D1Brush *brush) override { return target->FillEllipse(ellipse, brush); }
    void STDMETHODCALLTYPE DrawGeometry(ID2D1Geometry *geometry, ID2D1Brush *brush, FLOAT strokeWidth, ID2D1StrokeStyle *strokeStyle) override { return target->DrawGeometry(geometry, brush, strokeWidth, strokeStyle); }
    void STDMETHODCALLTYPE FillGeometry(ID2D1Geometry *geometry, ID2D1Brush *brush, ID2D1Brush *opacityBrush) override { return target->FillGeometry(geometry, brush, opacityBrush); }
    void STDMETHODCALLTYPE FillMesh(ID2D1Mesh *mesh, ID2D1Brush *brush) override { return target->FillMesh(mesh, brush); }
    void STDMETHODCALLTYPE FillOpacityMask(ID2D1Bitmap *opacityMask, ID2D1Brush *brush, D2D1_OPACITY_MASK_CONTENT content, CONST D2D1_RECT_F *destinationRectangle, CONST D2D1_RECT_F *sourceRectangle) override { return target->FillOpacityMask(opacityMask, brush, content, destinationRectangle, sourceRectangle); }
    void STDMETHODCALLTYPE DrawBitmap(ID2D1Bitmap *bitmap, CONST D2D1_RECT_F *destinationRectangle, FLOAT opacity, D2D1_BITMAP_INTERPOLATION_MODE interpolationMode, CONST D2D1_RECT_F *sourceRectangle) override { return target->DrawBitmap(bitmap, destinationRectangle, opacity, interpolationMode, sourceRectangle); }
    void STDMETHODCALLTYPE DrawText(CONST WCHAR *string, UINT32 stringLength, IDWriteTextFormat *textFormat, CONST D2D1_RECT_F *layoutRect, ID2D1Brush *defaultFillBrush, D2D1_DRAW_TEXT_OPTIONS options, DWRITE_MEASURING_MODE measuringMode) override { return target->DrawText(string, stringLength, textFormat, layoutRect, defaultFillBrush, options, measuringMode); }
    void STDMETHODCALLTYPE DrawTextLayout(D2D1_POINT_2F origin, IDWriteTextLayout *textLayout, ID2D1Brush *defaultFillBrush, D2D1_DRAW_TEXT_OPTIONS options) override { ++wholeLayoutDraws; return target->DrawTextLayout(origin, textLayout, defaultFillBrush, options); }
    void STDMETHODCALLTYPE DrawGlyphRun(D2D1_POINT_2F baselineOrigin, CONST DWRITE_GLYPH_RUN *glyphRun, ID2D1Brush *foregroundBrush, DWRITE_MEASURING_MODE measuringMode) override { return target->DrawGlyphRun(baselineOrigin, glyphRun, foregroundBrush, measuringMode); }
    void STDMETHODCALLTYPE SetTransform(CONST D2D1_MATRIX_3X2_F *transform) override { return target->SetTransform(transform); }
    void STDMETHODCALLTYPE GetTransform(D2D1_MATRIX_3X2_F *transform) const override { return target->GetTransform(transform); }
    void STDMETHODCALLTYPE SetAntialiasMode(D2D1_ANTIALIAS_MODE antialiasMode) override { return target->SetAntialiasMode(antialiasMode); }
    D2D1_ANTIALIAS_MODE STDMETHODCALLTYPE GetAntialiasMode() const override { return target->GetAntialiasMode(); }
    void STDMETHODCALLTYPE SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE textAntialiasMode) override { return target->SetTextAntialiasMode(textAntialiasMode); }
    D2D1_TEXT_ANTIALIAS_MODE STDMETHODCALLTYPE GetTextAntialiasMode() const override { return target->GetTextAntialiasMode(); }
    void STDMETHODCALLTYPE SetTextRenderingParams(IDWriteRenderingParams *textRenderingParams) override { return target->SetTextRenderingParams(textRenderingParams); }
    void STDMETHODCALLTYPE GetTextRenderingParams(IDWriteRenderingParams **textRenderingParams) const override { return target->GetTextRenderingParams(textRenderingParams); }
    void STDMETHODCALLTYPE SetTags(D2D1_TAG tag1, D2D1_TAG tag2) override { return target->SetTags(tag1, tag2); }
    void STDMETHODCALLTYPE GetTags(D2D1_TAG *tag1, D2D1_TAG *tag2) const override { return target->GetTags(tag1, tag2); }
    void STDMETHODCALLTYPE PushLayer(CONST D2D1_LAYER_PARAMETERS *layerParameters, ID2D1Layer *layer) override { return target->PushLayer(layerParameters, layer); }
    void STDMETHODCALLTYPE PopLayer() override { return target->PopLayer(); }
    HRESULT STDMETHODCALLTYPE Flush(D2D1_TAG *tag1, D2D1_TAG *tag2) override { return target->Flush(tag1, tag2); }
    void STDMETHODCALLTYPE SaveDrawingState(ID2D1DrawingStateBlock *drawingStateBlock) const override { return target->SaveDrawingState(drawingStateBlock); }
    void STDMETHODCALLTYPE RestoreDrawingState(ID2D1DrawingStateBlock *drawingStateBlock) override { return target->RestoreDrawingState(drawingStateBlock); }
    void STDMETHODCALLTYPE PushAxisAlignedClip(CONST D2D1_RECT_F *clipRect, D2D1_ANTIALIAS_MODE antialiasMode) override { return target->PushAxisAlignedClip(clipRect, antialiasMode); }
    void STDMETHODCALLTYPE PopAxisAlignedClip() override { return target->PopAxisAlignedClip(); }
    void STDMETHODCALLTYPE Clear(CONST D2D1_COLOR_F *clearColor) override { return target->Clear(clearColor); }
    void STDMETHODCALLTYPE BeginDraw() override { return target->BeginDraw(); }
    HRESULT STDMETHODCALLTYPE EndDraw(D2D1_TAG *tag1, D2D1_TAG *tag2) override { return target->EndDraw(tag1, tag2); }
    D2D1_PIXEL_FORMAT STDMETHODCALLTYPE GetPixelFormat() const override { return target->GetPixelFormat(); }
    void STDMETHODCALLTYPE SetDpi(FLOAT dpiX, FLOAT dpiY) override { return target->SetDpi(dpiX, dpiY); }
    void STDMETHODCALLTYPE GetDpi(FLOAT *dpiX, FLOAT *dpiY) const override { return target->GetDpi(dpiX, dpiY); }
    D2D1_SIZE_F STDMETHODCALLTYPE GetSize() const override { return target->GetSize(); }
    D2D1_SIZE_U STDMETHODCALLTYPE GetPixelSize() const override { return target->GetPixelSize(); }
    UINT32 STDMETHODCALLTYPE GetMaximumBitmapSize() const override { return target->GetMaximumBitmapSize(); }
    BOOL STDMETHODCALLTYPE IsSupported(CONST D2D1_RENDER_TARGET_PROPERTIES *renderTargetProperties) const override { return target->IsSupported(renderTargetProperties); }
};
