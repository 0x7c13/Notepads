// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading.Tasks;
using Windows.Storage;

namespace Tool;

public partial class InterfaceGeneratorTool
{
    private const string GeneratedSourceHeader =
        "// Copyright 2026 by Breece Walker" + End
        + "// See src/Notepads.Native/LICENSE for the original WinUIEdit license." + End
        + "//" + End
        + "// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved." + End
        + "// See LICENSE.txt in the project root for the Notepads modifications." + End + End;

    private readonly string[] UnusedConstants = ["SCI_START", "SCI_OPTIONAL_START", "SCI_LEXER_START",];

    public async Task RunAsync(string path)
    {
        var parent = await StorageFolder.GetFolderFromPathAsync(path);

        // Hand-written API extensions are independent of the Scintilla interface.
        // Validate all fragments before replacing any generated output.
        await parent.GetFileAsync("WinUIEditor\\EditorExtensions.h");
        await parent.GetFileAsync("WinUIEditor\\EditorExtensionsState.h");
        await parent.GetFileAsync("WinUIEditor\\EditorExtensions.idl.inc");

        var ifaceWinUIEditor = await parent.GetFileAsync("WinUIEditor\\WinUIEditor.iface");
        var ifaceWinUIEditorLines = await FileIO.ReadLinesAsync(ifaceWinUIEditor);

        var ifaceScintillaFile = await parent.GetFileAsync("scintilla\\include\\Scintilla.iface");
        var ifaceScintillaLines = await FileIO.ReadLinesAsync(ifaceScintillaFile);

        var ifaceFileLexilla = await parent.GetFileAsync("lexilla\\include\\LexicalStyles.iface");
        var ifaceLinesLexilla = await FileIO.ReadLinesAsync(ifaceFileLexilla);

        var typesHeaderFile = await parent.GetFileAsync("scintilla\\include\\ScintillaTypes.h");
        var typesHeaderText = await FileIO.ReadTextAsync(typesHeaderFile);
        var flagsEnums = new List<string>(20);
        var matches = FindFlagsEnumsRegex().Matches(typesHeaderText);
        foreach (var match in matches)
        {
            var name = ((Match)match).Groups[1].Value;
            if (!flagsEnums.Contains(name))
            {
                flagsEnums.Add(name);
            }
        }

        var funs = new List<Function>(600);
        var allGets = new List<Function>(400);
        var allSets = new List<Function>(400);
        var enus = new List<Enum>(100);
        var lexes = new List<Lexer>(200);
        var vals = new Dictionary<string, int>(500);
        var alis = new Dictionary<string, string>(200);
        var evts = new List<Event>(100);
        var uses = new List<Use>(10);

        var messageEnu = new Enum("ScintillaMessage", new List<string>(0), false);

        List<string> currentComment = new List<string>();
        string currentCat = null;
        foreach (var line in ifaceWinUIEditorLines.Concat(ifaceScintillaLines).Concat(ifaceLinesLexilla))
        {
            if (Keyword(line, "cat", uses, out var cat))
            {
                currentCat = cat;
            }

            if (currentCat == "Deprecated")
            {
                continue;
            }

            if (Keyword(line, "fun", uses, out var fun))
            {
                var addFun = DecodeFunDef(fun, currentComment);
                funs.Add(addFun);
                messageEnu.Values.Add(addFun.Name, addFun.MsgValue);
            }
            else if (Keyword(line, "get", uses, out var get))
            {
                var addGet = DecodeFunDef(get, currentComment);
                allGets.Add(addGet);
                messageEnu.Values.Add(addGet.Name, addGet.MsgValue);
            }
            else if (Keyword(line, "set", uses, out var set))
            {
                var addSet = DecodeFunDef(set, currentComment);
                allSets.Add(addSet);
                messageEnu.Values.Add(addSet.Name, addSet.MsgValue);
            }
            else if (Keyword(line, "enu", uses, out var enu))
            {
                enus.Add(DecodeEnuDef(enu, flagsEnums));
            }
            else if (Keyword(line, "lex", uses, out var lex))
            {
                lexes.Add(DecodeLexDef(lex));
            }
            else if (Keyword(line, "val", uses, out var val))
            {
                var valSplit = val.Split('=');
                vals.Add(valSplit[0], (int)new Int32Converter().ConvertFromString(valSplit[1]));
            }
            else if (Keyword(line, "ali", uses, out var ali))
            {
                var aliSplit = ali.Split('=');
                alis.Add(aliSplit[0], aliSplit[1]);
            }
            else if (Keyword(line, "evt", uses, out var evt))
            {
                evts.Add(DecodeEvtDef(evt, currentComment));
            }
            else if (Keyword(line, "use", uses, out var use))
            {
                uses.Add(DecodeUseDef(use));
            }

            if (line.StartsWith("# "))
            {
                currentComment.Add(line[2..]);
            }
            else
            {
                currentComment.Clear();
            }
        }

        var extraConstantsEnum = new Enum("EditorConstants", new List<string>(0), false);
        enus.Add(extraConstantsEnum);

        foreach (var val in vals)
        {
            var match = enus.SingleOrDefault(e => e.Prefixes.Count(p => val.Key.StartsWith(p)) == 1);
            if (match == null)
            {
                var lexMatches = lexes.Where(l => l.Prefixes.Count(p => val.Key.StartsWith(p)) == 1).ToList();
                if (lexMatches.Count == 0)
                {

                    if (Array.IndexOf(UnusedConstants, val.Key) == -1)
                    {
                        extraConstantsEnum.Values.Add(UppercaseSnakeToPascalCase(val.Key), val.Value);
                    }
                }
                else
                {
                    foreach (var lexMatch in lexMatches)
                    {
                        lexMatch.Values.Add(UppercaseSnakeToPascalCase(val.Key.StartsWith("SCE_") ? val.Key[4..] : val.Key), val.Value);
                    }
                }
            }
            else
            {
                var prefix = match.Prefixes.Single(val.Key.StartsWith);
                match.Values.Add(UppercaseSnakeToPascalCase(alis.TryGetValue(val.Key, out var aliVal) ? aliVal : val.Key[prefix.Length..]), val.Value);
            }
        }

        var properties = new List<Property>(200);

        var getCount = 0;
        var setCount = 0;

        foreach (var get in allGets)
        {
            var set = get.Name.IndexOf("Get") is var getIndex && getIndex != -1
                ? allSets.SingleOrDefault(s =>
                    s.Name.IndexOf("Set") is var setIndex && setIndex != -1
                    && get.Name[(getIndex + 3)..] == s.Name[(setIndex + 3)..]
                    && get.Name[..getIndex] == s.Name[..setIndex])
                : null;
            if (get.Params.Count == 0 && (set == null || (set.Params.Count == 1 && get.RetType == set.Params[0].Type)))
            {
                properties.Add(new Property(getIndex != -1 ? get.Name[..getIndex] + get.Name[(getIndex + 3)..] : get.Name, get.RetType, get.Msg, set?.Msg ?? default, get.MsgValue, set?.MsgValue ?? default, set?.Params[0].Position ?? ParamType.None, new List<string>(get.Comment), set != null ? new List<string>(set.Comment) : new List<string>(0)));
                getCount++;
                if (set != null)
                {
                    setCount++;
                }
            }
            else
            {
                funs.Add(get);
                getCount++;
            }
        }

        foreach (var set in allSets)
        {
            var setIndex = set.Name.IndexOf("Set");
            if (setIndex == -1
                || (set.Name[..setIndex] + set.Name[(setIndex + 3)..] is var name
                    && !properties.Any(p => p.Name == name)))
            {
                funs.Add(set);
                setCount++;
            }
        }

        if (getCount != allGets.Count || setCount != allSets.Count)
        {
            throw new InvalidOperationException("Missing or extra getters or setters");
        }

        var idlSB = new StringBuilder(GeneratedSourceHeader + "import \"EditorUtf8Reader.idl\";" + End + "import \"EditorJournalCheckpoint.idl\";" + End + "import \"EditorSearch.idl\";" + End + "import \"EditorDiffResult.idl\";" + End + End + "namespace WinUIEditor" + End + "{" + End);
        var cppSB = new StringBuilder(
            GeneratedSourceHeader + "#include \"pch.h\"" + End + "#include \"EditorBaseControl.h\"" + End + "#include \"EditorWrapper.h\"" + End + "#include \"Editor.g.cpp\"" + End
            + End
            + "namespace winrt::WinUIEditor::implementation" + End
            + "{" + End);
        var hSB = new StringBuilder(GeneratedSourceHeader + "#pragma once" + End + "#include \"Editor.g.h\"" + End + "#include \"EditorBaseControl.h\"" + End + "#include \"NativeRegex.h\"" + End + "#include \"NativeDiff.h\"" + End);

        foreach (var evt in evts)
        {
            WriteHEventArgsInclude(hSB, evt);
        }

        hSB.Append(End
            + "namespace winrt::WinUIEditor::implementation" + End
            + "{" + End);

        WriteIdlEnu(idlSB, messageEnu);

        foreach (var enu in enus)
        {
            WriteIdlEnu(idlSB, enu);
        }

        foreach (var lex in lexes)
        {
            WriteIdlLex(idlSB, lex);
        }

        foreach (var evt in evts)
        {
            WriteIdlEventArgs(idlSB, evt);
            WriteCppEventArgs(cppSB, evt, false);
            WriteCppEventArgs(hSB, evt, true);
        }

        cppSB.Append(In + "Editor::Editor(com_ptr<EditorBaseControl> const &editor)" + End
            + In + In + ": _editor{ editor }" + End
            + In + "{" + End
            + In + "}" + End + End
            + In + "void Editor::ProcessEvent(Scintilla::NotificationData *data)" + End
            + In + "{" + End
            + In + In + "switch (static_cast<int>(data->nmhdr.code))" + End
            + In + In + "{" + End);

        hSB.Append(In + "struct Editor : EditorT<Editor>" + End
            + In + "{" + End
            + "#include \"EditorExtensions.h\"" + End + End
            + In + In + "Editor(com_ptr<EditorBaseControl> const &editor);" + End + End
            + In + In + "void ProcessEvent(Scintilla::NotificationData *data);" + End
            + End);

        foreach (var evt in evts)
        {
            WriteIdlDelegate(idlSB, evt);
            WriteCppEventInvoker(cppSB, evt);
        }

        cppSB.Append(In + In + "}" + End + In + "}" + End + End);

        if (evts.Count != 0)
        {
            idlSB.Append(End);
        }

        idlSB.Append(In + "[default_interface]" + End + In + "runtimeclass Editor" + End + In + "{" + End
            + "#include \"EditorExtensions.idl.inc\"" + End + End);

        foreach (var evt in evts)
        {
            WriteIdlEvent(idlSB, evt);
            WriteCppEvent(cppSB, evt, false);
            WriteCppEvent(hSB, evt, true);
        }

        if (evts.Count != 0)
        {
            idlSB.Append(End);
        }

        foreach (var prop in properties)
        {
            WriteIdlProperty(idlSB, prop);
            WriteCppProperty(cppSB, prop, false);
            WriteCppProperty(hSB, prop, true);
        }

        foreach (var fun in funs)
        {
            WriteIdlFun(idlSB, fun);
            WriteCppFunction(cppSB, fun, false);
            WriteCppFunction(hSB, fun, true);
        }

        if (funs.Count != 0)
        {
            idlSB.Length -= End.Length;
            cppSB.Length -= End.Length;
        }

        hSB.Append(In).Append("private:").Append(End);
        hSB.Append("#include \"EditorExtensionsState.h\"").Append(End).Append(End);

        foreach (var evt in evts)
        {
            WriteCppEventField(hSB, evt);
        }

        if (evts.Count != 0)
        {
            hSB.Append(End);
        }

        idlSB.Append(In + "}" + End + "}" + End);
        cppSB.Append("}" + End);
        hSB.Append(In + In + "weak_ref<EditorBaseControl> _editor{ nullptr };" + End + In + "};" + End + "}" + End);

        var idlFile = await parent.CreateFileAsync("WinUIEditor\\EditorWrapper.idl", CreationCollisionOption.ReplaceExisting);
        await FileIO.WriteTextAsync(idlFile, idlSB.ToString());

        var cppFile = await parent.CreateFileAsync("WinUIEditor\\EditorWrapper.cpp", CreationCollisionOption.ReplaceExisting);
        await FileIO.WriteTextAsync(cppFile, cppSB.ToString());

        var hFile = await parent.CreateFileAsync("WinUIEditor\\EditorWrapper.h", CreationCollisionOption.ReplaceExisting);
        await FileIO.WriteTextAsync(hFile, hSB.ToString());
    }

