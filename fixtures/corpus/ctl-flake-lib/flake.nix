{
  description = "Corpus control: a flake-lib that passes";
  outputs = { self }:
    let
      systems = [ "aarch64-darwin" "x86_64-linux" "aarch64-linux" ];
    in
    {
      lib = {
        double = n: n * 2;
        greet = name: "hello ${name}";
      };
      templates.default = {
        path = ./template;
        description = "A corpus template";
      };
      formatter = builtins.listToAttrs (map (s: { name = s; value = builtins.derivation {
        name = "fmt"; system = s; builder = "/bin/sh"; args = [ "-c" "echo fmt > $out" ];
      }; }) systems);
    };
}
