{
  description = "Corpus gate-3 case: a check defined for one system only";
  outputs = { self }:
    let
  mk = system: name: builtins.derivation {
    inherit name system;
    builder = "/bin/sh";
    args = [ "-c" "echo ${name} > $out" ];
  };
  systems = [ "aarch64-darwin" "x86_64-linux" "aarch64-linux" ];
  forAll = f: builtins.listToAttrs (map (s: { name = s; value = f s; }) systems);
in
    {
      packages = forAll (s: { default = mk s "pkg"; });
      checks.x86_64-linux.only = mk "x86_64-linux" "only";
    };
}