    [GeneratedRegex("operator\\|\\((\\w+)")]
    private static partial Regex FindFlagsEnumsRegex();

    private static bool Keyword(string line, string keyword, List<Use> uses, out string def)
    {
        if (uses.LastOrDefault(u => u.Find == line) is { } use)
        {
            line = use.Replace;
        }

        if (line.StartsWith(keyword))
        {
            def = line[(keyword.Length + 1)..];
            return true;
        }
        else
        {
            def = null;
            return false;
        }
    }

    private static string UppercaseSnakeToPascalCase(string value)
    {
        var sb = new StringBuilder();
        foreach (var part in value.Trim('_').Split('_'))
        {
            sb.Append(part[0..1]).Append(part[1..].ToLowerInvariant());
        }
        return sb.ToString();
    }

    private record Property(string Name, string Type, string GetMsg, string SetMsg, int GetMsgValue, int SetMsgValue, ParamType SetPosition, IList<string> GetComment, IList<string> SetComment);

    private record Function(string Name, string RetType, IList<(string Type, string Name, ParamType Position)> Params, string Msg, int MsgValue, IList<string> Comment);

    private record Event(string Name, string RetType, IList<(string Type, string Name)> Params, int Msg, IList<string> Comment);

    private record Enum(string Name, IReadOnlyList<string> Prefixes, bool Flags)
    {
        public IDictionary<string, int> Values { get; } = new Dictionary<string, int>();
    }

