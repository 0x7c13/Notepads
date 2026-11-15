// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <d3d11.h>
#include <d2d1_3.h>
#include <dwrite_3.h>
#include <wrl/client.h>
#include <chrono>
#include <iostream>
#include <vector>
#include <string>
#include <thread>
#include <stdexcept>
#include "DirectWriteAsyncTextCache.h"
#include "DirectWriteTextLayoutCache.h"
namespace {
size_t sourceChecks = 0;
using Microsoft::WRL::ComPtr;
using namespace Scintilla::Internal;
void HR(HRESULT hr) { if (FAILED(hr)) throw std::runtime_error(std::to_string(hr)); ++sourceChecks; }
void Require(bool value, const char* why) { if(!value) throw std::runtime_error(why); ++sourceChecks; }
struct Image {
 ComPtr<ID2D1DeviceContext> context;
 ComPtr<ID2D1Bitmap1> bitmap;
 ComPtr<ID2D1Bitmap1> cpu;
 ComPtr<ID2D1SolidColorBrush> brush;
 Image(ID2D1Device* device) {
  HR(device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, &context));
  const auto props=D2D1::BitmapProperties1(D2D1_BITMAP_OPTIONS_TARGET,D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM,D2D1_ALPHA_MODE_PREMULTIPLIED),96,96);
  HR(context->CreateBitmap(D2D1::SizeU(800,160),nullptr,0,props,&bitmap)); context->SetTarget(bitmap.Get());
  auto cpuProps=props; cpuProps.bitmapOptions=D2D1_BITMAP_OPTIONS_CPU_READ|D2D1_BITMAP_OPTIONS_CANNOT_DRAW;
  HR(context->CreateBitmap(D2D1::SizeU(800,160),nullptr,0,cpuProps,&cpu));
  HR(context->CreateSolidColorBrush(D2D1::ColorF(0,1),&brush));
 }
 std::vector<BYTE> Pixels() {
  HR(cpu->CopyFromBitmap(nullptr,bitmap.Get(),nullptr)); D2D1_MAPPED_RECT mapped{}; HR(cpu->Map(D2D1_MAP_OPTIONS_READ,&mapped));
  std::vector<BYTE> pixels(800*160*4);
  for(size_t row=0;row<160;++row) memcpy(pixels.data()+row*800*4,mapped.bits+row*mapped.pitch,800*4);
  HR(cpu->Unmap()); return pixels;
 }
 void Begin(bool opaque) {
  context->BeginDraw(); context->Clear(opaque?D2D1::ColorF(0x83A4C2,1):D2D1::ColorF(0,0));
  context->PushAxisAlignedClip(D2D1::RectF(0,0,530.5f,110.5f),D2D1_ANTIALIAS_MODE_ALIASED);
 }
 void End() { context->PopAxisAlignedClip(); HR(context->EndDraw()); }
};

DirectWriteAsyncTextCache::Statistics WaitForRaster(const DirectWriteAsyncTextCache &cache) {
 const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(10);
 auto status = cache.Snapshot();
 while (status.pending && std::chrono::steady_clock::now() < deadline) {
  std::this_thread::sleep_for(std::chrono::milliseconds(1));
  status = cache.Snapshot();
 }
 return status;
}

