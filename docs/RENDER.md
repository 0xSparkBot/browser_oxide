# render — box tree to pixels

The half of a browser engine `browser_oxide` did not have.

Before this module, Skia was a dependency used only by the `<canvas>` element:
nothing painted a background, a border, or a text run belonging to a DOM
element. The CDP surface implemented 48 methods and `Page.captureScreenshot`
was not one of them — a CDP server that cannot screenshot is a CDP server with
no rasterizer behind it.

## Pipeline

```
Dom + LayoutEngine ──► painter ──► DisplayList ──► raster ──► RGBA8 / PNG
```

Three stages, deliberately separable.

| Stage | Module | What it does |
|---|---|---|
| Paint | `render::painter` | Walks the box tree in paint order, reads geometry and styles from the `LayoutEngine`, emits display items |
| List | `render::display_list` | Flat, ordered, paint-only. No references back into the DOM |
| Raster | `render::raster` | Replays onto an `SkCanvas` over a plain pixel buffer |

**The display list in the middle is the point.** It is flat and holds no DOM
references, which is what makes it cacheable and diffable — repainting a scroll
should replay a diff, not re-run layout. Nothing exploits that yet; the
structure exists so that it can.

## Entry points

```rust
// The whole thing
let png = browser_oxide::render::render_to_png(&dom, &mut layout, 1280, 800);

// Or from a Page
let png = page.screenshot_png(1280, 800);
let rgba = page.screenshot_rgba(1280, 800);

// Or over CDP
// { "method": "Page.captureScreenshot", "params": { "clip": { "width": 1280, "height": 800 } } }
```

There is also `cargo run --release --example screenshot -- <url|file.html> out.png`.

## Two invariants worth knowing

**Text carries positioned glyphs, never strings.** A display list holding
strings would have to reshape on every replay, and shaping is the expensive
half. `DisplayItem::Text` carries glyph ids and offsets, already shaped.

**A `FontRef` carries the face's bytes, not a family name.** Glyph ids index
into *that* face. Handing a rasterizer a family name lets it resolve to a
different file — a different version, a bold variant — and the same ids then
draw different letters. Silent, and invisible in a screenshot.

## Glyph rasterization matches `<canvas>`

`SkFont` is configured exactly as `canvas/canvas2d.rs` configures it: grayscale
antialiasing, subpixel positioning, no hinting. This is not cosmetic. Text
rasterization is a fingerprint surface, and a browser whose page text and
`<canvas>` text were rasterized differently would be reporting two different
renderers.

The painter also uses `LayoutEngine::styles()` rather than re-resolving styles,
and `layout::engine::collapse_white_space` rather than its own white-space
handling. Both for the same reason: if the painter formed a second opinion
about what layout decided, a box would be painted with a style that did not
size it, and it would look like a paint bug.

## Borders are trapezoids

Four trapezoids, not four rectangles. Adjacent sides meet at a mitre, and
overlapping rectangles paint one colour over the other at every corner —
visible the moment two sides have different colours.

Used width, not specified width: a border whose `border-style` is `none` has a
used width of zero and layout reserved no space for it, so painting the
specified width would draw outside the box.

## What is not here

- **No compositing.** No layer tree, no damage tracking, no incremental
  invalidation. A screenshot rasterizes the whole page every time.
- **No GPU surface.** CPU raster only. On a small page rasterization is ~90% of
  the frame, so this is the first thing to change if anything needs to animate.
- **No stacking contexts or `z-index`.** Paint order is tree order. Getting
  paint order wrong is the most common source of "looks subtly broken", so this
  is a known gap rather than a discovered one.
- **No images, SVG, form controls, transforms, opacity, filters, blend modes,
  border radius, box shadows or gradients.**
- **No hit-testing.** The display list is not queryable by point.

## Accuracy

Measured against Chrome 150 on a page of nested divs with backgrounds, borders
and text, at 900×700, tolerance 16 per channel:

**1.66% of pixels differ, mean per-channel delta 0.80.**

The residual is sub-pixel text positioning and glyph rasterization. Note the
engine renders with its bundled Liberation faces while the reference is
Chrome-on-Windows using Arial — Liberation is metric-compatible with Arial by
design, which is why the geometry agrees.

The harness is `firstpixel --diff a.png b.png` in the app repo.