    private record Lexer(string Name, string Value, IReadOnlyList<string> Prefixes)
    {
        public IDictionary<string, int> Values { get; } = new Dictionary<string, int>(2);
    }

    private record Use(string Find, string Replace);

    private enum ParamType
    {
        None,
        WParam,
        LParam,
    }

    private static Function DecodeFunDef(string def, IList<string> comment)
    {
        var groups = FunDefRegex().Match(def).Groups;
        var paramList = new List<(string Type, string Name, ParamType Position)>(2);
        if (!string.IsNullOrEmpty(groups[4].Value))
        {
            paramList.Add((groups[4].Value, groups[5].Value, ParamType.WParam));
        }
        if (!string.IsNullOrEmpty(groups[6].Value))
        {
            paramList.Add((groups[6].Value, groups[7].Value, ParamType.LParam));
        }
        return new Function(groups[2].Value, groups[1].Value, paramList, groups[2].Value, int.Parse(groups[3].Value), new List<string>(comment));
    }

    [GeneratedRegex("(\\w+) (\\w+)=(\\w+)\\((\\w+)?[ ]?(\\w+)?,[ ]?(\\w+)?[ ]?(\\w+)?\\)")]
    private static partial Regex FunDefRegex();

    private static Event DecodeEvtDef(string def, IList<string> comment)
    {
        var groups = EvtDefRegex().Match(def).Groups;
        var paramString = groups[4].Value.Split(',');
        var paramList = new List<(string Type, string Name)>();
        foreach (var p in paramString)
        {
            if (p == "void")
            {
                continue;
            }

            var pair = p.Trim().Split(' ');
            if (pair[0] == "string")
            {
                paramList.Add((pair[0], pair[1] + "AsBuffer"));
                paramList.Add(("_ForceString", pair[1]));
            }
            else
            {
                paramList.Add((pair[0], pair[1]));
            }
        }
        var name = groups[2].Value;
        if (name == "Zoom")
        {
            name = "ZoomChanged"; // Zoom event name conflicts with Zoom property
        }
        return new Event(name, groups[1].Value, paramList, int.Parse(groups[3].Value), new List<string>(comment));
    }

    [GeneratedRegex("(\\w+) (\\w+)=(\\w+)\\((.*)\\)")]
    private static partial Regex EvtDefRegex();

