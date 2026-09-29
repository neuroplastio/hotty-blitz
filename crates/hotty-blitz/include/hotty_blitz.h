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
  void (*place)(void *ctx, const char *surface, uint16_t cols, uint16_t rows, bool move_cursor);
  void (*remove)(void *ctx, const char *surface);
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
void hotty_host_render(hotty_host *h, void *ctx, hotty_frame_fn cb);
void hotty_host_reset(hotty_host *h, const hotty_effects *fx);
void hotty_host_pointer(hotty_host *h, const char *surface, uint32_t kind, float x, float y,
                         uint32_t mods, const hotty_effects *fx);
bool hotty_host_key(hotty_host *h, uint32_t key, const char *text, uint32_t mods,
                     const hotty_effects *fx);
void hotty_host_blur(hotty_host *h, const char *surface, const hotty_effects *fx);
const char *hotty_host_focused(hotty_host *h);

#endif
