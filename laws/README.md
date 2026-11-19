# laws/

One file per law of the Nix-TOPS catalog (`NIX_TOPS.md` section 8.2): `N-01.toml` to `N-49.toml`. Each file validates against `schema/law.schema.json`; the metadata fields are those of section 8.3.

```
check-jsonschema --schemafile schema/law.schema.json laws/N-*.toml
```

The catalog row in `NIX_TOPS.md` and the file for the same ID say the same thing, and `spec_check.py catalog` checks it. It checks that there is one file per row of section 8.2 and no extra file, and then compares each file with its row, field by field. A mismatch fails and names the ID, the field, the file's value and the row's value.

| Field | File | Row cell of section 8.2 |
|---|---|---|
| `id` | `id` | the ID |
| `law` | `law` | Law, with markdown emphasis (`**`, `__`) stripped and whitespace normalized on both sides; code spans compare as written |
| `severity` | `severity` | Sev: `R` is `Refuse`, `A` is `Ack` |
| `reader` | `reader` | Reader: the first reader code (`ev`, `tl`, `an`, `at`, `tbd`) |
| `reader_also` | `reader_also` | Reader: every other code in the cell, in order, without the primary |
| `reader_detail` | `reader_detail` | Reader: the whole cell, normalized as the law text is; a row with only a bare code may leave the file field out |
| `sketch` | `sketch` | Sketch (an empty cell means the file has none) |
| `artifacts` | `artifacts` | Applies to, split on commas |
| `judged` | `judged` | v0.1: `true` exactly when the cell is the check mark |

Not compared: `title`, `rationale`, `fix` and `option_names`, which have no column in section 8.2, and the schema (the `check-jsonschema` command above checks that). The check needs a TOML parser, which is the standard library on Python 3.11 and `tomli` on 3.10; it fails and says so when neither is there.

The fields of a file and what they hold:

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

`diagnostics.toml` is the table behind gate 3's third warning source (`NIX_TOPS.md` section 3.1): one `[[diagnostic]]` entry per diagnostic the `nix` command prints, per implementation, each with a `match` expression and a `fixture` line taken from a real run. Each entry carries an `effect`, `report` or `fail`. A classified line takes its entry's effect and a line no entry matches fails the gate; the table, not `NIX_TOPS.md`, decides. All twelve entries are `report`: the output inventory judges outputs, `N-30` judges `meta`, gate 3's `--all-systems` leaves no system omitted, the five `host-setting` entries name settings a Determinate installation writes into `nix.conf` that the pinned CppNix does not know (exact names, never a general "unknown setting" pattern), and the `dirty-tree` entries record that Nix itself saw uncommitted changes (the gate-1 laws judge the tree). It is not a law file and has no `N-NN` row.
