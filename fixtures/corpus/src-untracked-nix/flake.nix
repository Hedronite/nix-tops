{
  description = "Corpus gate-1 case";
  outputs = { self }:
    let
      extra = import ./extra.nix;
      stub = text: builtins.derivation {
        name = "stub"; system = "x86_64-linux"; builder = "/bin/sh";
        args = [ "-c" "echo ${text} > $out" ];
      };
    in {
      # a package shows the package-set facet; the check reads the untracked file
      packages.x86_64-linux.default = stub "hi";
      checks.x86_64-linux.stub = stub extra.text;
    };
}