void CheckLookaheadRetention(ID2D1Device *device, IDWriteFactory *write, IDWriteTextFormat *format, const std::wstring &text) {
 Image image(device);image.context->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
 auto target=CreateDrawingTextLayout(write,format,text,2000,80);
 auto next=CreateDrawingTextLayout(write,format,text+L" A",2000,80);
 auto later=CreateDrawingTextLayout(write,format,text+L" B",2000,80);
 DirectWriteAsyncTextCache cache;const D2D1_POINT_2F origin{5,4};
 cache.Request(image.context.Get(),target.Get(),image.brush.Get(),origin,text.size(),text);
 const auto completed = WaitForRaster(cache);
 Require(completed.pending == 0 && completed.cpuBytes > 0, "target raster completes before concurrent lookahead");
 image.Begin(false);
 const bool ready = cache.Draw(image.context.Get(),target.Get(),image.brush.Get(),origin,text);
 image.End();
 Require(ready,"target pixels exist before concurrent lookahead");
 cache.Request(image.context.Get(),next.Get(),image.brush.Get(),origin,text.size()+2,text+L" A");
 cache.Request(image.context.Get(),later.Get(),image.brush.Get(),origin,text.size()+2,text+L" B");
 const auto pending=cache.Snapshot();
 Require(pending.cpuBytes<=4*1024*1024&&pending.pendingReservedBytes<=4*1024*1024&&pending.reservedBytes<=8*1024*1024,
  "resident pixels and concurrent lookahead have separate bounded reservations");
 image.Begin(false);Require(cache.Draw(image.context.Get(),target.Get(),image.brush.Get(),origin,text),
  "two lookahead jobs preserve completed target pixels");image.End();
 auto rebuilt=CreateDrawingTextLayout(write,format,text,2000,80);
 Require(rebuilt.Get()!=target.Get(),"independent producer returns a rebuilt layout object");
 image.Begin(false);Require(cache.Draw(image.context.Get(),rebuilt.Get(),image.brush.Get(),origin,text),
  "identical immutable producer inputs reuse actual pixels after layout eviction");image.End();
 auto resized=CreateDrawingTextLayout(write,format,text,2001,80);
 Require(!cache.Draw(image.context.Get(),resized.Get(),image.brush.Get(),origin,text),"changed layout width rejects cross-object pixel reuse");
 const auto editedText=text+L" C";auto edited=CreateDrawingTextLayout(write,format,editedText,2000,80);
 Require(!cache.Draw(image.context.Get(),edited.Get(),image.brush.Get(),origin,editedText),"changed source text rejects cross-object pixel reuse");
}

void CheckAdapterAdmission(ID2D1Factory1 *factory, IDXGIDevice *dxgi, ID2D1Device *device, IDWriteFactory *write, IDWriteTextFormat *format) {
 Image source(device);source.context->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
 Require(DirectWriteAsyncTextCache::SupportsTarget(source.context.Get()),"hardware grayscale context supports prefetch");
 source.context->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE);
 Require(!DirectWriteAsyncTextCache::SupportsTarget(source.context.Get()),"ClearType is rejected before source layout");
 source.context->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
 source.context->SetTransform(D2D1::Matrix3x2F::Translation(.5f,0));
 Require(!DirectWriteAsyncTextCache::SupportsTarget(source.context.Get()),"fractional translation is rejected before source layout");
 source.context->SetTransform(D2D1::Matrix3x2F::Identity());
 auto layout=CreateDrawingTextLayout(write,format,L"😀 🧪",200,80);
 DirectWriteAsyncTextCache cache;const D2D1_POINT_2F origin{5,4};
 const auto slotStart=std::chrono::steady_clock::now();
 while(!DirectWriteAsyncTextCache::CanRequest()&&std::chrono::steady_clock::now()-slotStart<std::chrono::seconds(10))
  std::this_thread::sleep_for(std::chrono::milliseconds(1));
 Require(DirectWriteAsyncTextCache::CanRequest(),"prior independent lookahead releases a worker slot");
 cache.Request(source.context.Get(),layout.Get(),source.brush.Get(),origin,5);
 const auto completed = WaitForRaster(cache);
 Require(completed.pending == 0 && completed.cpuBytes > 0, "same-adapter raster completes");
 source.Begin(false);
 const bool ready = cache.Draw(source.context.Get(),layout.Get(),source.brush.Get(),origin);
 source.End();
 Require(ready,"same-adapter pixels are generated and uploaded");
 const auto expected=source.Pixels();
 ComPtr<ID2D1Device> recreated;HR(factory->CreateDevice(dxgi,&recreated));Image replacement(recreated.Get());
 replacement.context->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
 replacement.Begin(false);Require(cache.Draw(replacement.context.Get(),layout.Get(),replacement.brush.Get(),origin),
  "same-adapter device recreation reuploads CPU pixels");replacement.End();
 Require(expected==replacement.Pixels(),"recreated same-adapter device preserves every source pixel");
 ComPtr<ID3D11Device> warp;HR(D3D11CreateDevice(nullptr,D3D_DRIVER_TYPE_WARP,nullptr,D3D11_CREATE_DEVICE_BGRA_SUPPORT,nullptr,0,D3D11_SDK_VERSION,&warp,nullptr,nullptr));
 ComPtr<IDXGIDevice> warpDxgi;HR(warp.As(&warpDxgi));ComPtr<ID2D1Device> warpDevice;HR(factory->CreateDevice(warpDxgi.Get(),&warpDevice));
 Image software(warpDevice.Get());software.context->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
 Require(!DirectWriteAsyncTextCache::SupportsTarget(software.context.Get()),"software adapter rejects prefetch before layout");
 Require(!cache.Draw(software.context.Get(),layout.Get(),software.brush.Get(),origin),"hardware CPU pixels never replay onto WARP");
 Require(cache.Snapshot().uploadedBytes==0,"unsupported replacement device releases old hardware uploads");
 DirectWriteAsyncTextCache rejected;rejected.Request(software.context.Get(),layout.Get(),software.brush.Get(),origin,5);
 Require(rejected.Snapshot().entries==0,"WARP source admits no unused pixel task");
}

