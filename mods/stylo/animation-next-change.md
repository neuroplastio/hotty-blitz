---
title: "An animation says when its output next changes"
patch: 0006-Say-when-an-animation-s-or-transition-s-output-next-.patch
upstream-fit: yes
---

**What it changes:** `next_change(now)` on `Animation`, `Transition` and
`ElementAnimationSet`: when the output may next change, at or after
`now`. `None` while paused, finished or canceled; the start while it
waits out a delay; for a `steps()` easing, its next jump, the end of its
keyframe segment or of the iteration, whichever is first, per property
and direction as the value is computed; `now` while an easing moves
continuously or the animation is due to iterate or finish. The keyframe
lookup `get_property_declaration_at_time` does moves into
`keyframes_around`, which both use.

**Why we carry it:** an embedder resolves styles while
`needs_animation_ticks` holds, which is every frame. plx's toast has a
countdown bar drawn with `steps()` at ten steps a second: hottyterm
resolved it 60 times a second, and an animation in its delay cost the
same for nothing. With this, hotty-blitz asks its host for a frame when
the output next changes (mods/blitz `animation-next-change`), and a
`steps()` animation costs its steps, not every frame.

**For upstream:** yes. It is additive, Servo-only animation code, the
same question Servo's script thread could ask before it schedules the
next animation tick, and it does not change how a value is computed.
Its tests are in the module.
