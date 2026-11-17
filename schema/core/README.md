# schema/core

Definitions shared by the TOPS protocols: verdicts, enforcement modes, baseline identity and acks. They match the blocks marked `tops-core` in [`NIX_TOPS.md`](../../NIX_TOPS.md) (§3.3, §6.8).

A `tops-core` sync replaces this whole directory. Do not edit Nix-specific shapes here; put them in the schema that `$ref`s `core/core.schema.json`.

The files carry no `$id`, so references are relative and resolve from a checkout. Validate with `check-jsonschema --base-uri file://$PWD/schema/<schema> --schemafile schema/<schema> <instance>`; `scripts/spec_check.py schema` does this.
