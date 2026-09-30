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
} hotty_config;

typedef struct {
  void *ctx;
  void (*reply)(void *ctx, const uint8_t *data, size_t len);
  /* The surface is cols x rows cells; the placement shows the window
   * x, y, w x h of it, over w x h cells at the cursor (SPEC 5.2). */
  void (*place)(void *ctx, const char *surface, uint16_t cols, uint16_t rows,
                uint16_t x, uint16_t y, uint16_t w, uint16_t h, bool move_cursor);
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
/* The next render delivers the surface's whole frame, for an adapter that
 * must show it anew and no longer has its pixels. */
void hotty_host_redeliver(hotty_host *h, const char *surface);
void hotty_host_render(hotty_host *h, void *ctx, hotty_frame_fn cb);
void hotty_host_reset(hotty_host *h, const hotty_effects *fx);
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

#endif
