// Scintilla source code edit control
/** @file ListBox.cxx
 ** Implementation of list box on Windows.
 **/
// Copyright 1998-2003 by Neil Hodgson <neilh@scintilla.org>
// The License.txt file describes the conditions under which this software may be distributed.

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

#include <string>
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
#include <shellscalingapi.h>

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
#include "ListBox.h"
#if defined(USE_D2D)
#include "SurfaceD2D.h"
#endif

#include "Helpers.h"
#include "AutocompletionWrapper.h"

using namespace Scintilla;
using namespace Scintilla::Internal;

namespace {

// These are reasonable initial guesses that may be refined by measurement or calls.
constexpr unsigned int commonCharacterWidth = 8;
constexpr int defaultVisibleRows = 8;

struct ListItemData {
	const char *text;
	int pixId;
};

class LineToItem {
	std::vector<char> words;

	std::vector<ListItemData> data;

public:
	void Clear() noexcept {
		words.clear();
		data.clear();
	}

	[[nodiscard]] ListItemData Get(size_t index) const noexcept {
		if (index < data.size()) {
			return data[index];
		}
		ListItemData missing = {"", -1};
		return missing;
	}
	[[nodiscard]] int Count() const noexcept {
		return static_cast<int>(data.size());
	}

	void AllocItem(const char *text, int pixId) {
		const ListItemData lid = { text, pixId };
		data.push_back(lid);
	}

	char *SetWords(const char *s) {
		words = std::vector<char>(s, s+strlen(s)+1);
		return words.data();
	}
};

}

class ListBoxWinUI : public ListBox {
public:
	ListBoxWinUI() noexcept;
	virtual ~ListBoxWinUI() noexcept override;
	// winui todo constructor/destructor

	virtual void SetFont(const Font *font);
	virtual void Create(Window &parent, int ctrlID, Point location, int lineHeight_, bool unicodeMode_, Scintilla::Technology technology_);
	virtual void SetAverageCharWidth(int width);
	virtual void SetVisibleRows(int rows);
	virtual int GetVisibleRows() const;
	virtual PRectangle GetDesiredRect();
	virtual int CaretFromEdge();
	virtual void Clear() noexcept;
	virtual void Append(char *s, int type = -1);
	virtual int Length();
	virtual void Select(int n);
	virtual int GetSelection();
	virtual int Find(const char *prefix);
	virtual std::string GetValue(int n);
	virtual void RegisterImage(int type, const char *xpm_data);
	virtual void RegisterRGBAImage(int type, int width, int height, const unsigned char *pixelsImage);
	virtual void ClearRegisteredImages();
	virtual void SetDelegate(IListBoxDelegate *lbDelegate);
	virtual void SetList(const char* list, char separator, char typesep);
	virtual void SetOptions(ListOptions options_);

	void AppendListItem(const char *text, const char *numword);

private:
	int _desiredVisibleRows = defaultVisibleRows;
	unsigned int _aveCharWidth = commonCharacterWidth;
	LineToItem _lti;

	WinUIEditor::AutocompletionWrapper *Wrapper();
};

ListBoxWinUI::ListBoxWinUI() noexcept
{
	// winui todo constructor/destructor
}

ListBoxWinUI::~ListBoxWinUI() noexcept
{
	// winui todo constructor/destructor
}

void ListBoxWinUI::SetFont(const Font *font)
{
	// winui todo
}

void ListBoxWinUI::Create(Window &parent, int ctrlID, Point location, int lineHeight_, bool unicodeMode_, Scintilla::Technology technology_)
{
	const auto wrapper{ static_cast<WinUIEditor::Wrapper *>(parent.GetID()) };
	wid = static_cast<WinUIEditor::Wrapper *>(wrapper->CreateAutocompletionWindow().get());
}

void ListBoxWinUI::SetAverageCharWidth(int width)
{
	_aveCharWidth = width;
}

void ListBoxWinUI::SetVisibleRows(int rows)
{
	_desiredVisibleRows = rows;
}

int ListBoxWinUI::GetVisibleRows() const
{
	return _desiredVisibleRows;
}