    private static Enum DecodeEnuDef(string def, IList<string> flagsEnums)
    {
        var groups = EnuDefRegex().Match(def).Groups;
        var name = groups[1].Value;
        return new Enum(name, groups[2].Value.Split(' '), flagsEnums.Contains(name));
    }

    [GeneratedRegex("(\\w+)=(.*)")]
    private static partial Regex EnuDefRegex();

    private static Lexer DecodeLexDef(string lex)
    {
        var groups = LexDefRegex().Match(lex).Groups;
        return new Lexer(groups[1].Value, groups[2].Value, groups[3].Value.Split(' '));
    }

    [GeneratedRegex("(\\w+)=(\\w+) (.*)")]
    private static partial Regex LexDefRegex();

    private static Use DecodeUseDef(string use)
    {
        var groups = UseDefRegex().Match(use).Groups;
        return new Use(groups[1].Value, groups[2].Value);
    }

    [GeneratedRegex("(.*) -> (.*)")]
    private static partial Regex UseDefRegex();

    private static void WriteIdlEnu(StringBuilder sb, Enum fun)
    {
        if (fun.Flags)
        {
            sb.Append(In).Append("[flags]").Append(End);
        }

        sb.Append(In).Append("enum ").Append(fun.Name).Append(End)
            .Append(In).Append('{').Append(End);

        foreach (var value in fun.Values)
        {
            sb.Append(In).Append(In).Append(value.Key).Append(" = ").Append(value.Value).Append(',').Append(End);
        }

        sb.Append(In).Append("};").Append(End).Append(End);
    }

    private static void WriteIdlLex(StringBuilder sb, Lexer lex)
    {
        sb.Append(In).Append("enum ").Append(lex.Name).Append(End)
            .Append(In).Append('{').Append(End);

        foreach (var value in lex.Values)
        {
            sb.Append(In).Append(In).Append(value.Key).Append(" = ").Append(value.Value).Append(',').Append(End);
        }

        sb.Append(In).Append("};").Append(End).Append(End);
    }

    private static void WriteComment(StringBuilder sb, IList<string> comment, int indentLevel)
    {
        var indent = string.Empty;

        for (var i = 0; i < indentLevel; i++)
        {
            indent += In;
        }

        if (comment.Count != 0)
        {
            sb.Append(indent).Append("/**").Append(End);
        }
        foreach (var line in comment)
        {
            sb.Append(indent).Append(" * ").Append(line).Append(End);
        }
        if (comment.Count != 0)
        {
            sb.Append(indent).Append(" */").Append(End);
        }
    }

    private static Function PropertyToFunction(Property prop, bool setter)
    {
        return new Function(prop.Name, setter ? "void" : prop.Type, setter ? new List<(string Type, string Name, ParamType Position)>(1)
        {
            (prop.Type, "value", prop.SetPosition),
        } : new List<(string Type, string Name, ParamType Position)>(0), setter ? prop.SetMsg : prop.GetMsg, setter ? prop.SetMsgValue : prop.GetMsgValue, setter ? prop.SetComment : prop.GetComment);
    }

    private static void WriteCppSignature(StringBuilder sb, bool header, Function func, bool usesString)
    {
        var usesStringReturn = UsesStringReturn(func);
        var retType = usesString && usesStringReturn ? "hstring" : ConvertToCppType(func.RetType, false);
        sb.Append(In).Append(header ? In : string.Empty).Append(retType).Append(header ? " " : " Editor::").Append(func.Name).Append(!usesString && NeedsStringVersion(func) ? (usesStringReturn ? "Write" : "From") + "Buffer" : string.Empty).Append('(');

        var paramsCount = 0;
        foreach (var param in func.Params)
        {
            if (!usesString || param.Type != "stringresult")
            {
                var paramType = usesString && param.Type == "string" ? "hstring const &" : ConvertToCppType(param.Type, true);
                sb.Append(paramType).Append(paramType.EndsWith('&') ? string.Empty : " ").Append(param.Name).Append(", ");
                paramsCount++;
            }
        }
        if (paramsCount != 0)
        {
            sb.Length -= ", ".Length;
        }

        sb.Append(')');
        if (header)
        {
            sb.Append(';');
            sb.Append(End);
        }
        sb.Append(End);
    }

    private static (bool NeedsResult, string Compute, string Argument, string Apply) WriteInteropCode(Function fun, string type, bool usesString, string name, ParamType position)
    {
        var ptrType = position switch
        {
            ParamType.WParam => "Scintilla::uptr_t",
            ParamType.LParam => "Scintilla::sptr_t",
            _ => throw new InvalidOperationException(),
        };

        return type switch
        {
            "cells" => (false, string.Empty, $"reinterpret_cast<{ptrType}>(" + name + ".data())", string.Empty),
            "stringresult" => (false, string.Empty, $"reinterpret_cast<{ptrType}>(" + name + " ? " + name + ".data() : nullptr)", string.Empty),
            "string" when usesString => (false,
                In + In + "const auto " + name + "Utf8{ to_string(" + name + ") };" + End,
                $"reinterpret_cast<{ptrType}>(" + name + "Utf8.c_str())", string.Empty),
            "string" => (false, string.Empty, $"reinterpret_cast<{ptrType}>(" + name + " ? " + name + ".data() : nullptr)", string.Empty),
            _ => (false, string.Empty, $"static_cast<{ptrType}>(" + name + ")", string.Empty),
        };
    }

