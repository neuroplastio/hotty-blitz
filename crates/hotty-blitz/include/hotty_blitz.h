/* hotty-blitz C ABI (PoC-2). See crates/hotty-blitz/src/ffi.rs for the contract.
 * One host per terminal, used from one thread at a time.
 *
 * No call unwinds into the caller. A panic costs the surface whose document
 * the host was working in (all of them when it was in none): it is deleted,
 * and its remove comes with the next hotty_host_events, which
 * hotty_host_has_dirty asks for. The host carries on; only a panic while
 * recovering stops it for good, and every call then does nothing. */
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
  /* Put the UTF-8 text on the clipboard: a text field's cut (SPEC 10.2). */
  void (*clipboard)(void *ctx, const uint8_t *text, size_t len);
  /* A text field's paste (SPEC 10.2): read the clipboard and give its text
   * to hotty_host_paste. NULL: the terminal does not read it, and the key
   * reaches the program. */
  void (*paste)(void *ctx);
  /* a=edit (SPEC 10.6): the edit commands the program takes on the screen
   * it is on, in place of the last ones, comma-separated (empty: none).
   * Keep them per screen; a key of one goes to the program as
   * hotty_host_command. */
  void (*edit)(void *ctx, const uint8_t *names, size_t len);
  /* Show the level of the program's menu that its keys reach, the surface
   * `surface`, cols x rows cells, at the bottom right of the screen, above
   * everything (SPEC 10.7). Its pixels come from hotty_host_render; remove
   * hides it. */
  void (*menu)(void *ctx, const char *surface, uint16_t cols, uint16_t rows);
} hotty_effects;

typedef struct { uint32_t x, y, w, h; } hotty_rect;

/* rgba: the whole frame, premultiplied RGBA8, `stride` bytes per row, valid
 * only during the call. Only `rects` changed; `full` when all of it did. */
typedef void (*hotty_frame_fn)(void *ctx, const char *surface, uint32_t w, uint32_t h,
                                const uint8_t *rgba, size_t stride, const hotty_rect *rects,
                                size_t nrects, bool full);

hotty_host *hotty_host_new(const hotty_config *cfg);
void hotty_host_free(hotty_host *h);
/* New cell size, theme or font. A new font changes a cell in CSS pixels
 * (SPEC 5.3), and a program that asked for the capabilities hears it, as it
 * hears a new theme, as a caps event (SPEC 4, 9) through the next
 * hotty_host_events; a zoom (the cell and scale together) tells it nothing. */
void hotty_host_configure(hotty_host *h, const hotty_config *cfg);
void hotty_host_osc(hotty_host *h, const uint8_t *body, size_t len, const hotty_effects *fx);
bool hotty_host_has_dirty(const hotty_host *h);
/* Milliseconds until an animated image (GIF, APNG, WebP) on a placed surface
 * shows its next frame, scrollbars that fade are drawn again, or a scroll that
 * held a view back comes to rest (SPEC 9.5): render then (hotty_host_has_dirty
 * is true by that time). 0 when one is due now, -1 when nothing plays or
 * nothing that plays is in a placement's window, no scrollbar fades and no
 * view waits. Ask again after every call that renders, handles commands or
 * hands the host input, and keep one timer. */
int64_t hotty_host_next_frame(hotty_host *h);
/* The next render delivers the surface's whole frame, for an adapter that
 * must show it anew and no longer has its pixels. */
void hotty_host_redeliver(hotty_host *h, const char *surface);
void hotty_host_render(hotty_host *h, void *ctx, hotty_frame_fn cb);
/* What rendering and the terminal's settings found for the program, through
 * fx->reply: caps events (SPEC 4, 9) after hotty_host_configure,
 * hotty_host_set_network, hotty_host_set_passthrough or hotty_host_set_counts
 * changed what the capabilities say; fit events (SPEC 5.2: a surface with
 * f=1 hears the rows its document needs), with the rows of the last frame
 * drawn, and view events (SPEC 9.5: which elements with data-on~=view are in
 * view), with what it shows, at most one of each per surface; and, through
 * fx->remove, the surfaces a panic cost (above). Call after
 * hotty_host_render. */
