---
title: Alerts
based_on: https://github.com/jgm/commonmark-hs/blob/master/commonmark-extensions/test/alerts.md
---

## Alerts

GitHub style alerts look like this:

```````````````````````````````` example
> [!NOTE]
> Highlights information that users should take into account, even when skimming.
.
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<p>Highlights information that users should take into account, even when skimming.</p>
</div>
````````````````````````````````

It shouldn't matter if there's a soft break or hard break after
the `[!NOTE]`:

```````````````````````````````` example
> [!NOTE]  
> Highlights information that users should take into account, even when skimming.
.
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<p>Highlights information that users should take into account, even when skimming.</p>
</div>
````````````````````````````````

Uppercase isn't required:

```````````````````````````````` example
> [!note]
> Highlights information that users should take into account, even when skimming.
.
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<p>Highlights information that users should take into account, even when skimming.</p>
</div>
````````````````````````````````


Alerts can contain multiple blocks:

```````````````````````````````` example
> [!NOTE]
> Highlights information that users should take into account, even when skimming.
>
> Paragraph two.
.
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<p>Highlights information that users should take into account, even when skimming.</p>
<p>Paragraph two.</p>
</div>
````````````````````````````````

Other kinds of alerts:

```````````````````````````````` example
> [!TIP]
> Optional information to help a user be more successful.
.
<div class="markdown-alert markdown-alert-tip">
<p class="markdown-alert-title">Tip</p>
<p>Optional information to help a user be more successful.</p>
</div>
````````````````````````````````

```````````````````````````````` example
> [!IMPORTANT]
> Crucial information necessary for users to succeed.
.
<div class="markdown-alert markdown-alert-important">
<p class="markdown-alert-title">Important</p>
<p>Crucial information necessary for users to succeed.</p>
</div>
````````````````````````````````

```````````````````````````````` example
> [!WARNING]
> Critical content demanding immediate user attention due to potential risks.
.
<div class="markdown-alert markdown-alert-warning">
<p class="markdown-alert-title">Warning</p>
<p>Critical content demanding immediate user attention due to potential risks.</p>
</div>
````````````````````````````````

```````````````````````````````` example
> [!CAUTION]
> Negative potential consequences of an action.
.
<div class="markdown-alert markdown-alert-caution">
<p class="markdown-alert-title">Caution</p>
<p>Negative potential consequences of an action.</p>
</div>
````````````````````````````````

A title can be specified to override the default title:

```````````````````````````````` example
> [!NOTE] Pay attention
> Highlights information that users should take into account, even when skimming.
.
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Pay attention</p>
<p>Highlights information that users should take into account, even when skimming.</p>
</div>
````````````````````````````````

The title does not process markdown and is escaped:

```````````````````````````````` example
> [!NOTE] **Pay** attention <script>
> Highlights information that users should take into account, even when skimming.
.
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">**Pay** attention &lt;script&gt;</p>
<p>Highlights information that users should take into account, even when skimming.</p>
</div>
````````````````````````````````

They work in the same places as a normal blockquote would, such as in a list item:

```````````````````````````````` example
- Item one

  > [!NOTE]
  > Highlights information that users should take into account, even when skimming.
.
<ul>
<li>
<p>Item one</p>
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<p>Highlights information that users should take into account, even when skimming.</p>
</div>
</li>
</ul>
````````````````````````````````


