# layout — Box Model for getBoundingClientRect

Provides layout computation so JS APIs like `getBoundingClientRect()`, `offsetWidth`, `offsetHeight` return meaningful values.

## Why We Need Layout (Without Rendering)

Many websites and anti-bot systems call layout APIs:

```javascript
// SPA frameworks (React, Vue) check element dimensions
const rect = element.getBoundingClientRect();
if (rect.width === 0) { /* element not visible, skip */ }

// Anti-bot checks (visibility verification)
const nav = document.querySelector('nav');
if (nav.offsetHeight === 0) { /* suspicious — bot doesn't have layout */ }

// Lazy loading
if (entry.isIntersecting) { loadImage(); }
```

If these return `0` or `undefined`, sites break or flag us as a bot.

## Core: taffy

| Property | Value |
|---|---|
| Crate | `taffy` |
| License | MIT |
| Algorithms | CSS Block, Flexbox, Grid |
| Used by | Dioxus, Zed editor, Bevy UI |

taffy takes a tree of nodes with `Style` structs and computes `Layout` (position + size) for each node.

## Style resolution

Layout does not resolve styles itself. `crate::style` does, once per `compute`,
and layout reads the result:

```
DOM ──┬─► style::compute_styles ──► StyleTree (ComputedStyle per element)
      │        ▲   ▲   ▲                        │
      │        │   │   └── style="…" attributes │
      │        │   └────── <style> blocks       │
      │        └────────── style/ua.css         ▼
      └──────────────────────────────► LayoutEngine::compute ──► taffy tree
```

`style/ua.css` is the user-agent stylesheet, compiled in with `include_str!`.
It is small on purpose — it covers what is *observable through geometry*: which
elements do not render (`head`, `script`, `style`, `title`, …), which are
block-level, and the default margins Chrome applies. Without it, `head` and
`title` are laid out as visible blocks and `body` starts several tens of pixels
down the page.

Cascade order is user-agent → author → `style` attribute, with the attribute
given an unreachable specificity because `css_cascade` has no separate tier
for it.

> **History.** Until this was written, `LayoutEngine` resolved each element from
> an *empty* cascaded map plus its `style` attribute — so `<style>` blocks and
> stylesheets had no effect on geometry at all, and `font-size` did not inherit
> (every `em` resolved against a fixed 16px). `getComputedStyle` had its own
> separate path in `js_runtime::state`, which is why the gap was easy to miss:
> the *reported* style was right while the *laid out* style was not.

## Borders

The used width of a border is zero unless `border-style` says it draws. This
matters more than it sounds: `border-width`'s initial value is `medium` (3px)
and `border-style`'s is `none`, so applying width unconditionally gives every
element on every page a 3px border — which is what happened before
`border-style` existed as a property.

## Text measurement

Text nodes go through CSS white-space processing before they are measured, so
the newline-and-indent between two tags collapses to nothing and produces no
box. Without it every such text node became a full line box and an ordinary
document gained one per element.

Measurement itself is still the placeholder: `char_count × font_size × 0.6`,
one line, no wrapping. That is what the renderer replaces — see
[`browser_oxide_app/docs/POC1_RESULTS.md`](https://github.com/yfedoseev/browser_oxide_app)
for the shaped, Chrome-parity implementation that is being lifted in.

## Architecture

```
layout/
├── src/
│   ├── lib.rs              # LayoutEngine — compute + query
│   ├── engine.rs           # DOM → taffy tree conversion
│   ├── style_map.rs        # CSS computed styles → taffy::Style
│   ├── viewport.rs         # Virtual viewport (1920x1080 default)
│   ├── fonts.rs            # Font metrics (character widths for text sizing)
│   └── query.rs            # getBoundingClientRect, offset*, client*, scroll*
├── tests/
│   ├── basic_layout.rs
│   ├── flexbox.rs
│   └── bounding_rect.rs
└── Cargo.toml
```

## How It Works

```
DOM tree + computed styles
        │
        ▼
  ┌─────────────┐
  │ Convert DOM  │  Map each DOM element to a taffy node with
  │ → taffy tree │  Style { display, width, height, padding, margin, ... }
  └──────┬──────┘
         │
         ▼
  ┌─────────────┐
  │ taffy layout │  Compute position (x, y) and size (w, h) for every node
  │  algorithm   │
  └──────┬──────┘
         │
         ▼
  ┌─────────────┐
  │ Layout cache │  Store results, invalidate on DOM mutation
  └─────────────┘
```

## Font Metrics

To compute text layout, taffy needs to know how wide text is. We need basic font metrics without full font rendering:

| Crate | License | Purpose |
|---|---|---|
| `fontdb` | MIT | System font database (find fonts by family name) |
| `rustybuzz` | MIT | Text shaping (compute glyph advances/widths) |
| `ttf-parser` | MIT/Apache-2.0 | Parse TrueType/OpenType font files |

We load system fonts (or bundle a default), measure text widths, and feed them to taffy's `MeasureFunc`.

## JS API Mapping

| JS API | Implementation |
|---|---|
| `getBoundingClientRect()` | taffy layout position/size, offset by scroll position |
| `offsetWidth` / `offsetHeight` | taffy layout size including padding + border |
| `clientWidth` / `clientHeight` | taffy layout size including padding, excluding border + scrollbar |
| `offsetTop` / `offsetLeft` | Position relative to `offsetParent` |
| `scrollWidth` / `scrollHeight` | Content overflow dimensions |
| `window.innerWidth` | Virtual viewport width (default 1920) |
| `window.innerHeight` | Virtual viewport height (default 1080) |

## Lazy Computation

Layout is expensive. We only compute it when JS actually calls a layout API:

1. DOM mutation marks layout as dirty
2. `getBoundingClientRect()` triggers layout if dirty
3. Layout result is cached until next DOM mutation
4. Only the dirty subtree is re-laid-out (incremental)

## Virtual Viewport

No real screen. We simulate one:

```rust
pub struct Viewport {
    pub width: f32,          // 1920.0
    pub height: f32,         // 1080.0
    pub device_pixel_ratio: f32,  // 1.0
    pub scroll_x: f32,      // 0.0
    pub scroll_y: f32,      // 0.0
}
```

This matches the stealth profile's `screen` configuration.
