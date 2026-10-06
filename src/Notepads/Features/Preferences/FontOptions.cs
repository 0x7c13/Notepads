// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Collections.Generic;
using System.Linq;
using Notepads.Infrastructure.Fonts;

namespace Notepads.Features.Preferences;

public static class FontOptions
{
    /// <summary>
    /// The collection of symbol fonts that need to be skipped from the available fonts, as they don't produce readable text
    /// </summary>
    private static readonly IReadOnlyCollection<string> SymbolFonts = new HashSet<string>(new[]
    {
        "Segoe MDL2 Assets",
        "Webdings",
        "Wingdings",
        "HoloLens MDL2 Assets",
        "Bookshelf Symbol 7",
        "MT Extra",
        "MS Outlook",
        "MS Reference Specialty",
        "Wingdings 2",
        "Wingdings 3",
        "Marlett"
    });

    /// <summary>
    /// The fallback collection of fonts available in all Windows 10 versions in case GetSystemFontFamilies API failed
    /// https://docs.microsoft.com/en-us/typography/fonts/windows_10_font_list
    /// </summary>
    private static readonly IReadOnlyCollection<string> DefaultFonts = new HashSet<string>(new[]
    {
        "Arial",
        "Arial Black",
        "Calibri",
        "Cambria",
        "Cambria Math",
        "Comic Sans MS",
        "Consolas",
        "Constantia",
        "Courier New",
        "Ebrima",
        "Franklin Gothic Medium",
        "Gabriola",
        "Gadugi",
        "Georgia",
        "Impact",
        "Javanese Text",
        "Leelawadee UI",
        "Lucida Console",
        "Lucida Sans Unicode",
        "Malgun Gothic",
        "Marlett",
        "Microsoft Himalaya",
        "Microsoft JhengHei",
        "Microsoft New Tai Lue",
        "Microsoft PhagsPa",
        "Microsoft Sans Serif",
        "Microsoft Tai Le",
        "Microsoft YaHei",
        "Microsoft Yi Baiti",
        "MingLiU-ExtB",
        "Mongolian Baiti",
        "MS Gothic",
        "MV Boli",
        "Myanmar Text",
        "Nirmala UI",
        "Palatino Linotype",
        "Segoe Print",
        "Segoe Script",
        "Segoe UI",
        "SimSun",
        "Sitka",
        "Sylfaen",
        "Tahoma",
        "Times New Roman",
        "Trebuchet MS",
        "Verdana",
        "Yu Gothic"
    });

    public static readonly int[] PredefinedFontSizes =
    [
        8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 20, 22, 24, 26, 28, 36, 48, 72
    ];

    public static readonly Dictionary<string, FontSlant> PredefinedFontStylesMap = new()
    {
        {nameof(FontSlant.Normal),  FontSlant.Normal},
        {nameof(FontSlant.Italic),  FontSlant.Italic},
        {nameof(FontSlant.Oblique), FontSlant.Oblique}
    };

    public static readonly Dictionary<string, ushort> PredefinedFontWeightsMap = new()
    {
        {"Normal",     400},
        {"Thin",       100},
        {"ExtraLight", 200},
        {"Light",      300},
        {"SemiLight",  350},
        {"Medium",     500},
        {"SemiBold",   600},
        {"Bold",       700},
        {"ExtraBold",  800},
        {"Black",      900},
        {"ExtraBlack", 950}
    };

    public static string[] GetSystemFontFamilies()
    {
        var fonts = SystemFontCatalog.TryGetFamilyNames(out var systemFonts) ? systemFonts : DefaultFonts;
        return fonts.Where(font => !SymbolFonts.Contains(font)).OrderBy(font => font).ToArray();
    }
}
