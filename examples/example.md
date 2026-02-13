# Marko Feature Showcase

This document tests every markdown feature for .docx round-trip fidelity.

## Inline Formatting

Here is **bold text**, *italic text*, and ***bold italic*** together. You can also
use __underscores for bold__ and _underscores for italic_

~~Strikethrough text~~ is supported too. And here's `inline code` in a sentence.

==Highlighted text== should stand out. Here's ==multiple highlights== in ==one
line==.

Superscript: x^2^ and subscript: H~2~O.

## Links and Images

[Marko on GitHub](https://github.com/example/marko) is a terminal markdown editor.

Here's a PNG image:

![Ferris the Crab](https://rustacean.net/assets/rustacean-flat-noshadow.png)

## Headings

### Third Level

#### Fourth Level

##### Fifth Level

###### Sixth Level

## Lists

### Unordered Lists

- First item
- Second item
  - Nested item A
  - Nested item B
    - Deeply nested
- Third item

### Ordered Lists

1. Step one
2. Step two
   1. Sub-step A
   2. Sub-step B
3. Step three

### Task Lists

- [x] Write the pandoc module
- [x] Add CLI export subcommand
- [ ] Test all markdown features
- [ ] Ship it

### Mixed Lists

1. First ordered
   - Unordered child
   - Another child
2. Second ordered
   - [x] Completed task
   - [ ] Pending task

## Blockquotes

> This is a simple blockquote.

> Multi-line blockquote.
> It continues here.
>
> And has a paragraph break.

> Nested blockquotes:
>
> > This is nested one level.
> >
> > > And this is two levels deep.

> **Bold in a blockquote** with `inline code` and a [link](https://example.com).

## Code Blocks

Inline: Use `cargo run` to start the app.

Fenced with language:

```rust
fn main() {
    let greeting = "Hello, Marko!";
    println!("{}", greeting);

    for i in 0..5 {
        println!("Count: {}", i);
    }
}
```

```python
def fibonacci(n):
    """Generate fibonacci sequence."""
    a, b = 0, 1
    for _ in range(n):
        yield a
        a, b = b, a + b

print(list(fibonacci(10)))
```

```javascript
const marko = {
  name: "Marko",
  type: "editor",
  features: ["markdown", "preview", "docx"],
};

console.log(JSON.stringify(marko, null, 2));
```

```bash
# Build and test
cargo build --release
cargo test
cargo run -- export EXAMPLE.md -o example.docx
```

Plain code block (no language):

```
This is a plain code block.
No syntax highlighting here.
```

## Tables

### Simple Table

| Feature         | Status               | Notes                                          |
| --------------- | -------------------- | ---------------------------------------------- |
| Bold            | Supported            | Works well                                     |
| Italic          | Supported            | Works well                                     |
| Tables          | Supported            | You're looking at one                          |
| Images          | Partial              | Text-only editor                               |

### Alignment Table

| Left                           | Center                     | Right                     |
| ------------------------------ | -------------------------- | ------------------------- |
| Alice                          | 100                        | $1,200                    |
| Bob                            | 85                         | $950                      |
| Charlie                        | 92                         | $1,100                    |

### Wide Table

| Name         | Email              | Role     | Departmen  | Location     | Start Date   |
| ------------ | ------------------ | -------- | ---------- | ------------ | ------------ |
| Alice Johns  | alice@example.co   | Enginee  | Platform   | San Francis  | 2023-01-15   |
| Bob Smith    | bob@example.com    | Designe  | Product    | New York     | 2022-06-01   |
| Charlie Bro  | charlie@example.   | Manager  | Engineeri  | London       | 2021-03-20   |

## Horizontal Rules

Above the rule.

---

Between two rules.

***

Below the rules.

## Footnotes

Here is a sentence with a footnote[^1].

And another one[^note].

[^1]: This is the first footnote.
[^note]: This is a named footnote with more detail.

## Definition Lists

Term 1
: Definition for term 1.

Term 2
: First definition for term 2.
: Second definition for term 2.

## Math (if supported)

Inline math: $E = mc^2$

Block math:

$$
\int_{-\infty}^{\infty} e^{-x^2} dx = \sqrt{\pi}
$$

## Escapes and Special Characters

Backslash escapes: \*not italic\*, \[not a link\], \#not a heading.

Special characters: &amp; &lt; &gt; &copy; &mdash; &ndash;

Unicode: arrows → ← ↑ ↓, bullets • ◦ ▪, emoji 🦀 📝 ✅

## Paragraph Styles

This is a normal paragraph with enough text to potentially trigger word wrapping in
a narrow
terminal. The quick brown fox jumps over the lazy dog. Pack my box with five dozen
liquor jugs.

Short paragraph.

Another paragraph with **mixed** *formatting* and `code` and
[links](https://example.com) all
on ~~one line~~ to stress-test inline rendering.

## Summary

This file exercises headings, inline formatting (bold, italic, strikethrough,
highlight,
code), links, images, all list types (unordered, ordered, task, nested),
blockquotes (simple,
nested, formatted), code blocks (multiple languages, plain), tables (simple,
aligned, wide),
horizontal rules, footnotes, definition lists, math, special characters, and
paragraph
wrapping.

Use it to test round-trip fidelity:

```bash
cargo run -- export EXAMPLE.md -o examples/example.docx
open examples/example.docx
cargo run -- examples/example.docx
```