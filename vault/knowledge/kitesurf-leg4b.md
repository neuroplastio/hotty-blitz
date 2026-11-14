# Leg 4b filter follow-up

Vello CPU and AnyRender still execute only the first primitive of mixed filter chains, such as `blur(4px) brightness(.5)`. Matrix-only chains are supported, with separate passes when intermediate clamping is needed. General mixed and branching filter graphs need an upstream or later implementation.

Filtered frames are first detected while forwarding their scene through the normal renderer, then repainted using a zero-worker renderer. Filter-free frames take the ordinary single pass. Measure both cases on hotty-perf before shipping.

`vault/bench/r33/filters.html` provides a filtered page for the measurement;
use a filter-free corpus page with the same build and an A/A control to check
that the forwarding check does not move its paint cost.