    private static bool GetParam(Function fun, bool usesString, ParamType position, out (bool NeedsResult, string Compute, string Argument, string Apply) param)
    {
        foreach (var p in fun.Params)
        {
            if (p.Position == position)
            {
                param = WriteInteropCode(fun, p.Type, usesString, p.Name, p.Position);
                return true;
            }
        }

        param = default;
        return false;
    }

    private static void WriteCppBody(StringBuilder sb, Function func, bool usesString)
    {
        sb.Append(In).Append('{').Append(End);

        var hasWParam = GetParam(func, usesString, ParamType.WParam, out var wParam);
        var hasLParam = GetParam(func, usesString, ParamType.LParam, out var lParam);
        var needsResult = (hasWParam && wParam.NeedsResult) || (hasLParam && lParam.NeedsResult);

        if (hasWParam)
        {
            sb.Append(wParam.Compute);
        }
        if (hasLParam)
        {
            sb.Append(lParam.Compute);
        }

        if (needsResult && func.RetType == "void")
        {
            throw new InvalidOperationException();
        }

        if (usesString && UsesStringReturn(func))
        {
            if (needsResult)
            {
                throw new NotImplementedException();
            }

            // These messages return the full source length when queried, but
            // write at most the requested byte length. Reject negative lengths
            // before converting the signed WinRT parameter to native size_t.
            var boundedText = func.Msg == "GetText" || func.Msg == "GetCurLine";
            if (boundedText)
            {
                sb.Append(In).Append(In).Append("if (length <= 0) return hstring{};").Append(End);
            }

            string wParamArg;
            if (hasWParam)
            {
                wParamArg = "wParam";
                sb.Append(In).Append(In).Append("const auto wParam{ ").Append(wParam.Argument).Append(" };").Append(End);
            }
            else
            {
                wParamArg = "static_cast<Scintilla::uptr_t>(0)";
            }

            sb.Append(In).Append(In).Append("const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::").Append(func.Msg).Append(", ").Append(wParamArg).Append(", static_cast<Scintilla::sptr_t>(0))) };").Append(End)
                .Append(In).Append(In).Append("const auto len{ ")
                .Append(boundedText ? "std::min(sourceLength, static_cast<size_t>(length))" : "sourceLength")
                .Append(" };").Append(End)
                .Append(In).Append(In).Append("if (len)").Append(End)
                .Append(In).Append(In).Append('{').Append(End)
                .Append(In).Append(In).Append(In).Append("std::string value(len + 1, '\\0');").Append(End)
                .Append(In).Append(In).Append(In).Append("_editor.get()->PublicWndProc(Scintilla::Message::").Append(func.Msg).Append(", ").Append(wParamArg).Append(", reinterpret_cast<Scintilla::sptr_t>(value.data()));").Append(End)
                .Append(In).Append(In).Append(In).Append("return to_hstring(std::string_view(value.data(), len));").Append(End)
                .Append(In).Append(In).Append('}').Append(End)
                .Append(In).Append(In).Append("else").Append(End)
                .Append(In).Append(In).Append('{').Append(End)
                .Append(In).Append(In).Append(In).Append("return hstring{};").Append(End)
                .Append(In).Append(In).Append('}').Append(End);
        }
        else
        {
            sb.Append(In).Append(In);
            if (func.RetType != "void")
            {
                if (needsResult)
                {
                    sb.Append("const auto result{");
                }
                else
                {
                    sb.Append("return");
                }
                sb.Append(" static_cast<").Append(ConvertToCppType(func.RetType, false)).Append(">(");
            }
            sb.Append("_editor.get()->PublicWndProc(Scintilla::Message::")
                .Append(func.Msg).Append(", ")
                .Append(hasWParam ? wParam.Argument : "static_cast<Scintilla::uptr_t>(0)")
                .Append(", ")
                .Append(hasLParam ? lParam.Argument : "static_cast<Scintilla::sptr_t>(0)");
            if (func.RetType != "void")
            {
                sb.Append(')');
            }

            if (needsResult)
            {
                sb.Append(") };").Append(End);

                if (hasWParam)
                {
                    sb.Append(wParam.Apply);
                }
                if (hasLParam)
                {
                    sb.Append(lParam.Apply);
                }

                sb.Append(In).Append(In).Append("return result;").Append(End);
            }
            else
            {
                sb.Append(");").Append(End);

                if (hasWParam)
                {
                    sb.Append(wParam.Apply);
                }
                if (hasLParam)
                {
                    sb.Append(lParam.Apply);
                }
            }
        }

        sb.Append(In).Append('}').Append(End).Append(End);
    }

    private static void WriteCppFunc(StringBuilder sb, Function func, bool header, bool usesString)
    {
        if (!usesString)
        {
            WriteComment(sb, func.Comment, header ? 2 : 1);
        }
        else if (header)
        {
            sb.Length -= End.Length;
        }
        WriteCppSignature(sb, header, func, usesString);
        if (!header)
        {
            WriteCppBody(sb, func, usesString);
        }
    }

    private static void WriteCppFunc(StringBuilder sb, Property prop, bool setter, bool header, bool usesString)
    {
        WriteCppFunc(sb, PropertyToFunction(prop, setter), header, usesString);
    }

    private static void WriteCppProperty(StringBuilder sb, Property prop, bool header)
    {
        WriteCppFunc(sb, prop, false, header, false);

        if (prop.SetPosition != ParamType.None)
        {
            WriteCppFunc(sb, prop, true, header, false);
        }
    }

