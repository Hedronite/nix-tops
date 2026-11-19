{
  description = "Corpus gate-3 case: a package that warns on evaluation";
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
      packages = forAll (s: { default = builtins.warn "corpus: deprecated attribute" (mk s "pkg"); });
    };
}
