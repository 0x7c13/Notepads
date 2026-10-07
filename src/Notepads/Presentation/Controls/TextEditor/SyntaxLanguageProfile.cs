// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using Notepads.Presentation.Theming;

namespace Notepads.Presentation.Controls.TextEditor;

internal sealed class SyntaxLanguageProfile
{
    private SyntaxLanguageProfile(string lexer, string[] keywords, params (SyntaxColorRole Token, int[] Styles)[] groups)
    {
        Lexer = lexer;
        Keywords = keywords;
        foreach (var group in groups)
            foreach (var style in group.Styles) Tokens[style] = group.Token;
    }

    public string Lexer { get; }
    public string[] Keywords { get; }
    public Dictionary<string, string> Properties { get; } = new(StringComparer.Ordinal);
    public string[] PropertyNames { get; private set; }
    public string[] PropertyValues { get; private set; }
    public SyntaxColorRole[] Tokens { get; } = new SyntaxColorRole[(int)WinUIEditor.StylesCommon.Max + 1];

    // Style IDs are the pinned Lexilla 5.5.4 SciLexer.h definitions.
    public static SyntaxLanguageProfile Create(string id, string fileName)
    {
        var profile = CreateProfile(id, fileName);
        profile.PropertyNames = new string[profile.Properties.Count];
        profile.PropertyValues = new string[profile.Properties.Count];
        var index = 0;
        foreach (var property in profile.Properties)
        {
            profile.PropertyNames[index] = property.Key;
            profile.PropertyValues[index++] = property.Value;
        }
        return profile;
    }

