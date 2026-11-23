---
title: Reload inline stylesheet imports
patch: 0108-Reload-inline-stylesheets-when-an-import-changes.patch
upstream-fit: yes
---
When an import changes, find the inline stylesheets that refer to its URL, including through another import, and rebuild them in place. Hotty can replace a CID while its document stays live. The same invalidation is useful with any mutable resource provider.
