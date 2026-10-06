// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include <windows.h>
#include "RegexText.h"
#include <algorithm>
#include <cstdio>
#include <stdexcept>
#include <string>

namespace
{
	void Check(bool condition, const char *description)
	{
		if (!condition) throw std::runtime_error(description);
	}

	struct Text
	{
		UText value = UTEXT_INITIALIZER;
		~Text() { utext_close(&value); }
	};

	void CheckSource(std::string const &source)
	{
		auto logical = source;
		std::replace(logical.begin(), logical.end(), '\r', '\n');
		// Every physical gap is tested, including the middle of a UTF-8 scalar.
		for (size_t gap = 0; gap <= source.size(); ++gap)
		{
			UErrorCode error = U_ZERO_ERROR;
			Text custom, reference, clone;
			WinUIEditor::OpenSplitText(&custom.value, {
				std::string_view(source.data(), gap), std::string_view(source.data() + gap, source.size() - gap)}, &error);
			utext_openUTF8(&reference.value, logical.data(), static_cast<int64_t>(logical.size()), &error);
			Check(U_SUCCESS(error), "open provider and counted UTF-8 reference");
			for (size_t index = 0; index <= source.size(); ++index)
			{
				Check(utext_next32From(&custom.value, index) == utext_next32From(&reference.value, index), "forward scalar");
				Check(utext_getNativeIndex(&custom.value) == utext_getNativeIndex(&reference.value), "forward native offset");
				utext_setNativeIndex(&custom.value, index);
				utext_setNativeIndex(&reference.value, index);
				Check(utext_previous32(&custom.value) == utext_previous32(&reference.value), "backward scalar");
				Check(utext_getNativeIndex(&custom.value) == utext_getNativeIndex(&reference.value), "backward native offset");
			}
			utext_clone(&clone.value, &custom.value, false, true, &error);
			Check(U_SUCCESS(error), "independent shallow clone");
			utext_setNativeIndex(&clone.value, 0);
			utext_setNativeIndex(&custom.value, static_cast<int64_t>(source.size()));
			Check(utext_getNativeIndex(&clone.value) == 0, "clone conversion buffer and position are independent");

			error = U_ZERO_ERROR;
			const auto required = utext_extract(&custom.value, 0, source.size(), nullptr, 0, &error);
			Check(error == U_BUFFER_OVERFLOW_ERROR || source.empty(), "extraction preflight");
			std::u16string actual(static_cast<size_t>(required) + 1, u'!');
			std::u16string expected(actual.size(), u'!');
			error = U_ZERO_ERROR;
			Check(utext_extract(&custom.value, 0, source.size(), actual.data(), required + 1, &error) == required && U_SUCCESS(error), "counted extraction");
			utext_extract(&reference.value, 0, source.size(), expected.data(), required + 1, &error);
			Check(actual == expected && actual.back() == 0, "Unicode NUL and terminator extraction");
			error = U_ZERO_ERROR;
			utext_extract(&custom.value, 0, source.size(), actual.data(), required, &error);
			Check(error == U_STRING_NOT_TERMINATED_WARNING, "exact capacity has no room for terminator");
		}
	}

	void CheckMalformed()
	{
		const std::string source("\x80\xc0\xaf\xed\xa0\x80\xf4\x90\x80\x80\xe2\x82", 12);
		for (size_t gap = 0; gap <= source.size(); ++gap)
		{
			UErrorCode error = U_ZERO_ERROR;
			Text text;
			WinUIEditor::OpenSplitText(&text.value, {
				std::string_view(source.data(), gap), std::string_view(source.data() + gap, source.size() - gap)}, &error);
			for (size_t index = 0; index < source.size(); ++index)
			{
				Check(utext_next32From(&text.value, index) == 0xfffd, "invalid UTF-8 replaces exactly one byte");
				Check(utext_getNativeIndex(&text.value) == static_cast<int64_t>(index + 1), "invalid UTF-8 always advances");
			}
		}
	}
}

int main()
{
	const auto initialized = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
	try
	{
		Check(SUCCEEDED(initialized), "initialize Windows ICU worker COM");
		std::string source;
		for (int i = 0; i < 30; ++i) source.append("abc\xF0\x9F\x98\x80\xE4\xB8\xAD\r", 11);
		source.push_back(0);
		source.append("end");
		CheckSource(source);
		CheckSource("");
		CheckMalformed();
		CoUninitialize();
		std::puts("PASS: production UText gap/window boundaries, Unicode/native offsets, NUL, clones, extraction, malformed UTF-8.");
		return 0;
	}
	catch (std::exception const &error)
	{
		if (SUCCEEDED(initialized)) CoUninitialize();
		std::printf("FAIL: %s\n", error.what());
		return 1;
	}
}
