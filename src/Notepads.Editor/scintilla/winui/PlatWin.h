// Scintilla source code edit control
/** @file PlatWin.h
 ** Implementation of platform facilities on Windows.
 **/
// Copyright 1998-2011 by Neil Hodgson <neilh@scintilla.org>
// The License.txt file describes the conditions under which this software may be distributed.

#pragma once

namespace Scintilla::Internal {

#ifndef USER_DEFAULT_SCREEN_DPI
#define USER_DEFAULT_SCREEN_DPI		96
#endif
constexpr FLOAT dpiDefault = USER_DEFAULT_SCREEN_DPI;

// Used for defining font size with LOGFONT
constexpr int pointsPerInch = 72;

constexpr UINT codePageWindowsLatin = 1252;

extern void Platform_Initialise(void *hInstance) noexcept;

extern void Platform_Finalise(bool fromDllMain) noexcept;

constexpr RECT RectFromPRectangle(PRectangle prc) noexcept {
	const RECT rc = { static_cast<LONG>(prc.left), static_cast<LONG>(prc.top),
		static_cast<LONG>(prc.right), static_cast<LONG>(prc.bottom) };
	return rc;
}

UINT CodePageFromCharSet(CharacterSet characterSet, UINT documentCodePage) noexcept;

UINT DpiForWindow(WindowID wid) noexcept;

// Buffer to hold strings and string position arrays without always allocating on heap.
// May sometimes have string too long to allocate on stack. So use a fixed stack-allocated buffer
// when less than safe size otherwise allocate on heap and free automatically.
template<typename T, int lengthStandard>
class VarBuffer {
	T bufferStandard[lengthStandard];
public:
	T *buffer;
	explicit VarBuffer(size_t length) : buffer(nullptr) {
		if (length > lengthStandard) {
			buffer = new T[length];
		} else {
			buffer = bufferStandard;
		}
	}
	// Deleted so VarBuffer objects can not be copied.
	VarBuffer(const VarBuffer &) = delete;
	VarBuffer(VarBuffer &&) = delete;
	VarBuffer &operator=(const VarBuffer &) = delete;
	VarBuffer &operator=(VarBuffer &&) = delete;

	~VarBuffer() noexcept {
		if (buffer != bufferStandard) {
			delete[]buffer;
			buffer = nullptr;
		}
	}
};

constexpr int stackBufferLength = 400;
class TextWide : public VarBuffer<wchar_t, stackBufferLength> {
public:
	int tlen;	// Using int instead of size_t as most Win32 APIs take int.
	TextWide(std::string_view text, int codePage) :
		VarBuffer<wchar_t, stackBufferLength>(text.length()) {
		if (codePage == CpUtf8) {
			tlen = static_cast<int>(UTF16FromUTF8(text, buffer, text.length()));
		} else {
			// Support Asian string display in 9x English
			tlen = ::MultiByteToWideChar(codePage, 0, text.data(), static_cast<int>(text.length()),
				buffer, static_cast<int>(text.length()));
		}
	}
	[[nodiscard]] std::wstring_view AsView() const noexcept {
		return std::wstring_view(buffer, tlen);
	}
};
using TextPositions = VarBuffer<XYPOSITION, stackBufferLength>;

std::shared_ptr<Font> GetChevronFontFromSurface(Surface const &surface, XYPOSITION size);

}
