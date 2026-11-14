# Kitesurf leg 4: covered changes

- Parley 0.12 patch 0005 (breaks around out-of-flow and atomic boxes) is not ported: on 0.13 forced newlines are appended as text and contribute line metrics before the explicit break (upstream Parley a7c1c56); the existing hotty Parley patch `Give inline boxes the wrap mode of their span` covers the wrap-mode and intrinsic-width portions. A host regression checks the two out-of-flow/forced-break sequences and consecutive inline blocks under `white-space:nowrap` on this base.
