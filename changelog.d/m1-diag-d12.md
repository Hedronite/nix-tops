### Changed

- `laws/diagnostics.toml` (`table_version` 3) adds D-12, `warning: unknown setting 'build-provenance-tags'`: a fifth setting a Determinate installation writes into `nix.conf` that the pinned CppNix does not know, classified `report` by exact name like D-06 to D-09. Measured in CI run 44559681127 on Determinate Nix 3.24.2. Any other unknown setting stays unclassified.
