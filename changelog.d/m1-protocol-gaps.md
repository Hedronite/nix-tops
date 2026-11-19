### Added

- `[layers]` keys `lint`, `build-test`, `coverage`, `facet-layers` and `laws` switch a layer of the gate report off with `{ enabled = false, reason = "..." }`; the reason is required, `source` and `eval` cannot be switched off, and the report shows the layer as `skip` with the reason. Switching off `laws` is allowed under every mode and the report says laws are not enforced; changing it needs the `protocol-change` label.
- `laws/diagnostics.toml` (`table_version` 2) classifies, as `report`, the four host-written settings the pinned CppNix does not know (`eval-cores`, `lazy-trees`, `bash-prompt-prefix`, `upgrade-nix-store-path-url`) and the "Git tree ... is dirty" line of each implementation.

### Changed

- `schema/gate-report.schema.json`: an output `level` may be `instantiated`, as section 6.1 already defines.
- Section 5.4 says a configuration lists `facets`, as the schema requires.
