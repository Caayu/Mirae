# 0029 — Interface layout, a small design system, explicit tool modes

Quality gate after 5F (2026-10-17;
[../quality/UI_AUDIT.md](../quality/UI_AUDIT.md)).

# Context

Each subphase had appended its panel to one side panel: colour,
selection, fill, gradient, transform and brush were stacked, so at the
default 1280×800 window the brush presets, the eraser, the brush size,
the transform buttons and the gradient's stop buttons lay below the
window (Q-01). The active tool was marked differently in each panel,
labels repeated ("Linear", "Ellipse", "None"), rows grew and moved the
controls below them, and the brush's eraser mode silently turned the
gradient and the bucket into erasers (Q-05). The owner asked for a
professional interface with the canvas first, predictable controls and a
small design system, without a new UI framework.

# Decision

1. **Layout** (in the app's `build_ui`, panels from `tela-ui`): a top bar
   (document name and unsaved dot, file commands, undo, redo); a **tool
   strip** at the left, one button per canvas tool, the active one filled
   with the accent colour; a **tool options** panel beside it showing the
   Colour section and only the active tool's options; the Document
   section and the layers at the right. Fixed panel widths; every panel
   scrolls, so every control can be reached at 800×600; the canvas fits
   the area the panels leave and keeps its place when the window
   resizes.
2. **Design system** (`tela_ui::style`): spacing, control height, the label
   column, type sizes, semantic colours (panel, chrome, surfaces, fields,
   text, muted, accent, warning), the egui theme built from them, and
   shared pieces — section, labelled row, segmented choice, tool button,
   Draw / Erase. Panels use them instead of their own numbers and
   colours.
3. **Explicit tool modes.** The gradient and the fill tools have their
   own **Mode: Draw / Erase** in their options (Draw by default, Erase
   shown in the warning colour with a line saying it removes paint). The
   brush's eraser mode no longer affects them. The stylus eraser end
   still erases with both (ADR 0026, ADR 0027). B selects the brush and
   leaves eraser mode.
4. **Interaction rules kept**: shortcuts, the reference's keys (G cycles
   fill and gradient, X, M, K, E), the transform's "Apply the
   transform?" question before another tool (now also for the strip),
   the egui-only boundary (ADR 0011).

# Consequences

- A user finds every tool in one place and sees which is active; options
  that do not apply are not shown.
- Commands that belonged to a tool's panel are reached through it or
  their shortcut: Edit › Fill is the Fill tool's button or Alt+Delete;
  the selection commands are in the Select tool's options and on
  Ctrl+A/D/Shift+I. Clear moved to the layer commands.
- An erasing gradient or fill needs the Erase mode (or the pen's eraser
  end); after E then G the gradient draws. This changes 5E/5F behaviour
  that the parity records described as "the eraser end / E erases":
  the eraser end is unchanged; the E key now means the brush's eraser
  only.
- The interface gate (`ui_layout.rs`) checks reachability, overlaps,
  stability, the marked tool and focus at five window sizes.

# Alternatives considered

- **Collapsing every panel in one scroll area** (the minimal fix of
  Q-01): reachable, but still a long list of unrelated controls with the
  active tool marked in many places.
- **Another UI framework**: excluded by the brief; egui serves.
- **Keeping the shared eraser mode and showing it in the gradient and
  fill options**: still surprising (a brush setting changing other
  tools), and Alt+Delete would still erase after E.