PRectangle ListBoxWinUI::GetDesiredRect()
{
	PRectangle rcDesired = GetPosition();
	const auto dpiScale{ Wrapper()->LogicalDpi() / 96.f };
	rcDesired.right += WinUIEditor::ConvertFromDipToPixelUnit(432, dpiScale);
	rcDesired.bottom += WinUIEditor::ConvertFromDipToPixelUnit(32 * std::min(_desiredVisibleRows, _lti.Count()) + 6, dpiScale); // winui todo

	XYPOSITION adjustmentForScaling = 0;
	float discard;
	const auto scalingFix{ std::modff(dpiScale, &discard) };
	if (scalingFix == 0.25f)
	{
		adjustmentForScaling = -1;
	}
	else if (scalingFix == 0.5f)
	{
		adjustmentForScaling = 1;
	}
	rcDesired.bottom += adjustmentForScaling;

	return rcDesired;
}

int ListBoxWinUI::CaretFromEdge()
{
	return 0;
}

void ListBoxWinUI::Clear() noexcept
{
	Wrapper()->Clear();
	_lti.Clear();
}

void ListBoxWinUI::Append(char *s, int type)
{
	// This method is no longer called in Scintilla
	PLATFORM_ASSERT(false);
}

int ListBoxWinUI::Length()
{
	return _lti.Count();
}

void ListBoxWinUI::Select(int n)
{
	Wrapper()->SelectedIndex(n);
}

int ListBoxWinUI::GetSelection()
{
	return Wrapper()->SelectedIndex();
}

int ListBoxWinUI::Find(const char *prefix)
{
	// This is not actually called at present
	return -1;
}

std::string ListBoxWinUI::GetValue(int n)
{
	const ListItemData item = _lti.Get(n);
	return item.text;
}

void ListBoxWinUI::RegisterImage(int type, const char *xpm_data)
{
	// winui todo
}

void ListBoxWinUI::RegisterRGBAImage(int type, int width, int height, const unsigned char *pixelsImage)
{
	// winui todo
}

void ListBoxWinUI::ClearRegisteredImages()
{
	// winui todo
}

void ListBoxWinUI::SetDelegate(IListBoxDelegate *lbDelegate)
{
	Wrapper()->SetListBoxDelegate(lbDelegate);
}

void ListBoxWinUI::AppendListItem(const char *text, const char *numword)
{
	int pixId = -1;
	if (numword)
	{
		pixId = 0;
		char ch;
		while ((ch = *++numword) != '\0')
		{
			pixId = 10 * pixId + (ch - '0');
		}
	}

	_lti.AllocItem(text, pixId);
	// winui todo maxItemCharacters = std::max(maxItemCharacters, static_cast<int>(strlen(text)));
}

WinUIEditor::AutocompletionWrapper *ListBoxWinUI::Wrapper()
{
	return static_cast<WinUIEditor::AutocompletionWrapper *>(static_cast<WinUIEditor::Wrapper *>(GetID()));
}

void ListBoxWinUI::SetList(const char *list, char separator, char typesep)
{
	// Turn off redraw while populating the list - this has a significant effect, even if
	// the listbox is not visible.
	//SetRedraw(false);
	Clear();
	const size_t size = strlen(list);
	char *words = _lti.SetWords(list);
	const char *startword = words;
	char *numword = nullptr;
	for (size_t i = 0; i < size; i++) {
		if (words[i] == separator) {
			words[i] = '\0';
			if (numword)
				*numword = '\0';
			AppendListItem(startword, numword);
			startword = words + i + 1;
			numword = nullptr;
		}
		else if (words[i] == typesep) {
			numword = words + i;
		}
	}
	if (startword) {
		if (numword)
			*numword = '\0';
		AppendListItem(startword, numword);
	}

	// Finally populate the listbox itself with the correct number of items
	const int count = _lti.Count();
	const auto wrapper{ Wrapper() };
	//::SendMessage(lb, LB_INITSTORAGE, count, 0);
	for (intptr_t j = 0; j < count; j++) {
		//ListBox_AddItemData(lb, j + 1);
		wrapper->Append(winrt::to_hstring(_lti.Get(j).text));
	}
	//SetRedraw(true);
}

void ListBoxWinUI::SetOptions(ListOptions options_)
{

}

std::unique_ptr<ListBox> ListBox::Allocate() {
	return std::make_unique<ListBoxWinUI>();
}

namespace Scintilla::Internal {

ListBox::ListBox() noexcept = default;

ListBox::~ListBox() noexcept = default;

}