void hotty_host_events(hotty_host *h, const hotty_effects *fx);
void hotty_host_reset(hotty_host *h, const hotty_effects *fx);
/* kind: 0 move, 1 down, 2 up, 3 leave; down and up are the primary button's
 * (SPEC 10.1): pass no other button. A press also takes the
 * keyboard from any other surface that has it; its events come through fx.
 * From a press to its release the pointer is the surface's: pass it every
 * move, with x and y counted from its top left even outside it (negative, or
 * past its size), for drags (SPEC 9.1). Pass leave if the pointer is lost
 * before the release: it ends a drag. Pass no touch here, not even a tap: a
 * touch over a surface goes to hotty_host_touch.
 * A surface with v=1 hears hover (SPEC 9.4) from moves, releases and
 * leaves: pass leave whenever the pointer goes off the surface (onto the
 * cells, another surface, a part it lets through, or out of the window).
 * count, for down and up: the platform's count of the press in a run of
 * clicks (GTK's n_press, AppKit's clickCount), 1 for a single one, which the
 * clicks the release makes carry (SPEC 9); 0 when it does not know. Call
 * hotty_host_set_counts once it passes them. */
void hotty_host_pointer(hotty_host *h, const char *surface, uint32_t kind, float x, float y,
                         uint32_t mods, uint32_t count, const hotty_effects *fx);
/* A wheel's turn, a touchpad's scroll or a touch drag at device pixel (x, y)
 * of the surface, by (dx, dy) device pixels: positive scrolls towards the
 * content's end, right and down, a wheel's notch as far as it scrolls the
 * cells. surface NULL or "": over the cells, which makes the gesture the
 * terminal's even where it goes on over a surface. mods as for pointer
 * events; shift turns a vertical wheel horizontal. Returns true if the
 * surface took it: its document scrolled, or overscroll-behavior stopped it
 * there (SPEC 5.3). Otherwise handle it as over the cells beneath (SPEC 9):
 * scrollback, or the program's wheel input. A gesture goes where its first
 * wheel went; wheels within 150 ms of each other are one, and a press ends
 * one. Its events (hover) come through fx; the view it changes (SPEC 9.5)
 * waits for it to come to rest (hotty_host_next_frame). */
bool hotty_host_wheel(hotty_host *h, const char *surface, float x, float y, float dx, float dy,
                      uint32_t mods, const hotty_effects *fx);
/* The wheel gesture under way ended (a touchpad's fingers lifted, or its
 * momentum stopped): the next wheel begins another. */
void hotty_host_end_gesture(hotty_host *h);
/* A finger on surface, the one its touch began on, at device pixel (x, y) of
 * it, counted from its top left even outside it. Pass every phase of a touch
 * that begins over a surface (where hotty_host_takes_pointer is true) here,
 * down first, and none of it to hotty_host_pointer. phase: 0 down (the
 * first finger touched), 1 move, 2 up (it lifted), 3 cancel (a second finger
 * touched, or the platform cancelled the touch), 4 long press (the terminal
 * took the touch for one). mods as for pointer events. Returns whose the touch is:
 *   0 the terminal's: scroll with it (hotty_host_wheel first), or take it
 *     for a long press, as with any touch;
 *   1 undecided: it may still drag, so hold its moves (no scroll yet);
 *   2 the surface's: a drag, or on up a tap, which the surface took as a
 *     click where the finger lifted; do nothing with it.
 * A move past the 8 CSS px tap slop decides an undecided touch: a drag (2)
 * when the touched element opts in to drags and its touch-action allows no
 * pan along the move, else a pan (0), which the terminal scrolls from where
 * the touch began. A second finger is cancel: the rest of the gesture is the
 * terminal's until the next down (SPEC 9.1). Events come through fx. */
