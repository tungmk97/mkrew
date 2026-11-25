# Composer polish - plan

Spec: `docs/superpowers/specs/2026-11-25-composer-polish-design.md`.
Branch `fix/composer-polish` in `.worktrees/composer-polish`.

1. [x] Remove the mode 2 workspace stripe (`shell/space_render.rs`); amend ADR 0115 and SPEC §10.
2. [x] Reproduce the placeholder box in the smoke sandbox (resumed omp session).
3. [x] `theme::ICON_FONT`; `push_runs` draws glyph runs in it.
4. [x] `preview::text`: glyph runs and font overrides (`icon_glyphs`, `font_overrides`) plus a unit test.
5. [x] `theme::TSP_THUMB_SIZE`, `thumbnail()`; image chips in the composer and `omp.user.images` images draw it; SPEC §11.
6. [x] `cargo fmt`, `cargo clippy -p mkrew-ui --all-targets -D warnings`, `cargo test -p mkrew-ui`.
7. [x] Smoke: placeholder, sent message (`#n` glyphs, thumbnails), a pasted image in the composer, a window per workspace; dark and light screenshots in `docs/screenshots/composer-polish-*.png`.
8. [x] Commit, merge into master (no push).
