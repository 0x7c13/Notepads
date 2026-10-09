// Scintilla source code edit control
/** @file PlatWin.cxx
 ** Implementation of platform facilities on Windows.
 **/
// Copyright 1998-2003 by Neil Hodgson <neilh@scintilla.org>
// The License.txt file describes the conditions under which this software may be distributed.

// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#include "pch.h"

#include <cstddef>
#include <cstdlib>
#include <cstdint>
#include <cstring>
#include <cstdio>
#include <cstdarg>
#include <ctime>
#include <cmath>
#include <climits>

#include <utility>
#include <string_view>
#include <vector>
#include <map>
#include <optional>
#include <algorithm>
#include <iterator>
#include <memory>
#include <mutex>

// Want to use std::min and std::max so don't want Windows.h version of min and max
#if !defined(NOMINMAX)
#define NOMINMAX
#endif
#undef _WIN32_WINNT
#define _WIN32_WINNT 0x0A00
#undef WINVER
#define WINVER 0x0A00
#define WIN32_LEAN_AND_MEAN 1
#include <windows.h>
#include <commctrl.h>
#include <richedit.h>
#include <windowsx.h>

#if !defined(DISABLE_D2D)
#define USE_D2D 1
#endif

#if defined(USE_D2D)
#include <d2d1_1.h>
#include <d3d11_1.h>
#include <dwrite_1.h>
#endif

#include "ScintillaTypes.h"

#include "Debugging.h"
#include "Geometry.h"
#include "Platform.h"
#include "XPM.h"
#include "UniConversion.h"
#include "DBCS.h"

#include "WinTypes.h"
#include "PlatWin.h"
#if defined(USE_D2D)
#include "SurfaceD2D.h"
#endif

#include "Wrapper.h"
#include "MainWrapper.h"

using namespace Scintilla;