struct WorkerReleaseState {
 std::atomic<bool> entered{}, allowReturn{}, timedOut{}, released{};
 HRESULT apartmentResult=E_UNEXPECTED;
 APTTYPE apartment=APTTYPE_CURRENT;
 DWORD releaseThread{};
 uint32_t moduleLocks{};
};

// A resource retained only by the submitted layout makes the last worker's
// COM Release observable even after the UI font/cache has already closed.
class WorkerReleaseObject final : public IDWriteInlineObject {
 std::atomic<ULONG> references{1};
 std::shared_ptr<WorkerReleaseState> state;
 ~WorkerReleaseObject() {
  APTTYPEQUALIFIER qualifier{};
  state->apartmentResult=CoGetApartmentType(&state->apartment,&qualifier);
  state->releaseThread=GetCurrentThreadId();
  state->moduleLocks=static_cast<uint32_t>(winrt::get_module_lock());
  state->released.store(true,std::memory_order_release);
 }
public:
 explicit WorkerReleaseObject(std::shared_ptr<WorkerReleaseState> value):state(std::move(value)) {}
 HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id,void **object) noexcept override {
  if(!object)return E_POINTER;
  *object=nullptr;
  if(id!=__uuidof(IUnknown)&&id!=__uuidof(IDWriteInlineObject))return E_NOINTERFACE;
  *object=static_cast<IDWriteInlineObject*>(this);AddRef();return S_OK;
 }
 ULONG STDMETHODCALLTYPE AddRef() noexcept override {return ++references;}
 ULONG STDMETHODCALLTYPE Release() noexcept override {
  const auto count=--references;if(!count)delete this;return count;
 }
 HRESULT STDMETHODCALLTYPE Draw(void*,IDWriteTextRenderer*,FLOAT,FLOAT,BOOL,BOOL,IUnknown*) noexcept override {
  state->entered.store(true,std::memory_order_release);
  const auto start=std::chrono::steady_clock::now();
  while(!state->allowReturn.load(std::memory_order_acquire)) {
   if(std::chrono::steady_clock::now()-start>=std::chrono::seconds(10)) {
    state->timedOut.store(true);return E_ABORT;
   }
   std::this_thread::sleep_for(std::chrono::milliseconds(1));
  }
  return S_OK;
 }
 HRESULT STDMETHODCALLTYPE GetMetrics(DWRITE_INLINE_OBJECT_METRICS *metrics) noexcept override {
  if(!metrics)return E_POINTER;*metrics={1,1,1,FALSE};return S_OK;
 }
 HRESULT STDMETHODCALLTYPE GetOverhangMetrics(DWRITE_OVERHANG_METRICS *metrics) noexcept override {
  if(!metrics)return E_POINTER;*metrics={};return S_OK;
 }
 HRESULT STDMETHODCALLTYPE GetBreakConditions(DWRITE_BREAK_CONDITION *before,DWRITE_BREAK_CONDITION *after) noexcept override {
  if(!before||!after)return E_POINTER;*before=*after=DWRITE_BREAK_CONDITION_NEUTRAL;return S_OK;
 }
};

