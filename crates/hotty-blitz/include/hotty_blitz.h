/* hotty-blitz C ABI (PoC-2). See crates/hotty-blitz/src/ffi.rs for the contract.
 * One host per terminal, used from one thread at a time. */
#ifndef HOTTY_BLITZ_H
#define HOTTY_BLITZ_H
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef struct hotty_host hotty_host;

typedef struct {
  uint32_t cell_w, cell_h; /* device pixels */
  float scale;             /* device pixels per CSS pixel */
  uint32_t fg, bg;         /* 0xRRGGBB */
  uint32_t palette[16];
  bool dark;
  const char *font_family; /* UTF-8, may be NULL */
  float font_size;         /* the terminal's, in CSS pixels; 0: from cell_h */
} hotty_config;

typedef struct {
  void *ctx;
  void (*reply)(void *ctx, const uint8_t *data, size_t len);
  /* The surface is cols x rows cells; the placement shows the window
   * x, y, w x h of it, over w x h cells at the cursor, above or below
   * overlapping placements by z (SPEC 5.2: greater above; among equals, the
   * one whose surface was created later, a greater `created`, above). */
  void (*place)(void *ctx, const char *surface, uint16_t cols, uint16_t rows,
                uint16_t x, uint16_t y, uint16_t w, uint16_t h, int32_t z, uint32_t created,
                bool move_cursor);
  void (*remove)(void *ctx, const char *surface); /* the surface is gone */
  /* The surface is hidden (SPEC 5.4): remove its placement, and keep what
   * shows it again cheaply. Its document stays. */
  void (*hide)(void *ctx, const char *surface);
} hotty_effects;

typedef struct { uint32_t x, y, w, h; } hotty_rect;

/* rgba: the whole frame, premultiplied RGBA8, `stride` bytes per row, valid
 * only during the call. Only `rects` changed; `full` when all of it did. */
typedef void (*hotty_frame_fn)(void *ctx, const char *surface, uint32_t w, uint32_t h,
                                const uint8_t *rgba, size_t stride, const hotty_rect *rects,
                                size_t nrects, bool full);

hotty_host *hotty_host_new(const hotty_config *cfg);
void hotty_host_free(hotty_host *h);
void hotty_host_configure(hotty_host *h, const hotty_config *cfg);
void hotty_host_osc(hotty_host *h, const uint8_t *body, size_t len, const hotty_effects *fx);
bool hotty_host_has_dirty(const hotty_host *h);
/* Milliseconds until an animated image (GIF, APNG, WebP) on a placed surface
 * shows its next frame: render then (hotty_host_has_dirty is true by that
 * time). 0 when one is due now, -1 when nothing plays or nothing that plays
 * is in a placement's window. Ask again after every call that renders or
 * handles commands, and keep one timer. */
int64_t hotty_host_next_frame(hotty_host *h);
/* The next render delivers the surface's whole frame, for an adapter that
 * must show it anew and no longer has its pixels. */
void hotty_host_redeliver(hotty_host *h, const char *surface);
void hotty_host_render(hotty_host *h, void *ctx, hotty_frame_fn cb);
void hotty_host_reset(hotty_host *h, const hotty_effects *fx);
/* kind: 0 move, 1 down, 2 up, 3 leave; down and up are the primary button's,
 * or a tap's (SPEC 10.1): pass no other button. A press also takes the
 * keyboard from any other surface that has it; its events come through fx.
 * From a press to its release the pointer is the surface's: pass it every
 * move, with x and y counted from its top left even outside it (negative, or
 * past its size), for drags (SPEC 9.1). Pass leave if the pointer is lost
 * before the release: it ends a drag. Pass no touch moves: touch scrolls. */
void hotty_host_pointer(hotty_host *h, const char *surface, uint32_t kind, float x, float y,
                         uint32_t mods, const hotty_effects *fx);
bool hotty_host_key(hotty_host *h, uint32_t key, const char *text, uint32_t mods,
                     const hotty_effects *fx);
void hotty_host_blur(hotty_host *h, const char *surface, const hotty_effects *fx);
const char *hotty_host_focused(hotty_host *h);
/* The pointer's shape over the surface after the last pointer event, as a
 * CSS cursor name ("pointer", "text", ...), or NULL for the host's own.
 * Valid until the next call. */
const char *hotty_host_cursor(hotty_host *h, const char *surface);
/* The hyperlink under the pointer on the surface after the last pointer
 * event (SPEC 9: a link with target="_blank"), as its url, or NULL. Treat it
 * as an OSC 8 hyperlink: its gesture, its feedback, its policies. A click on
 * it reports nothing to the program. Valid until the next call. */
const char *hotty_host_hyperlink(hotty_host *h, const char *surface);
/* Whether the surface takes the pointer at device pixel (x, y) of the
 * surface: false where only boxes with pointer-events: none are (SPEC 9.3).
 * There the pointer passes through it, to the placement below or the cells:
 * presses, motion and releases, with the terminal's mouse reporting. Ask
 * before giving the surface a press or a hover, not during its drag. */
bool hotty_host_takes_pointer(hotty_host *h, const char *surface, float x, float y);
/* The terminal does hand the pointer through (above): the capabilities then
 * carry "passthrough": true (SPEC 4). */
void hotty_host_set_passthrough(hotty_host *h, bool on);

#endif