namespace Scintilla::Internal {

UINT DpiForWindow(WindowID wid) noexcept {
	auto wrapper{ reinterpret_cast<WinUIEditor::Wrapper *>(wid) };
	return wrapper && wrapper->LogicalDpi()
		? wrapper->LogicalDpi()
		: USER_DEFAULT_SCREEN_DPI;
}

Window::~Window() noexcept = default;

void Window::Destroy() noexcept {
	const auto wrapper{ reinterpret_cast<WinUIEditor::Wrapper *>(GetID()) };
	if (wrapper)
	{
		wrapper->Destroy();
	}
	wid = nullptr;
}

PRectangle Window::GetPosition() const {
	return PRectangle::FromInts(0, 0, 0, 0);
}

void Window::SetPosition(PRectangle rc) {
}

namespace {

RECT RectFromMonitor(HMONITOR hMonitor) noexcept {
	/*MONITORINFO mi = {};
	mi.cbSize = sizeof(mi);
	if (GetMonitorInfo(hMonitor, &mi)) {
		return mi.rcWork;
	}
	RECT rc = {0, 0, 0, 0};
	if (::SystemParametersInfoA(SPI_GETWORKAREA, 0, &rc, 0) == 0) {
		rc.left = 0;
		rc.top = 0;
		rc.right = 0;
		rc.bottom = 0;
	}
	return rc;*/
	// WinUI Todo
	return { 0, 0, 0, 0 };
}

}

void Window::SetPositionRelative(PRectangle rc, const Window *relativeTo) {
	auto wrapper{ reinterpret_cast<WinUIEditor::Wrapper *>(GetID()) };
	if (wrapper)
	{
		return wrapper->SetPositionRelative(rc, *reinterpret_cast<WinUIEditor::Wrapper *>(relativeTo->GetID()));
	}
}

PRectangle Window::GetClientPosition() const {
	auto wrapper{ reinterpret_cast<WinUIEditor::Wrapper *>(GetID()) };
	if (wrapper)
	{
		return PRectangle::FromInts(0, 0, wrapper->Width(), wrapper->Height()); // WinUI Todo
	}
	return PRectangle{};
}

void Window::Show(bool show) {
	auto wrapper{ reinterpret_cast<WinUIEditor::Wrapper *>(GetID()) };
	if (wrapper)
	{
		wrapper->Show(show);
	}
}

void Window::InvalidateAll() {
	auto wrapper{ reinterpret_cast<WinUIEditor::Wrapper *>(GetID()) };
	if (wrapper && wrapper->VsisNative())
	{
		wrapper->VsisNative()->Invalidate(RECT{0, 0, wrapper->Width(), wrapper->Height()});
	}
}

void Window::InvalidateRectangle(PRectangle rc) {
	auto wrapper{ reinterpret_cast<WinUIEditor::Wrapper *>(GetID()) };
	if (wrapper && wrapper->VsisNative())
	{
		wrapper->VsisNative()->Invalidate(RectFromPRectangle(rc));
	}
}

void Window::SetCursor(Cursor curs) {
	if (auto wrapper{ static_cast<WinUIEditor::MainWrapper *>(static_cast<WinUIEditor::Wrapper *>(GetID())) }) // Todo: Consider moving SetCursor or checking the type of window (this should only be called on main)
	{
		winrt::DCUR type{ winrt::DCUR::Arrow };
		switch (curs) {
		case Cursor::text:
			type = winrt::DCUR::IBeam;
			break;
		case Cursor::up:
			type = winrt::DCUR::UpArrow;
			break;
		case Cursor::wait:
			type = winrt::DCUR::Wait;
			break;
		case Cursor::horizontal:
			type = winrt::DCUR::SizeWestEast;
			break;
		case Cursor::vertical:
			type = winrt::DCUR::SizeNorthSouth;
			break;
		case Cursor::hand:
			type = winrt::DCUR::Hand;
			break;
		}

		wrapper->SetCursor(type);
	}
}

/* Returns rectangle of monitor pt is on, both rect and pt are in Window's
   coordinates */
PRectangle Window::GetMonitorRect(Point pt) {
	/*const PRectangle rcPosition = GetPosition();
	const POINT ptDesktop = {static_cast<LONG>(pt.x + rcPosition.left),
		static_cast<LONG>(pt.y + rcPosition.top)};
	HMONITOR hMonitor = MonitorFromPoint(ptDesktop, MONITOR_DEFAULTTONEAREST);

	const RECT rcWork = RectFromMonitor(hMonitor);
	if (rcWork.left < rcWork.right) {
		PRectangle rcMonitor(
			rcWork.left - rcPosition.left,
			rcWork.top - rcPosition.top,
			rcWork.right - rcPosition.left,
			rcWork.bottom - rcPosition.top);
		return rcMonitor;
	} else {*/
		return PRectangle(0,0,0,0);
	/*}*/
	// WinUI Todo
}

Menu::Menu() noexcept : mid{} {
}

void Menu::CreatePopUp() {
	Destroy();
	//mid = ::CreatePopupMenu();
	// WinUI Todo
}

void Menu::Destroy() noexcept {
	// WinUI Todo
	//if (mid)
		//::DestroyMenu(static_cast<HMENU>(mid));
	mid = {};
}

void Menu::Show(Point pt, const Window &w) {
	// WinUI Todo
	//::TrackPopupMenu(static_cast<HMENU>(mid),
		//TPM_RIGHTBUTTON, static_cast<int>(pt.x - 4), static_cast<int>(pt.y), 0,
		//HwndFromWindow(w), nullptr);
	Destroy();
}

namespace {

bool assertionPopUps = true;

constexpr int defaultFontSize = 8;
constexpr size_t lengthDiagnostic = 2000;

}

ColourRGBA Platform::Chrome() {
	//return ColourRGBA::FromRGB(static_cast<int>(::GetSysColor(COLOR_3DFACE)));
	return ColourRGBA{ 255, 255, 255 };
	// WinUI Todo
}

ColourRGBA Platform::ChromeHighlight() {
	//return ColourRGBA::FromRGB(static_cast<int>(::GetSysColor(COLOR_3DHIGHLIGHT)));
	return ColourRGBA{ 255, 242, 0 };
	// WinUI Todo
}

const char *Platform::DefaultFont() {
	return "Verdana";
}

int Platform::DefaultFontSize() {
	return defaultFontSize;
}

unsigned int Platform::DoubleClickTime() {
	return winrt::Windows::UI::ViewManagement::UISettings{}.DoubleClickTime();
}

void Platform::DebugDisplay(const char *s) noexcept {
	::OutputDebugStringA(s);
}

//#define TRACE

#ifdef TRACE
void Platform::DebugPrintf(const char *format, ...) noexcept {
	char buffer[lengthDiagnostic];
	va_list pArguments;
	va_start(pArguments, format);
	vsnprintf(buffer, std::size(buffer), format, pArguments);
	va_end(pArguments);
	Platform::DebugDisplay(buffer);
}
#else
void Platform::DebugPrintf(const char *, ...) noexcept {
}
#endif

bool Platform::ShowAssertionPopUps(bool assertionPopUps_) noexcept {
	const bool ret = assertionPopUps;
	assertionPopUps = assertionPopUps_;
	return ret;
}

void Platform::Assert(const char *c, const char *file, int line) noexcept {
	char buffer[lengthDiagnostic] {};
	snprintf(buffer, std::size(buffer), "Assertion [%s] failed at %s %d%s", c, file, line, assertionPopUps ? "" : "\r\n");
	// WinUI Todo
	/*if (assertionPopUps) {
		const int idButton = ::MessageBoxA({}, buffer, "Assertion failure",
			MB_ABORTRETRYIGNORE|MB_ICONHAND|MB_SETFOREGROUND|MB_TASKMODAL);
		if (idButton == IDRETRY) {
			::DebugBreak();
		} else if (idButton == IDIGNORE) {
			// all OK
		} else {
			abort();
		}
	} else {*/
		Platform::DebugDisplay(buffer);
		::DebugBreak();
		abort();
	/*}*/
}

void Platform_Initialise(void *hInstance) noexcept {
	//ListBoxX_Register();
	// WinUI Todo
}

void Platform_Finalise(bool fromDllMain) noexcept {
	if (!fromDllMain) {
#if defined(USE_D2D)
		ReleaseD2D();
#endif
		//ReleaseLibrary(hDLLShcore);
	}
	//ListBoxX_Unregister(); // WinUI Todo
}

}