    private static SyntaxLanguageProfile CreateProfile(string id, string fileName)
    {
        SyntaxLanguageProfile profile;
        switch (id)
        {
            case "c": case "cpp": case "csharp": case "java": case "javascript": case "typescript":
                profile = new("cpp", [GetCppKeywords(id), GetCppTypes(id)],
                    (SyntaxColorRole.Comment, [1, 2, 3, 15, 17, 18, 23, 24, 26]),
                    (SyntaxColorRole.Keyword, [5, 9]), (SyntaxColorRole.Type, [16, 19]),
                    (SyntaxColorRole.String, [6, 7, 12, 13, 14, 20, 21, 22, 25, 27]),
                    (SyntaxColorRole.Number, [4]), (SyntaxColorRole.Operator, [10]));
                profile.Properties.Add("lexer.cpp.track.preprocessor", "0");
                profile.Properties.Add("lexer.cpp.update.preprocessor", "0");
                profile.Properties.Add("lexer.cpp.allow.dollars", id is "javascript" or "typescript" or "java" ? "1" : "0");
                profile.Properties.Add("lexer.cpp.backquoted.strings", id is "javascript" or "typescript" ? "1" : "0");
                profile.Properties.Add("lexer.cpp.triplequoted.strings", id is "csharp" or "java" ? "1" : "0");
                return profile;
            case "python":
                profile = new("python", ["False None True and as assert async await break case class continue def del elif else except finally for from global if import in is lambda match nonlocal not or pass raise return try type while with yield", "bool bytes dict float frozenset int list object set str tuple"],
                    (SyntaxColorRole.Comment, [1, 12]), (SyntaxColorRole.Keyword, [5]), (SyntaxColorRole.Type, [8, 9, 14]),
                    (SyntaxColorRole.String, [3, 4, 6, 7, 13, 16, 17, 18, 19]), (SyntaxColorRole.Number, [2]),
                    (SyntaxColorRole.Operator, [10]), (SyntaxColorRole.Attribute, [15]));
                profile.Properties.Add("lexer.python.fstrings", "1");
                return profile;
            case "json":
                profile = new("json", ["false null true"], (SyntaxColorRole.String, [2, 3, 5, 9, 10]),
                    (SyntaxColorRole.Attribute, [4]), (SyntaxColorRole.Comment, [6, 7]), (SyntaxColorRole.Operator, [8]),
                    (SyntaxColorRole.Number, [1]), (SyntaxColorRole.Keyword, [11, 12]));
                profile.Properties.Add("lexer.json.allow.comments", fileName?.EndsWith(".jsonc", StringComparison.OrdinalIgnoreCase) == true ? "1" : "0");
                return profile;
            case "html": case "xml":
                profile = new(id == "xml" ? "xml" : "hypertext", ["a abbr article aside audio b base body br button canvas caption code col colgroup data datalist dd del details div dl dt em embed fieldset figcaption figure footer form h1 h2 h3 h4 h5 h6 head header hr html i iframe img input label legend li link main mark menu meta nav ol optgroup option p picture pre progress script section select small source span strong style sub summary sup table tbody td template textarea th thead time title tr u ul video", GetCppKeywords("javascript")],
                    (SyntaxColorRole.Markup, [1, 2, 11, 12, 13, 14, 18, 22]), (SyntaxColorRole.Attribute, [3, 4, 10, 23, 27, 28]),
                    (SyntaxColorRole.String, [6, 7, 19, 24, 25]), (SyntaxColorRole.Comment, [9, 20, 29, 30]), (SyntaxColorRole.Number, [5]));
                // Embedded JavaScript/VB/Python/PHP style ranges from SciLexer.h.
                profile.Set(SyntaxColorRole.Comment, 42, 43, 44, 57, 58, 59, 72, 82, 92, 107, 124, 125);
                profile.Set(SyntaxColorRole.Keyword, 47, 62, 74, 84, 96, 111, 121);
                profile.Set(SyntaxColorRole.String, 48, 49, 51, 52, 53, 63, 64, 66, 67, 68, 75, 77, 85, 87, 94, 95, 97, 98, 109, 110, 112, 113, 119, 120, 126);
                profile.Set(SyntaxColorRole.Operator, 50, 65, 101, 116, 127);
                profile.Set(SyntaxColorRole.Number, 45, 60, 73, 83, 93, 108, 122);
                profile.Properties.Add("lexer.xml.allow.scripts", id == "xml" ? "0" : "1");
                return profile;
            case "css":
                return new("css", [], (SyntaxColorRole.Comment, [9]), (SyntaxColorRole.Markup, [1, 2, 3, 4, 10, 18, 20, 21]),
                    (SyntaxColorRole.Attribute, [6, 7, 15, 16, 17, 19, 23]), (SyntaxColorRole.Keyword, [11, 12, 22]),
                    (SyntaxColorRole.String, [8, 13, 14]), (SyntaxColorRole.Operator, [5]));
            case "powershell":
                return new("powershell", ["begin break catch class clean continue data default do dynamicparam else elseif end enum exit filter finally for foreach from function hidden if in param process return static switch throw trap try until using var while workflow", "add-content get-childitem get-content get-item get-process get-service new-item remove-item select-object set-content set-item sort-object start-process stop-process test-path where-object write-error write-host write-output", "cd cls dir echo ls select sort where"],
                    (SyntaxColorRole.Comment, [1, 13, 16]), (SyntaxColorRole.Keyword, [8]), (SyntaxColorRole.Type, [9, 10, 11, 12]),
                    (SyntaxColorRole.String, [2, 3, 14, 15]), (SyntaxColorRole.Number, [4]), (SyntaxColorRole.Attribute, [5]), (SyntaxColorRole.Operator, [6]));
            case "sql":
                return new("sql", ["add all alter and any as asc begin between by case check column commit constraint create cross database default delete desc distinct drop else end except exists false foreign from full grant group having in index inner insert intersect into is join key left like limit not null offset on or order outer primary references right rollback select set table then top transaction true truncate union unique update using values view when where with", "bigint binary bit blob boolean char date datetime decimal double float int integer interval numeric real smallint text time timestamp tinyint varchar"],
                    (SyntaxColorRole.Comment, [1, 2, 3, 13, 15, 17, 18]), (SyntaxColorRole.Keyword, [5, 8, 9]),
                    (SyntaxColorRole.Type, [16]), (SyntaxColorRole.String, [6, 7, 23, 24]), (SyntaxColorRole.Number, [4]), (SyntaxColorRole.Operator, [10]));
            case "yaml":
                return new("yaml", ["false null true"], (SyntaxColorRole.Comment, [1]), (SyntaxColorRole.Attribute, [2, 5]),
                    (SyntaxColorRole.Keyword, [3, 6]), (SyntaxColorRole.Number, [4]), (SyntaxColorRole.String, [7]), (SyntaxColorRole.Operator, [9]));
            case "markdown":
                profile = new("markdown", [], (SyntaxColorRole.Markup, [2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 17]),
                    (SyntaxColorRole.Comment, [15]), (SyntaxColorRole.Attribute, [18]), (SyntaxColorRole.String, [19, 20, 21]));
                profile.Properties.Add("lexer.markdown.header.eolfill", "1");
                return profile;
            case "rust":
                return new("rust", ["Self as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return self static struct super trait true type unsafe use where while", "bool char f32 f64 i8 i16 i32 i64 i128 isize str u8 u16 u32 u64 u128 usize", "Box Option Result String Vec"],
                    (SyntaxColorRole.Comment, [1, 2, 3, 4]), (SyntaxColorRole.Keyword, [6]), (SyntaxColorRole.Type, [7, 8, 9, 10, 11, 12]),
                    (SyntaxColorRole.String, [13, 14, 15, 21, 22, 23, 24, 25]), (SyntaxColorRole.Number, [5]), (SyntaxColorRole.Operator, [16]), (SyntaxColorRole.Attribute, [18, 19]));
            case "bash":
                return new("bash", ["alias bg bind break builtin case cd command compgen complete continue declare dirs disown do done echo elif else enable esac eval exec exit export false fc fg fi for function getopts hash help history if in jobs kill let local logout mapfile popd printf pushd pwd read readonly return select set shift shopt source test then time times trap true type typeset ulimit umask unalias unset until wait while"],
                    (SyntaxColorRole.Comment, [2]), (SyntaxColorRole.Keyword, [4]), (SyntaxColorRole.String, [5, 6, 11, 12, 13]),
                    (SyntaxColorRole.Number, [3]), (SyntaxColorRole.Operator, [7]), (SyntaxColorRole.Attribute, [9, 10]));
            case "lua":
                return new("lua", ["and break do else elseif end false for function goto if in local nil not or repeat return then true until while", "_G _VERSION assert collectgarbage coroutine debug dofile error getmetatable io ipairs load loadfile math next os package pairs pcall print rawequal rawget rawlen rawset require select setmetatable string table tonumber tostring type utf8 xpcall"],
                    (SyntaxColorRole.Comment, [1, 2, 3]), (SyntaxColorRole.Keyword, [5, 9]), (SyntaxColorRole.Type, [13, 14, 15, 16, 17, 18, 19]),
                    (SyntaxColorRole.String, [6, 7, 8, 12]), (SyntaxColorRole.Number, [4]), (SyntaxColorRole.Operator, [10]));
            case "toml":
                return new("toml", ["false inf nan true"], (SyntaxColorRole.Comment, [1]), (SyntaxColorRole.Keyword, [3]),
                    (SyntaxColorRole.Number, [4, 14]), (SyntaxColorRole.Markup, [5]), (SyntaxColorRole.Attribute, [6]),
                    (SyntaxColorRole.Operator, [8]), (SyntaxColorRole.String, [9, 10, 11, 12, 13, 15]));
            default:
                return new("", []);
        }
    }