void CheckClosedWorkerReleaseOrder(ID2D1Device *device,IDWriteFactory *write,IDWriteTextFormat *format,uint32_t baselineLocks) {
 const auto state=std::make_shared<WorkerReleaseState>();
 // Unblock a pending callback on every exit path, including a failed assertion.
 struct ReturnGuard {
  const std::shared_ptr<WorkerReleaseState> state;
  ~ReturnGuard(){state->allowReturn.store(true,std::memory_order_release);}
 } guard{state};
 {
  Image image(device);image.context->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
  auto layout=CreateDrawingTextLayout(write,format,L"X",200,80);
  ComPtr<IDWriteInlineObject> object;object.Attach(new WorkerReleaseObject(state));
  HR(layout->SetInlineObject(object.Get(),{0,1}));
  DirectWriteAsyncTextCache cache;
  cache.Request(image.context.Get(),layout.Get(),image.brush.Get(),{5,4},1);
  const auto enteredStart=std::chrono::steady_clock::now();
  while(!state->entered.load(std::memory_order_acquire)&&std::chrono::steady_clock::now()-enteredStart<std::chrono::seconds(10))
   std::this_thread::sleep_for(std::chrono::milliseconds(1));
  Require(state->entered.load(std::memory_order_acquire),"detached source worker reaches the retained layout resource");
  Require(static_cast<uint32_t>(winrt::get_module_lock())>baselineLocks,"blocked source worker holds the module lease");
 }
 Require(!state->released.load(std::memory_order_acquire),"closed cache leaves the blocked worker as final layout owner");
 state->allowReturn.store(true,std::memory_order_release);
 const auto releasedStart=std::chrono::steady_clock::now();
 while(!state->released.load(std::memory_order_acquire)&&std::chrono::steady_clock::now()-releasedStart<std::chrono::seconds(10))
  std::this_thread::sleep_for(std::chrono::milliseconds(1));
 Require(state->released.load(std::memory_order_acquire),"closed source worker releases its last COM layout resource");
 Require(!state->timedOut.load(),"source resource callback completes without timing out");
 Require(state->releaseThread!=GetCurrentThreadId(),"last COM resource is released by the background worker");
 Require(SUCCEEDED(state->apartmentResult)&&state->apartment==APTTYPE_MTA,"last COM resource is released before worker CoUninitialize");
 Require(state->moduleLocks>baselineLocks,"last COM resource is released before worker module lease ends");
 const auto settledStart=std::chrono::steady_clock::now();
 while(static_cast<uint32_t>(winrt::get_module_lock())!=baselineLocks&&std::chrono::steady_clock::now()-settledStart<std::chrono::seconds(10))
  std::this_thread::sleep_for(std::chrono::milliseconds(1));
 Require(static_cast<uint32_t>(winrt::get_module_lock())==baselineLocks,"closed source worker returns the module lease after COM release");
}
}

