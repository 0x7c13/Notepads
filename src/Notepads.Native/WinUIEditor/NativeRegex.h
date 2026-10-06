// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once
#include "RegexText.h"
#include <atomic>
#include <chrono>
#include <functional>
#include <memory>
#include <string>
#include <winrt/WinUIEditor.h>

namespace WinUIEditor
{
	struct RegexJob
	{
		std::atomic<bool> canceled{false};
		std::chrono::steady_clock::time_point deadline{std::chrono::steady_clock::now() + std::chrono::seconds(2)};
		winrt::WinUIEditor::EditorSearchStatus Interruption() const noexcept;
		void Check() const;
	};
	struct RegexFailure
	{
		winrt::WinUIEditor::EditorSearchStatus status;
		int32_t offset{-1};
	};
	struct RegexRequest
	{
		std::wstring pattern;
		std::wstring replacement;
		bool matchCase{};
		bool previous{};
		bool wrap{};
		bool replace{};
		bool replaceAll{};
		int64_t origin{};
		int64_t excludedEmpty{-1};
	};
	winrt::WinUIEditor::EditorSearchResult RunRegex(SplitText source, RegexRequest const &request,
		RegexJob const &job, std::function<void(std::string_view)> const &emit = {});
}