uint32_t hotty_host_touch(hotty_host *h, const char *surface, uint32_t phase, float x, float y,
                          uint32_t mods, const hotty_effects *fx);
/* A key for the focused surface as the user pressed it, offered first,
 * before the terminal's own shortcuts and the bindings that translate keys
 * (SPEC 10.4). A press has up to two names, UTF-8, as myelin key names
 * write them, from the key event, with the platform's command key as
 * "super":
 *   physical: the physical key with the modifiers held ("ctrl+key_w",
 *     "super+arrow_left", "backspace"); for a modifier tapped alone and
 *     named on its release, its sided key ("shift_left");
 *   other: the character it types with the modifiers ("ctrl+w", "super+c");
 *     for that modifier, the modifier key ("shift"). Empty for a key that
 *     types no character, such as "arrow_left".
 * Either may be empty (length 0, the pointer may then be NULL), not both.
 * The host looks the physical name up first: the surface uses the press if
 * it uses either name, and where a keymap binds both, the physical binding
 * wins. For keys that type no text. True if the surface used it; its
 * events go through fx, and the terminal does nothing more with the key.
 * On false nothing happened: go on with the shortcuts, then
 * hotty_host_key_bytes. */
bool hotty_host_key_pressed(hotty_host *h, const uint8_t *physical, size_t physical_len,
                            const uint8_t *other, size_t other_len, const hotty_effects *fx);
/* A key for the focused surface, as the bytes the terminal would send the
 * program for it (SPEC 10.4): its key encoding, in the modes the program
 * set, or what a binding writes. True if the surface used any of the keys
 * in them; the others then reach the program through fx's replies, in
 * order. On false, send the bytes to the program as usual. */
