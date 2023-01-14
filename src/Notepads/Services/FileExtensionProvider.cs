namespace Notepads.Services
{
    using Microsoft.Toolkit.Uwp.Helpers;
    using System.Collections.Generic;

    public static class FileExtensionProvider
    {
        public static ISet<string> TextDocumentFileExtensions { get; } = new HashSet<string>()
        {
            ".txt", ".md", ".markdown"
        };

        public static ISet<string> AllSupportedFileExtensions { get; } = new HashSet<string>()
        {
            //Aegisub Advanced Subtitle
            ".ass",
            //Assembly Language
            ".asm",
            //ASP
            ".asp", ".aspx",
            //AutoHotkey Script
            ".ahk",
            //Bash
            ".bash", ".bash_aliases", ".bash_history", ".bash_login", ".bash_logout", ".bash_profile",
            //BibTeX
            ".bib",
            //Bond
            ".bond",
            //Build Path
            ".buildpath",
            //C
            ".c", ".m", ".i",
            //CGI
            ".cgi",
            //Cheat
            ".cht",
            //Clojure
            ".clj", ".cljs", ".cljc", ".cljx", ".clojure", ".edn",
            //Cmake
            ".cmake",
            //CoffeeScript
            ".coffee", ".cson", ".iced",
            //Comma Separated Values
            ".csv",
            //Configuration
            ".cfg", ".config", ".cnf", ".conf", ".properties", ".directory",
            //C++
            ".cpp", ".cc", ".mm", ".cxx", ".ii", ".ino",
            //C Sharp Project
            ".csproj",
            //C#
            ".cs", ".csx", ".cake",
            //CSS
            ".css", ".scss",
            //DAT
            ".dat",
            //Diff
            ".patch", ".diff", ".rej",
            //Docker
            ".dockerfile",
            //EBuild
            ".ebuild",
            //ENV
            ".env",
            //F#
            ".fs", ".fsi", ".fsx", ".fsscript",
            //Git
            ".gitignore", ".gitattributes", ".gitconfig",
            //GLSLP
            ".glslp",
            //GLSP
            ".glsp",
            //Go
            ".go",
            //Groovy
            ".groovy", ".gvy", ".gradle",
            //Handlebars
            ".handlebars", ".hbs", ".hjs",
            //Header
            ".h", ".hpp", ".hh", ".hxx",
            //HLSL
            ".hlsl", ".hlsli", ".fx", ".fxh", ".vsh", ".psh", ".cginc", ".compute",
            //HTML
            ".html", ".htm", ".shtml", ".xhtml", ".mdoc", ".jshtm", ".volt",
            //Hypertext Access
            ".htaccess",
            //Initialization
            ".ini",
            //Install
            ".install",
            //Java
            ".java", ".jav",
            //Javascript
            ".js", ".jsx", ".es6", ".mjs", ".cjs", ".pac",
            //JSON
            ".json", ".hintrc", ".jsonc", ".jsonld", ".babelrc", ".eslintrc", ".jslintrc", ".bowerrc", ".jscsrc", ".webmanifest", ".har",
            //JSP
            ".jsp", ".jspx",
            //Less
            ".less",
            //License
            ".lic",
            //Log
            ".log",
            //Lyrics
            ".lrc",
            //LUA
            ".lua",
            //M3U
            ".m3u", ".m3u8",
            //Map
            ".map",
            //Markdown
            ".md", ".markdown", ".mkd", ".mdwn", ".mdown", ".markn", ".mdtxt",
            //NFO
            ".nfo",
            //NPM Config
            ".npmrc",
            //OPT
            ".opt",
            //Perl 6
            ".p6", ".pl6", ".pm6", ".nqp",
            //Perl
            ".pl", ".pm", ".psgi",
            //PHP
            ".php", ".php4", ".php5", ".phtml", ".ctp",
            //Pod
            ".pod", ".podspec",
            //PowerShell
            ".ps1", ".psm1", ".psd1", ".pssc", ".psrc",
            //Profile
            ".profile",
            //Project
            ".project", ".prj",
            //Property List
            ".plist",
            //Pug
            ".jade", ".pug",
            //PVD
            ".pvd",
            //Python
            ".py",
            //Razor
            ".cshtml",
            //Resource
            ".resx",
            //R
            ".r", ".rhistory", ".rprofile", ".rt",
            //Ruby
            ".rb", ".rbx", ".rjs", ".gemspec", ".rake", ".ru", ".erb", ".rbi", ".arb",
            //Run Commands
            ".bashrc", ".vimrc", ".rc",
            //Rust
            ".rs",
            //Shader
            ".shader",
            //Shell Script
            ".sh",
            //SQL
            ".sql", ".dsql",
            //Sub Station Alpha Subtitle
            ".ssa",
            //Subtitle
            ".srt",
            //Swift
            ".swift",
            //Text Document
            ".txt",
            //T
            ".t",
            //TOML
            ".toml",
            //TypeScript
            ".ts", ".tsx",
            //User
            ".user",
            //Verilog
            ".v",
            //Visual Basic
            ".vb", ".vbs", ".brs", ".bas",
            //Vue Config
            ".vuerc",
            //Vue
            ".vue",
            //XAML
            ".xaml",
            //XML
            ".xml", ".xsd", ".ascx", ".atom", ".axml", ".bpmn", ".cpt", ".csl",
            //XSL
            ".xsl",
            //YAML
            ".yml", ".yaml",
            //Z shell Config
            ".zshrc",
            //Z shell History
            ".zsh_history",
        };

        public static bool IsFileExtensionSupported(string fileExtension)
        {
            // Windows 10 2004 (build 19041) enables support for handling any kind of file
            // https://github.com/microsoft/ProjectReunion/issues/27
            if (SystemInformation.Instance.OperatingSystemVersion.Build >= 19041)
            {
                return true;
            }

            if (string.IsNullOrEmpty(fileExtension))
            {
                return false;
            }

            if (!fileExtension.StartsWith("."))
            {
                fileExtension = "." + fileExtension;
            }

            return AllSupportedFileExtensions.Contains(fileExtension.ToLower());
        }
    }
}