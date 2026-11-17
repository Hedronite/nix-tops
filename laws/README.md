# laws/

One file per law of the Nix-TOPS catalog (`NIX_TOPS.md` section 8.2): `N-01.toml` to `N-49.toml`. Each file validates against `schema/law.schema.json`; the metadata fields are those of section 8.3.

```
check-jsonschema --schemafile schema/law.schema.json laws/N-*.toml
```

The catalog row in `NIX_TOPS.md` and the file for the same ID say the same thing. `spec_check.py catalog` checks one thing about the files: there is one file per row of section 8.2 and no extra file. It does not compare the fields of a file with its row; that comparison belongs to the runner.

| Field | Meaning |
|---|---|
| `id`, `title`, `law` | Stable ID (never renumbered), short name, the rule |
| `severity` | `Refuse` fails; `Ack` needs an acknowledgement |
| `artifacts` | The Applies to cell of section 8.2, as a list; the law applies when one of these artifacts exists |
| `reader`, `reader_also`, `reader_detail` | Primary reader (`ev`, `tl`, `an`, `at`, `tbd`), further readers, and what is read or run |
| `judged` | Judged in protocol 0.1 (the check mark in section 8.2) |
| `sketch` | Section of the source sketch the law comes from, where there is one |
| `option_names` | Option names a law reads (N-34) |
| `rationale`, `fix` | Why the law exists; the usual fix |

`diagnostics.toml` is the table behind gate 3's third warning source (`NIX_TOPS.md` section 3.1): one `[[diagnostic]]` entry per diagnostic the `nix` command prints, per implementation, each with a `match` expression and a `fixture` line taken from a real run. Each entry carries an `effect`, `report` or `fail`. A classified line takes its entry's effect and a line no entry matches fails the gate; the table, not `NIX_TOPS.md`, decides. All five entries are `report`: the output inventory judges outputs, `N-30` judges `meta`, and gate 3's `--all-systems` leaves no system omitted. It is not a law file and has no `N-NN` row.