    private void Set(SyntaxColorRole token, params int[] styles)
    {
        foreach (var style in styles) Tokens[style] = token;
    }

    private static string GetCppTypes(string id) => id switch
    {
        "csharp" => "Boolean Byte DateTime Decimal Double Exception Guid Int16 Int32 Int64 List Object SByte Single String Task UInt16 UInt32 UInt64 ValueTask",
        "java" => "Boolean Byte Character Double Exception Float Integer Long Object Short String System",
        "typescript" => "Array Date Error Map Promise Record ReadonlyArray Set String",
        "javascript" => "Array Boolean Date Error JSON Map Math Number Object Promise RegExp Set String",
        _ => "size_t ptrdiff_t int8_t int16_t int32_t int64_t uint8_t uint16_t uint32_t uint64_t"
    };

    private static string GetCppKeywords(string id) => id switch
    {
        "csharp" => "abstract add alias and as ascending async await base bool break by byte case catch char checked class const continue decimal default delegate descending do double dynamic else enum equals event explicit extern false file finally fixed float for foreach from get global goto group if implicit in init int interface internal into is join let lock long managed nameof namespace new not notnull null object on operator or orderby out override params partial private protected public readonly record ref remove required return sbyte scoped sealed select set short sizeof stackalloc static string struct switch this throw true try typeof uint ulong unchecked unmanaged unsafe ushort using value var virtual void volatile when where while with yield",
        "java" => "abstract assert boolean break byte case catch char class const continue default do double else enum extends final finally float for if implements import instanceof int interface long native new null package private protected public record return sealed short static strictfp super switch synchronized this throw throws transient true try var void volatile while false yield",
        "javascript" or "typescript" => "abstract any as async await bigint boolean break case catch class const constructor continue debugger declare default delete do else enum export extends false finally for from function get if implements import in infer instanceof interface is keyof let module namespace never new null number object of package private protected public readonly require return satisfies set static string super switch symbol this throw true try type typeof undefined unknown var void while with yield",
        "c" => "auto break case char const continue default do double else enum extern float for goto if inline int long register restrict return short signed sizeof static struct switch typedef union unsigned void volatile while _Alignas _Alignof _Atomic _Bool _Complex _Generic _Imaginary _Noreturn _Static_assert _Thread_local",
        _ => "alignas alignof and asm auto bitand bitor bool break case catch char char8_t char16_t char32_t class compl concept const consteval constexpr constinit const_cast continue co_await co_return co_yield decltype default delete do double dynamic_cast else enum explicit export extern false float for friend goto if inline int long mutable namespace new noexcept not nullptr operator or private protected public register reinterpret_cast requires return short signed sizeof static static_assert static_cast struct switch template this thread_local throw true try typedef typeid typename union unsigned using virtual void volatile wchar_t while xor"
    };
}
