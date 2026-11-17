# laws/

One file per law of the Nix-TOPS catalog (`NIX_TOPS.md` section 8.2): `N-01.toml` to `N-49.toml`. Each file validates against `schema/law.schema.json`; the metadata fields are those of section 8.3.

```
check-jsonschema --schemafile schema/law.schema.json laws/N-*.toml
```

The catalog row in `NIX_TOPS.md` and the file for the same ID say the same thing. `spec_check.py catalog` compares the two sets: one file per row, no extra file, and the judged count (`judged = true`) equal to the count in section 8.2.

| Field | Meaning |
|---|---|
| `id`, `title`, `law` | Stable ID (never renumbered), short name, the rule |
| `severity` | `Refuse` fails; `Ack` needs an acknowledgement |
| `classes` | Classes the law is an extra for (section 5.2); absent means every class |
| `reader`, `reader_also`, `reader_detail` | Primary reader (`ev`, `tl`, `an`, `at`, `tbd`), further readers, and what is read or run |
| `judged` | Judged in protocol 0.1 (the check mark in section 8.2) |
| `sketch` | Section of the source sketch the law comes from, where there is one |
| `option_names` | Option names a law reads (N-34) |
| `rationale`, `fix` | Why the law exists; the usual fix |
| `note` | An open point for the spec owners |

`diagnostics.toml` (the `nix` diagnostics table) joins this directory after the warning-model step merges.
