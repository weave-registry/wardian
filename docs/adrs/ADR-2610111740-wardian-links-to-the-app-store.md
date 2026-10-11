# ADR-2610111740: Wardian links to the app store

**Status:** Accepted
**Date:** 2026-10-11
**Drivers:** The user: "we need to update wardian with a link to the app store".

## Context

wardian.studio has an app store: apps people made, each installed as a `.wardian` file that Wardian
imports. Nothing in Wardian said so. A new user met an empty shelf with Make an app and Import, and
no word of where finished apps are.

## Decision

1. **An "App store" link in the header**, before Docs and in the same style, to
   `https://wardian.studio/store/` in a new tab (`rel="noopener"`).
2. **Where a user looks for apps, the store is offered too:** "Find apps in the store" on the home
   card and in the empty app list, and a line in Settings → Import saying that Install in the store
   gives the `.wardian` file to choose there.
3. The address is one constant, `STORE`, in `static/index.html`.

## Consequences

- A user with no apps has somewhere to get one without building it.
- The link leaves Wardian for the website; Wardian still imports only what the user chooses.

## Implementation

`static/index.html` (the header link `storeLink`, `STORE`, the home card, the empty list, the
Import tab).
