{
  description = "Corpus gate-1 case";
  outputs = { self }:
    let extra = import ./extra.nix; in {
      checks.x86_64-linux.stub = builtins.derivation {
        name = "stub"; system = "x86_64-linux"; builder = "/bin/sh";
        args = [ "-c" "echo ${extra.text} > $out" ];
      };
    };
}