bool hotty_host_key_bytes(hotty_host *h, const uint8_t *data, size_t len,
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
/* The text selected in the host, UTF-8, or NULL when nothing is (SPEC 11):
 * a surface document's selection, or the focused text field's, never a
 * password field's. Copy it when the terminal has no selection of its own,
 * and copy it on select after a press's release. Valid until the next
 * call. */
const char *hotty_host_selected_text(hotty_host *h);
/* Drops every surface's selection: there is one selection across the
 * terminal (SPEC 11), and the user pressed on the cells or selected there.
 * A press on a surface drops the others' itself. */
void hotty_host_clear_selection(hotty_host *h);

/* A key as pressed, for the selection in a surface's document outside a
 * text field (SPEC 11), offered before the surface that has the keyboard
 * and the terminal's own shortcuts, keyboard or not. It has up to two
 * names, as for hotty_host_key_pressed (physical, other; either may be
 * empty, not both): the selection's keymap goes by the first it binds,
 * the physical one first.
 * 0: no surface has such a selection; the key goes on. 1: its keymap binds
 * the key to copy: *text is the text for the clipboard, valid until the
 * next call, and the key goes no further. 2: it does not: the key goes on
 * as if nothing were selected, and the terminal copies nothing with it. */
int hotty_host_copy_key(hotty_host *h, const uint8_t *physical, size_t physical_len,
                        const uint8_t *other, size_t other_len, const char **text);

/* Types the UTF-8 text into the focused text field of the surface that has
 * the keyboard, as a field's paste does (SPEC 10.2, 11): the clipboard read
 * for fx->paste, or the terminal's own paste, which then does not reach the
 * program. False when no text field has the keyboard: the terminal pastes
 * to the program as usual. */
bool hotty_host_paste(hotty_host *h, const uint8_t *text, size_t len, const hotty_effects *fx);
/* Whether the surface takes the pointer at device pixel (x, y) of the
 * surface: false where only boxes with pointer-events: none are (SPEC 9.3).
 * There the pointer passes through it, to the placement below or the cells:
 * presses, motion and releases, with the terminal's mouse reporting. Ask
 * before giving the surface a press or a hover, not during its drag. */
bool hotty_host_takes_pointer(hotty_host *h, const char *surface, float x, float y);
/* The terminal does hand the pointer through (above): the capabilities then
 * list the feature "passthrough" (SPEC 4). */
void hotty_host_set_passthrough(hotty_host *h, bool on);
/* The terminal passes its platform's count of each press to
 * hotty_host_pointer: the capabilities then list the feature "count" (SPEC
 * 4, 9). */
void hotty_host_set_counts(hotty_host *h, bool on);
/* The host's half of the network policy (SPEC 7.2) in CSP's syntax, as its
 * user grants it: "img-src https://example.com; font-src https:". NULL or
 * empty: none, as a new host starts. The capabilities report it ("net"). A
 * change reaches a program that asked for them as a caps event, through the
 * next hotty_host_events. */
void hotty_host_set_network(hotty_host *h, const char *policy);
/* wake(ctx) is called on a fetching thread when something a document fetched
 * arrives or fails: render then, on the terminal's thread
 * (hotty_host_has_dirty is true). It must return quickly and not call into
 * hotty-blitz. NULL wake removes it; once this returns, or the host is freed,
 * the old one is not called again. */
void hotty_host_set_waker(hotty_host *h, void (*wake)(void *ctx), void *ctx);
/* How the host reads keys (SPEC 10.2, 10.4-10.6): mod_super, mod is "super"
 * (Command) rather than "ctrl" (SPEC 4); edit, the edit shortcuts as a
 * data-keys value of edit actions ("ctrl+c=copy ..."), NULL for the defaults;
 * commands, the terminal takes a=edit (fx->edit, hotty_host_command);
 * menus, it shows the program's menu (fx->menu), which the capabilities
 * then list as the feature "menu". A program that asked for the
 * capabilities hears what changed ("caps") through fx. */
void hotty_host_set_keys(hotty_host *h, bool mod_super, const char *edit, bool commands, bool menus,
                         const hotty_effects *fx);
/* The edit command a press is (SPEC 10.6): the edit action whose shortcut
 * either of its names is (physical, other, as for hotty_host_key_pressed;
 * either may be empty, not both), the physical one first. Static and
 * NUL-terminated; NULL for none. */
const char *hotty_host_edit_command(hotty_host *h, const uint8_t *physical, size_t physical_len,
                                    const uint8_t *other, size_t other_len);
/* Sends the program in the cells the edit command `name` (SPEC 10.6)
 * through fx's reply: from the key `key` (NULL from a menu), and, for a
 * paste, the clipboard's UTF-8 `text` (NULL when it holds no text). */
void hotty_host_command(const char *name, const uint8_t *key, size_t key_len,
                        const uint8_t *text, size_t text_len, const hotty_effects *fx);
/* A key that reaches the program in the cells while a level of its menu may
 * show (SPEC 10.7): "escape", and a key the level does not list, hide it.
 * The key has up to two names (physical, other, as for
 * hotty_host_key_pressed; either may be empty, not both); it is listed if
 * the level lists either.
 * Keys for a surface hide it as well, through hotty_host_key_pressed and
 * hotty_host_key_bytes. */
void hotty_host_cells_key(hotty_host *h, const uint8_t *physical, size_t physical_len,
                          const uint8_t *other, size_t other_len);
/* The terminal's screen changed, or is told for the first time (SPEC 10.7):
 * leaving the alternate screen drops its menu, and any change hides the
 * level shown. */
void hotty_host_set_screen(hotty_host *h, bool alt);
/* Picks the first item of the program's menu that has the UTF-8 name,
 * depth-first (SPEC 10.7). If it is enabled, sends the program `command`
 * with exactly {"name": name} through fx's reply, as hotty_host_command
 * does with no key, and returns true; else sends nothing and returns false.
 * For a list of commands the terminal already has, where it may offer the
 * menu's items; it draws none of its own for them. A terminal routes a pick
 * of copy or paste through its own Edit menu (SPEC 10.6): the selection
 * first, and the clipboard's text. */
bool hotty_host_pick(hotty_host *h, const uint8_t *name, size_t len, const hotty_effects *fx);

#endif
