# Menu cursor feedback patch

This is the runtime source of `gpui-component` 0.7.3 from crates.io, distributed
under Apache-2.0. The original `LICENSE-APACHE` is included.

- Archive: https://static.crates.io/crates/gpui-component/gpui-component-0.7.3.crate
- Archive SHA-256: `465a363235cd4062ef5f0953fc096fc764ed6f53dfbd9f409f669f93ea3b1ff9`
- Upstream repository: https://github.com/longbridge/gpui-kit
- Upstream snapshot recorded in package metadata:
  `67265a36e260ffe03c5aaebdcd8d654dbf892eab`, path `crates/component`.
- `src/`, `locales/`, the build script, README and original license are retained.
  The normalized manifest retains all runtime dependencies and features.
  Upstream integration tests and their seven `[[test]]` manifest declarations,
  the package-local lockfile, registry markers and redundant original manifest
  are omitted. No runtime module or locale was omitted.

## Change

In `src/menu/menu_item.rs`, `MenuItemElement::render` applies an arrow cursor
after instance styling, then replaces it with a pointing hand when the row is
enabled. This is the only behavioral modification.

The cursor covers the complete row, including its padding, for ordinary
actions, custom items and submenu triggers. Disabled rows, labels and
separators retain an arrow. Activation, hover selection, focus, keyboard
navigation, dismissal and layout remain the upstream implementation.

An application ancestor cannot provide this policy: each popup and submenu
occludes lower hitboxes, and submenus are deferred outside the parent menu's
layout bounds. Patch the shared row itself so nested menus follow the same
policy without reconstructing the menu implementation in application code.

The application retains native Windows cursor checks in its UI verification.
GPUI's public test window snapshots do not expose the resolved cursor, so a
style-only surrogate would not verify pointer hit testing on submenu rows.

Re-evaluate this patch when upgrading GPUI Component.