    private static void WriteCppFunction(StringBuilder sb, Function func, bool header)
    {
        WriteCppFunc(sb, func, header, false);
        if (NeedsStringVersion(func))
        {
            WriteCppFunc(sb, func, header, true);
        }
    }

    private static void WriteIdlProperty(StringBuilder sb, Property prop)
    {
        WriteComment(sb, prop.GetComment.Concat(prop.SetComment).ToList(), 2);

        sb.Append(In).Append(In).Append(ConvertToIdlType(prop.Type)).Append(' ').Append(prop.Name).Append(" { get; ");

        if (prop.SetPosition != ParamType.None)
        {
            sb.Append("set; ");
        }

        sb.Append("};").Append(End).Append(End);
    }

    private static void WriteHEventArgsInclude(StringBuilder sb, Event evt)
    {
        sb.AppendFormat("#include \"{0}EventArgs.g.h\"", evt.Name).Append(End);
    }

    private static void WriteCppEvent(StringBuilder sb, Event evt, bool header)
    {
        sb.Append(In).Append(header ? In : string.Empty).AppendFormat("event_token {0}{1}(WinUIEditor::{1}Handler const &handler){2}", !header ? "Editor::" : string.Empty, evt.Name, header ? ";" : string.Empty).Append(End);
        if (!header)
        {
            sb.Append(In).Append('{').Append(End).Append(In).Append(In).AppendFormat("return _{0}Event.add(handler);", ConvertEventNameToFieldName(evt.Name)).Append(End).Append(In).Append('}').Append(End).Append(End);
        }

        sb.Append(In).Append(header ? In : string.Empty).AppendFormat("void {0}{1}(event_token const &token){2}", !header ? "Editor::" : string.Empty, evt.Name, header ? ";" : string.Empty).Append(End);
        if (!header)
        {
            sb.Append(In).Append('{').Append(End).Append(In).Append(In).AppendFormat("_{0}Event.remove(token);", ConvertEventNameToFieldName(evt.Name)).Append(End).Append(In).Append('}').Append(End).Append(End);
        }

        if (header)
        {
            sb.Append(End);
        }
    }

    private static void WriteCppEventField(StringBuilder sb, Event evt)
    {
        sb.Append(In).Append(In).AppendFormat("event<WinUIEditor::{0}Handler> _{1}Event;", evt.Name, ConvertEventNameToFieldName(evt.Name)).Append(End);
    }

