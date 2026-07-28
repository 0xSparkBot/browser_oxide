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

## Compositing

A fourth stage, for anything that moves:

```
painter ──► LayerTree ──► Compositor ──► RGBA8
```

The compositor keeps each layer's rasterized surface between frames. A scroll
changes no layer's *content*, so the next frame is a blit at a new offset — no
paint, no shaping, no Skia. Without it every scroll frame re-rasterizes the
page, which on a CPU surface is ~90% of the frame budget spent redrawing pixels
that did not change.

`render_to_png` does not use it — a one-off screenshot has nothing to reuse —
but anything scrolling or animating should.

### Promotion

Every layer is a separate surface and separate memory, so the list of reasons is
short on purpose:

| Reason | Why |
|---|---|
| `Root` | The document. Always present, always first |
| `Opacity` | `opacity < 1`. The subtree must composite as a unit, or overlapping children show through each other |
| `Transform` | A non-identity `transform` |
| `Fixed` | `position: fixed`. Not an optimisation — a fixed element must *not* move when the page scrolls, and the only way to say that to a compositor that scrolls by translating surfaces is to give it a surface of its own |

`will-change` is not here because the engine has no such property yet. When it
arrives it belongs in `promotion_reason` and nowhere else.

### Damage

A layer is re-rasterized when its display list differs from the one it was last
rasterized from — a **structural** comparison, not pointer identity. A relayout
rebuilds the list from scratch even when nothing changed, and treating that as
damage would defeat the cache entirely.

That comparison is only cheap because the display list is flat and holds no DOM
references, which is the reason it is structured that way.

Layers entirely outside the viewport are culled before rasterization, and
surfaces for layers that no longer exist are dropped, so a long-lived compositor
over a changing document does not grow without bound.

### Coordinates

A layer's display list holds **page** coordinates — that is what lets hit
regions and damage comparison work without every layer rebasing them — while its
surface is only as big as its own bounds. Rasterization translates by
`-bounds.origin`; the compositor puts the surface back.

## Hit-testing

`painter::paint_layered` returns hit regions alongside the layer tree, and
`render::hit_test(&regions, x, y)` returns the topmost element at a point in
page coordinates. Regions are recorded in paint order, so the last one
containing the point wins — which is what `document.elementFromPoint` means.

The display list itself holds no DOM references, so this is a parallel list.
`RENDERER_DESIGN.md` says hit-testing should reuse the fragment tree; there is
no fragment tree yet, and this is the honest interim.

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

- **No GPU surface.** CPU raster only. Compositing removes the repaint cost of
  scrolling, but the final blit is still CPU.
- **No damage *rectangles*.** Damage is per layer, not per region: a one-pixel
  change re-rasterizes its whole layer.
- **No scroll containers.** Only the document scrolls; `overflow: scroll` on an
  element clips but does not scroll.
- **No 3D transforms.** `rotate3d`, `matrix3d` and perspective are rejected at
  parse time rather than silently flattened — wrong and visible beats wrong and
  invisible.
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