size_t CheckAsyncSourcePixels() {
  sourceChecks = 0;
  const auto baselineModuleLocks=static_cast<uint32_t>(winrt::get_module_lock());
  ComPtr<ID3D11Device> d3d;
  if (FAILED(D3D11CreateDevice(nullptr,D3D_DRIVER_TYPE_HARDWARE,nullptr,D3D11_CREATE_DEVICE_BGRA_SUPPORT,nullptr,0,D3D11_SDK_VERSION,&d3d,nullptr,nullptr))) {
    std::cout << "SKIP: async source pixels need a hardware Direct3D device.\n";
    return 0;
  }
  ComPtr<IDXGIDevice> dxgi; HR(d3d.As(&dxgi)); ComPtr<ID2D1Factory1> factory;
  HR(D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED,IID_PPV_ARGS(&factory)));
  ComPtr<ID2D1Device> device; HR(factory->CreateDevice(dxgi.Get(),&device));
  ComPtr<IDWriteFactory> write; HR(DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED,__uuidof(IDWriteFactory),reinterpret_cast<IUnknown**>(write.GetAddressOf())));
  const std::wstring text=L"😀 😅 🫠 🥹 🫨 🥳 🧪 ✨ · 👍 👍🏻 👍🏽 👍🏿 · 👩🏽‍💻 👨‍👩‍👧‍👦 🧑‍🚀 · 🇨🇳 🇯🇵 🇺🇳 · 🏳️‍🌈 🏴‍☠️ · 1️⃣ #️⃣ *️⃣";
  size_t comparisons=0,differentBytes=0,greaterTwo=0; unsigned maximum=0;
  DirectWriteAsyncTextCache smallFontCache, largeFontCache;
  for(float size:{14.f,28.f}) for(auto aa:{D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,D2D1_TEXT_ANTIALIAS_MODE_ALIASED})
  for(auto colour:{D2D1::ColorF(0,1),D2D1::ColorF(0xD4D4D4,1),D2D1::ColorF(0x317AF2,.6f)}) {
   auto &cache = size == 14.f ? smallFontCache : largeFontCache;
   ComPtr<IDWriteTextFormat> format; HR(write->CreateTextFormat(L"Consolas",nullptr,DWRITE_FONT_WEIGHT_NORMAL,DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,size,L"en-US",&format));
   HR(format->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP));
   ComPtr<IDWriteTextLayout> metricsX;HR(write->CreateTextLayout(L"X",1,format.Get(),10000,1000,&metricsX));
   DWRITE_LINE_METRICS lineMetrics[2]{};UINT32 lineCount{};HR(metricsX->GetLineMetrics(lineMetrics,2,&lineCount));
   HR(format->SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM,lineMetrics[0].height,lineMetrics[0].baseline));
   auto layout=CreateDrawingTextLayout(write.Get(),format.Get(),text,2000,80); Require(!!layout,"drawing layout");
   Image actual(device.Get()),expected(device.Get());
   actual.context->SetTextAntialiasMode(aa);expected.context->SetTextAntialiasMode(aa);
   actual.brush->SetColor(colour);expected.brush->SetColor(colour);
   actual.context->SetTransform(D2D1::Matrix3x2F::Translation(1079,2));
   const D2D1_POINT_2F requested{82.f,-2156.7578125f};
   cache.Request(actual.context.Get(),layout.Get(),actual.brush.Get(),requested,text.size(),text);
   const auto completed = WaitForRaster(cache);
   if (completed.pending || !completed.cpuBytes) {
    std::cerr << "CACHE: entries=" << completed.entries << " pending=" << completed.pending
     << " bytes=" << completed.cpuBytes << " reserved=" << completed.reservedBytes << " size=" << size << " aa=" << int(aa) << '\n';
   }
   Require(completed.pending == 0 && completed.cpuBytes > 0, "async source raster completes");
   Require(completed.workerDeviceCreations == 1, "independent raster keys reuse the hardware worker device");
   actual.Begin(false);
   const bool ready = cache.Draw(actual.context.Get(),layout.Get(),actual.brush.Get(),requested,text);
   actual.End();
   Require(ready,"completed source raster can be uploaded");
   const auto resident=cache.Snapshot();
   Require(resident.reservedBytes<=4*1024*1024&&resident.cpuBytes<=resident.reservedBytes,
    "completed and pending reservations bound retained source pixels");
   for(bool opaque:{false,true}) for(auto translation:{D2D1::Point2F(23,17),D2D1::Point2F(-7,-2)})
   for(auto origin:{requested,D2D1::Point2F(11.f,6.2421875f),D2D1::Point2F(82.f,4.2421875f)}) {
    actual.context->SetTransform(D2D1::Matrix3x2F::Translation(translation.x,translation.y));
    expected.context->SetTransform(D2D1::Matrix3x2F::Translation(translation.x,translation.y));
    actual.Begin(opaque);Require(cache.Draw(actual.context.Get(),layout.Get(),actual.brush.Get(),origin,text),"matching origin phases reuse actual source pixels");actual.End();
    expected.Begin(opaque);expected.context->DrawTextLayout(origin,layout.Get(),expected.brush.Get(),D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);expected.End();
    const auto a=actual.Pixels(),b=expected.Pixels();
    if(origin.y>0&&origin.y<100) {
     size_t ink=0;for(size_t i=0;i<b.size();i+=4)ink+=opaque?(b[i]!=0xC2||b[i+1]!=0xA4||b[i+2]!=0x83):b[i+3]!=0;
     Require(ink>20,"independent shifted source oracle contains actual visible ink");
    }
    for(size_t i=0;i<a.size();++i){const auto delta=static_cast<unsigned>(std::abs(int(a[i])-int(b[i])));maximum=std::max(maximum,delta);differentBytes+=delta!=0;greaterTwo+=delta>2;}
    ++comparisons;
   }
   actual.context->SetTransform(D2D1::Matrix3x2F::Translation(23.5f,17));
   Require(!cache.Draw(actual.context.Get(),layout.Get(),actual.brush.Get(),requested,text),"fractional transform falls back");
   actual.context->SetTransform(D2D1::Matrix3x2F::Identity()); actual.context->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE);
   Require(!cache.Draw(actual.context.Get(),layout.Get(),actual.brush.Get(),requested,text),"ClearType falls back");
   actual.context->SetTextAntialiasMode(aa); actual.brush->SetColor(D2D1::ColorF(0x55FF11,1));
   Require(!cache.Draw(actual.context.Get(),layout.Get(),actual.brush.Get(),requested,text),"changed foreground falls back");
  }
  ComPtr<IDWriteTextFormat> admissionFormat;HR(write->CreateTextFormat(L"Consolas",nullptr,DWRITE_FONT_WEIGHT_NORMAL,DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,28,L"en-US",&admissionFormat));
  HR(admissionFormat->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP));
  CheckLookaheadRetention(device.Get(),write.Get(),admissionFormat.Get(),text);
  CheckAdapterAdmission(factory.Get(),dxgi.Get(),device.Get(),write.Get(),admissionFormat.Get());
  // Closing the source/font cache must leave the detached worker with only
  // immutable CPU work. Its module lease must be returned after completion.
  const auto settleStart=std::chrono::steady_clock::now();
  while(static_cast<uint32_t>(winrt::get_module_lock())!=baselineModuleLocks&&std::chrono::steady_clock::now()-settleStart<std::chrono::seconds(10))
   std::this_thread::sleep_for(std::chrono::milliseconds(1));
  Require(static_cast<uint32_t>(winrt::get_module_lock())==baselineModuleLocks,"raster workers return their module lifetime leases");
  const auto moduleBefore=baselineModuleLocks;
  {
   Image closing(device.Get());closing.context->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
   ComPtr<IDWriteTextFormat> format;HR(write->CreateTextFormat(L"Consolas",nullptr,DWRITE_FONT_WEIGHT_NORMAL,DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,28,L"en-US",&format));
   HR(format->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP));
   DirectWriteAsyncTextCache closingCache;
   auto first=CreateDrawingTextLayout(write.Get(),format.Get(),text,2000,80);
   auto second=CreateDrawingTextLayout(write.Get(),format.Get(),text+L" 😀",2000,80);
   auto third=CreateDrawingTextLayout(write.Get(),format.Get(),text+L" 🧪",2000,80);
   closingCache.Request(closing.context.Get(),first.Get(),closing.brush.Get(),{5,4});
   closingCache.Request(closing.context.Get(),second.Get(),closing.brush.Get(),{5,4});
   const auto beforeThird=closingCache.Snapshot();
   closingCache.Request(closing.context.Get(),third.Get(),closing.brush.Get(),{5,4});
   const auto afterThird=closingCache.Snapshot();
   Require(beforeThird.reservedBytes<=4*1024*1024&&afterThird.reservedBytes<=4*1024*1024,
    "pending requests reserve the pixel budget without underflow or over-admission");
   Require(afterThird.entries>=beforeThird.entries,"busy-worker rejection does not evict a live resident");
  }
  const auto releaseStart=std::chrono::steady_clock::now();
  while(static_cast<uint32_t>(winrt::get_module_lock())!=moduleBefore&&std::chrono::steady_clock::now()-releaseStart<std::chrono::seconds(10))
   std::this_thread::sleep_for(std::chrono::milliseconds(1));
  Require(static_cast<uint32_t>(winrt::get_module_lock())==moduleBefore,"closed-cache workers complete and release module lifetime leases");
  // Hold a worker at the gate when checking its pending module lease.
  CheckClosedWorkerReleaseOrder(device.Get(),write.Get(),admissionFormat.Get(),baselineModuleLocks);
  std::cout<<"{\"comparisons\":"<<comparisons<<",\"differentBytes\":"<<differentBytes<<",\"greaterTwo\":"<<greaterTwo<<",\"maximumDelta\":"<<maximum<<"}\n";
  Require(greaterTwo==0,"source pixel equivalence (maximum channel delta 2)");
  return sourceChecks;
}