    private static void WriteCppEventArgs(StringBuilder sb, Event evt, bool header)
    {
        var hasBorrowedText = evt.Params.Any(p => p.Type == "_ForceString");
        if (header)
        {
            sb.Append(In).AppendFormat("struct {0}EventArgs : {0}EventArgsT<{0}EventArgs>", evt.Name).Append(End)
                .Append(In).Append('{').Append(End);
        }

        if (evt.Params.Count != 0)
        {
            sb.Append(In).Append(header ? In : string.Empty).AppendFormat(!header ? "{0}EventArgs::{0}EventArgs(" : "{0}EventArgs(", evt.Name);

            foreach (var param in evt.Params)
            {
                if (param.Type != "string")
                {
                    sb.AppendFormat("{0}{1}, ", param.Type == "_ForceString" ? "const char *" : (ConvertToCppType(param.Type, true) is var cppType && cppType.EndsWith('&') ? cppType : cppType + " "), param.Name);
                }
            }

            sb.Length -= ", ".Length;

            sb.Append(!header ? ")" : ");").Append(End);

            if (!header)
            {
                sb.Append(In).Append(In).Append(": ");

                foreach (var param in evt.Params)
                {
                    if (param.Type != "string")
                    {
                        sb.Append('_').Append(param.Type == "_ForceString" ? param.Name + "AsPointer" : param.Name).Append(" { ").Append(param.Name).Append(" }, ");
                    }
                }

                sb.Length -= ", ".Length;

                sb.Append(End);
            }
            else
            {
                sb.Append(End);
            }

            if (!header)
            {
                sb.Append(In).Append('{').Append(End).Append(In).Append('}').Append(End).Append(End);
            }
        }

        if (header && hasBorrowedText)
        {
            sb.Append(In).Append(In).Append("void InvalidateText() noexcept { std::lock_guard guard(_textMutex); _textExpired = true; _textAsPointer = nullptr; }").Append(End);
        }

        foreach (var param in evt.Params)
        {
            sb.Append(In).Append(header ? In : string.Empty).AppendFormat("{0} {1}{2}(){3}", ConvertToCppType(param.Type, false), !header ? evt.Name + "EventArgs::" : string.Empty, ConvertEventParameterToPropertyName(param.Name), header ? ";" : string.Empty).Append(End);
            if (!header)
            {
                sb.Append(In).Append('{').Append(End).Append(In).Append(In);

                switch (param.Type)
                {
                    case "string":
                        var name = param.Name[..^"AsBuffer".Length];
                        var hasLength = evt.Params.Any(p => p.Name == "length");
                        if (hasBorrowedText)
                        {
                            sb.Append("std::lock_guard guard(_textMutex);").Append(End)
                                .Append(In).Append(In).AppendFormat("if (!_{0}AsBuffer && _textExpired) throw_hresult(RO_E_CLOSED);", name).Append(End)
                                .Append(In).Append(In);
                        }
                        sb.AppendFormat("if (!_{0}AsBuffer)", name).Append(End)
                            .Append(In).Append(In).Append('{').Append(End)
                            .Append(In).Append(In).Append(In).AppendFormat("if (_{0}AsPointer)", name).Append(End)
                            .Append(In).Append(In).Append(In).Append('{').Append(End)
                            .Append(hasLength ? string.Empty : In + In + In + In + "const auto length { strnlen_s(_textAsPointer, std::numeric_limits<uint32_t>::max()) };" + End)
                            .Append(In).Append(In).Append(In).Append(In).AppendFormat("_{0}AsBuffer = Windows::Storage::Streams::Buffer{{ static_cast<uint32_t>({1}length) }};", name, hasLength ? "_" : string.Empty).Append(End)
                            .Append(In).Append(In).Append(In).Append(In).AppendFormat("memcpy(_{0}AsBuffer.data(), _{0}AsPointer, {1}length);", name, hasLength ? "_" : string.Empty).Append(End)
                            .Append(In).Append(In).Append(In).Append(In).AppendFormat("_{0}AsBuffer.Length({1}length);", name, hasLength ? "_" : string.Empty).Append(End)
                            .Append(In).Append(In).Append(In).Append('}').Append(End)
                            .Append(In).Append(In).Append(In).Append("else").Append(End)
                            .Append(In).Append(In).Append(In).Append('{').Append(End)
                            .Append(In).Append(In).Append(In).Append(In).AppendFormat("_{0}AsBuffer = Windows::Storage::Streams::Buffer{{ 0u }};", name).Append(End)
                            .Append(In).Append(In).Append(In).Append('}').Append(End)
                            .Append(In).Append(In).Append('}').Append(End)
                            .Append(In).Append(In).AppendFormat("return _{0}AsBuffer;", name);
                        break;
                    case "_ForceString":
                        sb.Append("std::lock_guard guard(_textMutex);").Append(End)
                            .Append(In).Append(In).Append("if (!_text && _textExpired) throw_hresult(RO_E_CLOSED);").Append(End)
                            .Append(In).Append(In);
                        sb.Append("if (!_text)").Append(End)
                            .Append(In).Append(In).Append('{').Append(End)
                            .Append(In).Append(In).Append(In).AppendFormat(evt.Params.Any(p => p.Name == "length")
                                ? "_text = _{0}AsPointer ? to_hstring(std::string_view{{ _{0}AsPointer, static_cast<size_t>(_length) }}) : L\"\";"
                                : "_text = _{0}AsPointer ? to_hstring(_{0}AsPointer) : L\"\";", param.Name).Append(End)
                            .Append(In).Append(In).Append('}').Append(End)
                            .Append(In).Append(In).AppendFormat("return _{0}.GetString();", param.Name);
                        break;
                    default:
                        sb.AppendFormat("return _{0};", param.Name);
                        break;
                }

                sb.Append(End).Append(In).Append('}').Append(End).Append(End);
            }
        }

        if (header)
        {
            if (evt.Params.Count != 0)
            {
                sb.Append(End).Append(In).Append("private:").Append(End);
                if (hasBorrowedText)
                {
                    sb.Append(In).Append(In).Append("std::mutex _textMutex;").Append(End)
                        .Append(In).Append(In).Append("bool _textExpired{};").Append(End);
                }

                foreach (var param in evt.Params)
                {
                    sb.Append(In).Append(In).Append(param.Type == "_ForceString" ? "Windows::Foundation::IReference<hstring>" : ConvertToCppType(param.Type, false)).Append(" _").Append(param.Name).Append(';').Append(End);
                    if (param.Type == "_ForceString")
                    {
                        sb.Append(In).Append(In).Append("const char *").Append('_').Append(param.Name).Append("AsPointer").Append(';').Append(End);
                    }
                }
            }
        }

        if (header)
        {
            sb.Append(In).Append("};").Append(End).Append(End);
        }
    }

    private static void WriteIdlEventArgs(StringBuilder sb, Event evt)
    {
        sb.Append(In).Append("[default_interface]").Append(End);

        sb.Append(In).Append("runtimeclass ").Append(evt.Name).Append("EventArgs").Append(End)
            .Append(In).Append('{').Append(End);

        foreach (var param in evt.Params)
        {
            sb.Append(In).Append(In).Append(ConvertToIdlType(param.Type)).Append(' ').Append(ConvertEventParameterToPropertyName(param.Name)).Append(" { get; };").Append(End);
        }

        sb.Append(In).Append('}').Append(End).Append(End);
    }

    private static string ConvertEventParameterToPropertyName(string name)
    {
        return name[..1].ToUpperInvariant() + name[1..];
    }

    private static string ConvertEventNameToFieldName(string name)
    {
        return name[..1].ToLowerInvariant() + name[1..];
    }

    private static void WriteIdlDelegate(StringBuilder sb, Event evt)
    {
        sb.Append(In).Append("delegate ").Append(ConvertToIdlType(evt.RetType)).Append(' ').Append(evt.Name).Append("Handler").Append("(Editor sender, ").Append(evt.Name).Append("EventArgs args);").Append(End);
    }

