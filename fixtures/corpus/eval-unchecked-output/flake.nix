{
  description = "Corpus gate-3 case: an output the evaluator does not check";
  outputs = { self }:
    let
      systems = [ "aarch64-darwin" "x86_64-linux" "aarch64-linux" ];
    in
    {
      lib.double = n: n * 2;
      fooBar = { };
      # One checked output on every declared system, so toolchain.systems is true.
      formatter = builtins.listToAttrs (map (s: {
        name = s;
        value = builtins.derivation { name = "fmt"; system = s; builder = "/bin/sh"; args = [ "-c" "echo fmt > $out" ]; };
      }) systems);
    };
}
