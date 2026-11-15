// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.

using Microsoft.VisualStudio.TestTools.UnitTesting;
using Microsoft.Toolkit.Parsers.Markdown;
using Microsoft.Toolkit.Parsers.Markdown.Blocks;
using Microsoft.Toolkit.Parsers.Markdown.Inlines;

namespace UnitTests.Markdown.Parse
{
    [TestClass]
    public class HyperlinkTests : ParseTestBase
    {
#pragma warning disable CS0618 // Type or member is obsolete
        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_Http()
        {
            AssertEqual("http://reddit.com",
                new ParagraphBlock().AddChildren(
                    new HyperlinkInline { Url = "http://reddit.com", Text = "http://reddit.com", LinkType = HyperlinkType.FullUrl }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_WithSurroundingText()
        {
            AssertEqual("Narwhal http://reddit.com fail whale",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "Narwhal " },
                    new HyperlinkInline { Url = "http://reddit.com", Text = "http://reddit.com", LinkType = HyperlinkType.FullUrl },
                    new TextRunInline { Text = " fail whale" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_Http_Uppercase()
        {
            AssertEqual("HTTP://reddit.com",
                new ParagraphBlock().AddChildren(
                    new HyperlinkInline { Url = "HTTP://reddit.com", Text = "HTTP://reddit.com", LinkType = HyperlinkType.FullUrl }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_Http_Inline()
        {
            AssertEqual("The best site (http://reddit.com) goes well with http://www.wikipedia.com, don't you think?",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "The best site (" },
                    new HyperlinkInline { Url = "http://reddit.com", Text = "http://reddit.com", LinkType = HyperlinkType.FullUrl },
                    new TextRunInline { Text = ") goes well with " },
                    new HyperlinkInline { Url = "http://www.wikipedia.com", Text = "http://www.wikipedia.com", LinkType = HyperlinkType.FullUrl },
                    new TextRunInline { Text = ", don't you think?" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_Https()
        {
            AssertEqual("https://reddit.com",
                new ParagraphBlock().AddChildren(
                    new HyperlinkInline { Url = "https://reddit.com", Text = "https://reddit.com", LinkType = HyperlinkType.FullUrl }));
        }


        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_DomainOnly()
        {
            AssertEqual("www.reddit.com",
                new ParagraphBlock().AddChildren(
                    new HyperlinkInline { Url = "http://www.reddit.com", Text = "www.reddit.com", LinkType = HyperlinkType.PartialUrl }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_Mailto()
        {
            AssertEqual("bob@bob.com",
                new ParagraphBlock().AddChildren(
                    new HyperlinkInline { Url = "mailto:bob@bob.com", Text = "bob@bob.com", LinkType = HyperlinkType.Email }));
        }

        //// See E-mail Spec Here: https://tools.ietf.org/html/rfc3696#section-3
        [TestCategory("Parse - inline")]
        [DataTestMethod]
        [DataRow("abcABC123@test.com")]
        [DataRow("a!b@test.com")]
        [DataRow("a#b@test.com")]
        [DataRow("a$b@test.com")]
        [DataRow("a&b@test.com")]
        [DataRow("a'b@test.com")]
        [DataRow("a*b@test.com")]
        [DataRow("a+b@test.com")]
        [DataRow("a-b@test.com")]
        [DataRow("a/b@test.com")]
        [DataRow("a=b@test.com")]
        [DataRow("a?b@test.com")]
        ////[DataRow("a^b@test.com")] // TODO: Issue in Common.FindNextInlineElement, see Issue #3200
        [DataRow("a_b@test.com")]
        [DataRow("a`b@test.com")]
        [DataRow("a{b@test.com")]
        [DataRow("a|b@test.com")]
        [DataRow("a}b@test.com")]
        [DataRow("a~b@test.com")]
        ////[DataRow("a.b@test.com")] // TODO: assuming an issue similar to ^, see Issue #3200
        ////[DataRow("\"a..b\"@test.com")] // TODO: 'Hard' case, see Issue #3200
        public void Hyperlink_MailtoLocalPartValid(string email)
        {
            AssertEqual(email, new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "mailto:" + email, Text = email, LinkType = HyperlinkType.Email }));
        }

        [TestCategory("Parse - inline")]
        [TestMethod]
        public void Hyperlink_MailtoLocalPartInvalid()
        {
            AssertEqual("a..b@test.com", new ParagraphBlock().AddChildren(new TextRunInline { Text = "a.." }, new HyperlinkInline { Url = "mailto:b@test.com", Text = "b@test.com", LinkType = HyperlinkType.Email }));

            AssertEqual(".ab@test.com", new ParagraphBlock().AddChildren(new TextRunInline { Text = "." }, new HyperlinkInline { Url = "mailto:ab@test.com", Text = "ab@test.com", LinkType = HyperlinkType.Email }));

            AssertEqual("ab.@test.com", new ParagraphBlock().AddChildren(new TextRunInline { Text = "ab.@test.com" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_MailtoHostPart()
        {
            AssertEqual(CollapseWhitespace(@"
                abc@abcABC123.com

                abc@a!b.com

                abc@a#b.com

                abc@a$b.com

                abc@a%b.com

                abc@a&b.com

                abc@a*b.com

                abc@a+b.com

                abc@a!b.com

                abc@a-b.com

                abc@a=b.com

                abc@a/b.com

                abc@a?b.com

                abc@a^b.com

                abc@a_b.com

                abc@a{b.com

                abc@a}b.com

                abc@a|b.com

                abc@a!b.com

                abc@a`b.com

                abc@a'b.com

                abc@a~b.com

                abc@a.b.com

                abc@a..t.com

                abc@ab..com

                abc@.ab.com"),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "mailto:abc@abcABC123.com", Text = "abc@abcABC123.com", LinkType = HyperlinkType.Email }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a!b.com" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a#b.com" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a$b.com" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a%b.com" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a&b.com" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a*b.com" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a+b.com" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a!b.com" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "mailto:abc@a-b.com", Text = "abc@a-b.com", LinkType = HyperlinkType.Email }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a=b.com" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a/b.com" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a?b.com" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a" }, new SuperscriptTextInline().AddChildren(new TextRunInline { Text = "b.com" })),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "mailto:abc@a_b.com", Text = "abc@a_b.com", LinkType = HyperlinkType.Email }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a{b.com" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a}b.com" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a|b.com" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a!b.com" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a`b.com" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a'b.com" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "abc@a~b.com" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "mailto:abc@a.b.com", Text = "abc@a.b.com", LinkType = HyperlinkType.Email }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "mailto:abc@a..t.com", Text = "abc@a..t.com", LinkType = HyperlinkType.Email }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "mailto:abc@ab..com", Text = "abc@ab..com", LinkType = HyperlinkType.Email }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "mailto:abc@.ab.com", Text = "abc@.ab.com", LinkType = HyperlinkType.Email }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_MailtoWithBold()
        {
            AssertEqual("abc__def@test.co__m",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "abc" },
                    new BoldTextInline().AddChildren(
                        new HyperlinkInline { Url = "mailto:def@test.co", Text = "def@test.co", LinkType = HyperlinkType.Email }),
                    new TextRunInline { Text = "m" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_AngleBrackets()
        {
            AssertEqual("<http://reddit.com>",
                new ParagraphBlock().AddChildren(
                    new HyperlinkInline { Url = "http://reddit.com", Text = "http://reddit.com", LinkType = HyperlinkType.BracketedUrl }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_AngleBracketsNoNeedForDot()
        {
            AssertEqual("<http://reddit>",
                new ParagraphBlock().AddChildren(
                    new HyperlinkInline { Url = "http://reddit", Text = "http://reddit", LinkType = HyperlinkType.BracketedUrl }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_AngleBracketsCanEndWithPunctuation()
        {
            AssertEqual("<http://reddit.com.>",
                new ParagraphBlock().AddChildren(
                    new HyperlinkInline { Url = "http://reddit.com.", Text = "http://reddit.com.", LinkType = HyperlinkType.BracketedUrl }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_AngleBracketsCantHaveSpaces()
        {
            AssertEqual("< http://reddit.com >",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "< " },
                    new HyperlinkInline { Url = "http://reddit.com", Text = "http://reddit.com", LinkType = HyperlinkType.FullUrl },
                    new TextRunInline { Text = " >" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_StartCharacters()
        {
            AssertEqual("0http://reddit.com",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "0" },
                    new HyperlinkInline { Url = "http://reddit.com", Text = "http://reddit.com", LinkType = HyperlinkType.FullUrl }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_EndCharacters()
        {
            AssertEqual(CollapseWhitespace(@"
                http://reddit.com)

                http://reddit.com).

                http://reddit.com)a

                http://reddit.com}

                http://reddit.com}a

                http://reddit.com]

                http://reddit.com]a

                http://reddit.com>

                http://reddit.com|

                http://reddit.com`

                http://reddit.com^

                http://reddit.com~

                http://reddit.com[

                http://reddit.com(

                http://reddit.com{

                http://reddit.com<

                http://reddit.com<a

                http://reddit.com#

                http://reddit.com%

                http://reddit.com!

                http://reddit.com!a

                http://reddit.com;

                http://reddit.com;a

                http://reddit.com.

                http://reddit.com.a

                http://reddit.com-

                http://reddit.com+

                http://reddit.com=

                http://reddit.com_

                http://reddit.com*

                http://reddit.com&

                http://reddit.com?

                http://reddit.com?a

                http://reddit.com,

                http://reddit.com,a"),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com", Text = "http://reddit.com", LinkType = HyperlinkType.FullUrl }, new TextRunInline { Text = ")" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com", Text = "http://reddit.com", LinkType = HyperlinkType.FullUrl }, new TextRunInline { Text = ")." }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com)a", Text = "http://reddit.com)a", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com", Text = "http://reddit.com", LinkType = HyperlinkType.FullUrl }, new TextRunInline { Text = "}" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com}a", Text = "http://reddit.com}a", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com", Text = "http://reddit.com", LinkType = HyperlinkType.FullUrl }, new TextRunInline { Text = "]" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com]a", Text = "http://reddit.com]a", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com>", Text = "http://reddit.com>", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com|", Text = "http://reddit.com|", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com`", Text = "http://reddit.com`", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com^", Text = "http://reddit.com^", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com~", Text = "http://reddit.com~", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com[", Text = "http://reddit.com[", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com(", Text = "http://reddit.com(", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com{", Text = "http://reddit.com{", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com", Text = "http://reddit.com", LinkType = HyperlinkType.FullUrl }, new TextRunInline { Text = "<" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com", Text = "http://reddit.com", LinkType = HyperlinkType.FullUrl }, new TextRunInline { Text = "<a" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com#", Text = "http://reddit.com#", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com%", Text = "http://reddit.com%", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com", Text = "http://reddit.com", LinkType = HyperlinkType.FullUrl }, new TextRunInline { Text = "!" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com!a", Text = "http://reddit.com!a", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com", Text = "http://reddit.com", LinkType = HyperlinkType.FullUrl }, new TextRunInline { Text = ";" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com;a", Text = "http://reddit.com;a", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com", Text = "http://reddit.com", LinkType = HyperlinkType.FullUrl }, new TextRunInline { Text = "." }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com.a", Text = "http://reddit.com.a", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com-", Text = "http://reddit.com-", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com+", Text = "http://reddit.com+", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com=", Text = "http://reddit.com=", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com_", Text = "http://reddit.com_", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com*", Text = "http://reddit.com*", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com&", Text = "http://reddit.com&", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com", Text = "http://reddit.com", LinkType = HyperlinkType.FullUrl }, new TextRunInline { Text = "?" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com?a", Text = "http://reddit.com?a", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com", Text = "http://reddit.com", LinkType = HyperlinkType.FullUrl }, new TextRunInline { Text = "," }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://reddit.com,a", Text = "http://reddit.com,a", LinkType = HyperlinkType.FullUrl }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_OtherSchemes()
        {
            AssertEqual(CollapseWhitespace(@"
                http://test.com

                https://test.com

                ftp://test.com

                steam://test.com

                irc://test.com

                news://test.com

                mumble://test.com

                ssh://test.com"),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "http://test.com", Text = "http://test.com", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "https://test.com", Text = "https://test.com", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "ftp://test.com", Text = "ftp://test.com", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "steam://test.com", Text = "steam://test.com", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "irc://test.com", Text = "irc://test.com", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "news://test.com", Text = "news://test.com", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "mumble://test.com", Text = "mumble://test.com", LinkType = HyperlinkType.FullUrl }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Url = "ssh://test.com", Text = "ssh://test.com", LinkType = HyperlinkType.FullUrl }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_Negative_SurroundingText()
        {
            AssertEqual("thttp://reddit.com",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "thttp://reddit.com" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_Negative_SchemeOnly()
        {
            AssertEqual("http",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "http" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_Negative_PrefixOnly()
        {
            AssertEqual("http://",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "http://" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_Negative_NoDot()
        {
            AssertEqual("http://localhost",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "http://localhost" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_Negative_DotTooSoon()
        {
            AssertEqual("http://.com",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "http://.com" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_Negative_AngleBracketsPrefixOnly()
        {
            AssertEqual("<http>",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "<http>" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_Negative_MailtoNeedsADot()
        {
            AssertEqual("bob@bob",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "bob@bob" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_Negative_AngleBracketDomainOnly()
        {
            AssertEqual("<www.reddit.com>",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "<www.reddit.com>" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void Hyperlink_Negative_WwwMustBeLowercase()
        {
            AssertEqual("WWW.reddit.com",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "WWW.reddit.com" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void SubredditLink_WithSlash()
        {
            AssertEqual("/r/subreddit",
                new ParagraphBlock().AddChildren(
                    new HyperlinkInline { Text = "/r/subreddit", Url = "/r/subreddit", LinkType = HyperlinkType.Subreddit }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void SubredditLink_WithoutSlash()
        {
            AssertEqual("r/subreddit",
                new ParagraphBlock().AddChildren(
                    new HyperlinkInline { Text = "r/subreddit", Url = "/r/subreddit", LinkType = HyperlinkType.Subreddit }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void SubredditLink_Short()
        {
            // Subreddit names can be min two chars long.
            AssertEqual("/r/ab",
                new ParagraphBlock().AddChildren(
                    new HyperlinkInline { Text = "/r/ab", Url = "/r/ab", LinkType = HyperlinkType.Subreddit }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void SubredditLink_WithBeginningEscape()
        {
            AssertEqual(@"\/r/subreddit",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "/r/subreddit" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void SubredditLink_WithMiddleEscape()
        {
            AssertEqual(@"r\/subreddit",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "r/subreddit" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void SubredditLink_EndCharacters()
        {
            AssertEqual(CollapseWhitespace(@"
                /r/news)

                /r/news).

                /r/news)a

                /r/news}

                /r/news}a

                /r/news]

                /r/news]a

                /r/news>

                /r/news|

                /r/news`

                /r/news^

                /r/news~

                /r/news[

                /r/news(

                /r/news{

                /r/news<

                /r/news<a

                /r/news#

                /r/news%

                /r/news!

                /r/news!a

                /r/news;

                /r/news;a

                /r/news.

                /r/news.a

                /r/news-

                /r/news=

                /r/news_

                /r/news*

                /r/news&

                /r/news?

                /r/news?a

                /r/news,

                /r/news,a

                /r/news0"),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = ")" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = ")." }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = ")a" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "}" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "}a" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "]" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "]a" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = ">" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "|" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "`" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "^" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "~" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "[" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "(" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "{" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "<" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "<a" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "#" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "%" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "!" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "!a" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = ";" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = ";a" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "." }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = ".a" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "-" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "=" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news_", Url = "/r/news_", LinkType = HyperlinkType.Subreddit }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "*" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "&" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "?" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "?a" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = "," }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news", Url = "/r/news", LinkType = HyperlinkType.Subreddit }, new TextRunInline { Text = ",a" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/news0", Url = "/r/news0", LinkType = HyperlinkType.Subreddit }));
        }

        // This test is ignored because it is written to pass the "reddit" quirks of markdown.
        // This parser doesn't conform to the reddit quirks, thus they shall not pass.
        [Ignore]
        [TestMethod]
        [TestCategory("Parse - inline")]
        public void SubredditLink_PlusCharacter()
        {
            // The plus character is treated strangely.
            AssertEqual(CollapseWhitespace(@"
                /r/+

                /r/+a

                /r/+ab

                /r/a+b

                /r/a+bc

                /r/ab+c

                /r/ab+cd

                /r/a+

                /r/ab+"),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "/r/+" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "/r/+a" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "/r/+ab" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "/r/a+b" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "/r/a+bc" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "/r/ab+c" }),
                new ParagraphBlock().AddChildren(new HyperlinkInline { Text = "/r/ab+cd", Url = "/r/ab+cd", LinkType = HyperlinkType.Subreddit }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "/r/a+" }),
                new ParagraphBlock().AddChildren(new TextRunInline { Text = "/r/ab+" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void SubredditLink_WithPath()
        {
            AssertEqual("/r/news/blah",
                new ParagraphBlock().AddChildren(
                    new HyperlinkInline { Text = "/r/news/blah", Url = "/r/news/blah", LinkType = HyperlinkType.Subreddit }));
        }



        [TestMethod]
        [TestCategory("Parse - inline")]
        public void SubredditLink_Negative_SurroundingText()
        {
            AssertEqual("bear/subreddit",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "bear/subreddit" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void SubredditLink_Negative_PrefixOnly()
        {
            AssertEqual("r/",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "r/" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void SubredditLink_Negative_UppercaseWithoutSlash()
        {
            AssertEqual("R/baconit",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "R/baconit" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void SubredditLink_Negative_UppercaseWithSlash()
        {
            AssertEqual("/R/baconit",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "/R/baconit" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void SubredditLink_Negative_TooShort()
        {
            // The subreddit name must be at least 2 chars.
            AssertEqual("r/a",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "r/a" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void UserLink_WithSlash()
        {
            AssertEqual("/u/quinbd",
                new ParagraphBlock().AddChildren(
                    new HyperlinkInline { Text = "/u/quinbd", Url = "/u/quinbd", LinkType = HyperlinkType.User }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void UserLink_WithoutSlash()
        {
            AssertEqual("u/quinbd",
                new ParagraphBlock().AddChildren(
                    new HyperlinkInline { Text = "u/quinbd", Url = "/u/quinbd", LinkType = HyperlinkType.User }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void UserLink_Short()
        {
            // User names can be one char long.
            AssertEqual("/u/u",
                new ParagraphBlock().AddChildren(
                    new HyperlinkInline { Text = "/u/u", Url = "/u/u", LinkType = HyperlinkType.User }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void UserLink_WithPath()
        {
            AssertEqual("/u/quinbd/blah",
                new ParagraphBlock().AddChildren(
                    new HyperlinkInline { Text = "/u/quinbd/blah", Url = "/u/quinbd/blah", LinkType = HyperlinkType.User }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void UserLink_Negative_PrefixOnly()
        {
            AssertEqual("u/",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "u/" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void UserLink_Negative_UppercaseWithoutSlash()
        {
            AssertEqual("U/quinbd",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "U/quinbd" }));
        }

        [TestMethod]
        [TestCategory("Parse - inline")]
        public void UserLink_Negative_UppercaseWithSlash()
        {
            AssertEqual("/U/quinbd",
                new ParagraphBlock().AddChildren(
                    new TextRunInline { Text = "/U/quinbd" }));
        }
#pragma warning restore CS0618 // Type or member is obsolete
    }
}