    private static void WriteCppEventInvoker(StringBuilder sb, Event evt)
    {
        var hasBorrowedText = evt.Params.Any(p => p.Type == "_ForceString");
        sb.Append(In).Append(In).Append("case ").Append(evt.Msg).Append(':').Append(" // ").Append(evt.Name).Append(End)
            .Append(In).Append(In).Append('{').Append(End)
            .Append(In).Append(In).Append(In).AppendFormat("if (_{0}Event)", ConvertEventNameToFieldName(evt.Name)).Append(End)
            .Append(In).Append(In).Append(In).Append('{').Append(End)
            .Append(In).Append(In).Append(In).Append(In);
        if (hasBorrowedText) sb.AppendFormat("auto args = make_self<implementation::{0}EventArgs>(", evt.Name);
        else sb.AppendFormat("_{0}Event(*this, *make_self<implementation::{1}EventArgs>(", ConvertEventNameToFieldName(evt.Name), evt.Name);

        var paramsCount = 0;
        foreach (var param in evt.Params)
        {
            if (param.Type != "string")
            {
                sb.AppendFormat(param.Type == "_ForceString" ? "data->{1}, " : "static_cast<{0}>(data->{1}), ", ConvertToCppType(param.Type, false), param.Name);
                paramsCount++;
            }
        }

        if (paramsCount != 0)
        {
            sb.Length -= ", ".Length;
        }

        if (hasBorrowedText)
        {
            sb.Append(");").Append(End)
                .Append(In).Append(In).Append(In).Append(In).AppendFormat("struct TextLifetime {{ implementation::{0}EventArgs *args; ~TextLifetime() {{ args->InvalidateText(); }} }} lifetime{{ args.get() }};", evt.Name).Append(End)
                .Append(In).Append(In).Append(In).Append(In).AppendFormat("_{0}Event(*this, *args);", ConvertEventNameToFieldName(evt.Name)).Append(End);
        }
        else
        {
            sb.Append("));").Append(End);
        }

        sb
            .Append(In).Append(In).Append(In).Append('}').Append(End)
            .Append(In).Append(In).Append('}').Append(End);

        sb.Append(In).Append(In).Append("break;").Append(End);
    }

    private static void WriteIdlEvent(StringBuilder sb, Event evt)
    {
        sb.Append(In).Append(In).Append("event ").Append(evt.Name).Append("Handler").Append(' ').Append(evt.Name).Append(';').Append(End);
    }

    private static bool NeedsStringVersion(Function fun)
    {
        return fun.Params.Any(p => p.Type == "string" || p.Type == "stringresult");
    }

    private static bool UsesStringReturn(Function fun)
    {
        return fun.Params.Any(p => p.Type == "stringresult");
    }

    private static void WriteIdlFun(StringBuilder sb, Function fun)
    {
        var needsStringVersion = NeedsStringVersion(fun);
        var usesStringReturn = UsesStringReturn(fun);

        WriteComment(sb, fun.Comment, 2);

        sb.Append(In).Append(In).Append(ConvertToIdlType(fun.RetType)).Append(' ').Append(fun.Name).Append(needsStringVersion ? (usesStringReturn ? "Write" : "From") + "Buffer" : string.Empty).Append('(');

        foreach (var param in fun.Params)
        {
            sb.Append(ConvertToIdlType(param.Type)).Append(' ').Append(param.Name).Append(", ");
        }
        if (fun.Params.Count != 0)
        {
            sb.Length -= ", ".Length;
        }

        sb.Append(");").Append(End);

        if (needsStringVersion)
        {
            var retType = usesStringReturn ? "String" : ConvertToIdlType(fun.RetType);

            sb.Append(In).Append(In).Append(retType).Append(' ').Append(fun.Name).Append('(');

            var paramsCount = 0;
            foreach (var param in fun.Params)
            {
                if (param.Type != "stringresult")
                {
                    sb.Append(param.Type == "string" ? "String" : ConvertToIdlType(param.Type)).Append(' ').Append(param.Name).Append(", ");
                    paramsCount++;
                }
            }
            if (paramsCount != 0)
            {
                sb.Length -= ", ".Length;
            }

            sb.Append(");").Append(End).Append(End);
        }
        else
        {
            sb.Append(End);
        }
    }

    private static string ConvertToIdlType(string def)
    {
        return def switch
        {
            "_ForceByteArray" => "UInt8[]",
            "_ForceString" => "String",
            "void" => "void",
            "int" => "Int32",
            "bool" => "Boolean",
            "position" => "Int64",
            "line" => "Int64",
            "colour" => "Int32",
            "colouralpha" => "Int32",
            "string" => "Windows.Storage.Streams.IBuffer",
            "stringresult" => "Windows.Storage.Streams.IBuffer",
            "cells" => "UInt8[]",
            "pointer" => "UInt64",
            "textrange" => "UInt64",
            "textrangefull" => "UInt64",
            "findtext" => "UInt64",
            "findtextfull" => "UInt64",
            "keymod" => "Int32",
            "formatrange" => "UInt64",
            "formatrangefull" => "UInt64",
            _ => def,
        };
    }

    private static string ConvertToCppType(string def, bool param)
    {
        return def switch
        {
            "_ForceString" when param => "hstring const &",
            "_ForceString" => "hstring",
            "void" => "void",
            "int" => "int32_t",
            "bool" => "bool",
            "position" => "int64_t",
            "line" => "int64_t",
            "colour" => "int32_t",
            "colouralpha" => "int32_t",
            "string" when param => "Windows::Storage::Streams::IBuffer const &",
            "string" => "Windows::Storage::Streams::IBuffer",
            "stringresult" => "Windows::Storage::Streams::IBuffer const &",
            "cells" => "array_view<uint8_t const>",
            "pointer" => "uint64_t",
            "textrange" => "uint64_t",
            "textrangefull" => "uint64_t",
            "findtext" => "uint64_t",
            "findtextfull" => "uint64_t",
            "keymod" => "int32_t",
            "formatrange" => "uint64_t",
            "formatrangefull" => "uint64_t",
            _ when param => "WinUIEditor::" + def + " const &",
            _ => "WinUIEditor::" + def,
        };
    }

    private const string In = "\t";
    private const string End = "\r\n";
}
