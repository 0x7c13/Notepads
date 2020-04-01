namespace Notepads.Settings
{
    internal static class SettingsKey
    {
        // App related
        internal static string AppVersionStr = "AppVersionStr";
        internal static string IsJumpListOutOfDateBool = "IsJumpListOutOfDateBool"; 
        internal static string ActiveInstanceIdStr = "ActiveInstanceIdStr";
        internal static string AlwaysOpenNewWindowBool = "AlwaysOpenNewWindowBool";

        // Theme related
        internal static string RequestedThemeStr = "RequestedThemeStr";
        internal static string UseWindowsThemeBool = "UseWindowsThemeBool";
        internal static string AppBackgroundTintOpacityDouble = "AppBackgroundTintOpacityDouble";
        internal static string AppAccentColorHexStr = "AppAccentColorHexStr";
        internal static string CustomAccentColorHexStr = "CustomAccentColorHexStr";
        internal static string UseWindowsAccentColorBool = "UseWindowsAccentColorBool";

        // Editor related
        internal static string EditorFontFamilyStr = "EditorFontFamilyStr";
        internal static string EditorFontSizeInt = "EditorFontSizeInt";
        internal static string EditorDefaultTextWrappingStr = "EditorDefaultTextWrappingStr";
        internal static string EditorDefaultLineHighlighterViewStateBool = "EditorDefaultLineHighlighterViewStateBool";
        internal static string EditorDefaultLineEndingStr = "EditorDefaultLineEndingStr";
        internal static string EditorDefaultEncodingCodePageInt = "EditorDefaultEncodingCodePageInt";
        internal static string EditorDefaultDecodingCodePageInt = "EditorDefaultDecodingCodePageInt";
        internal static string EditorDefaultUtf8EncoderShouldEmitByteOrderMarkBool = "EditorDefaultUtf8EncoderShouldEmitByteOrderMarkBool";
        internal static string EditorDefaultTabIndentsInt = "EditorDefaultTabIndentsInt";
        internal static string EditorDefaultSearchEngineStr = "EditorDefaultSearchUrlStr";
        internal static string EditorCustomMadeSearchUrlStr = "EditorCustomMadeSearchUrlStr";
        internal static string EditorShowStatusBarBool = "EditorShowStatusBarBool";
        internal static string EditorEnableSessionBackupAndRestoreBool = "EditorEnableSessionBackupAndRestoreBool";
        internal static string EditorHighlightMisspelledWordsBool = "EditorHighlightMisspelledWordsBool";
    }
}