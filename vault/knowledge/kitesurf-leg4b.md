# Leg 4b filter follow-up

Vello CPU and AnyRender still execute only the first primitive of mixed filter chains, such as `blur(4px) brightness(.5)`. Matrix-only chains are supported, with separate passes when intermediate clamping is needed. General mixed and branching filter graphs need an upstream or later implementation.

Filters are detected while forwarding a scene. A newly filtered frame is repainted with zero workers; subsequent frames start with zero workers while the previous frame had a filter. After a filter-free frame the next one returns to the normal renderer. Measure both cases on hotty-perf before shipping.

`vault/bench/r33/filters.html` provides a filtered page for the measurement;
use a filter-free corpus page with the same build and an A/A control to check
that the forwarding check does not move its paint cost.
