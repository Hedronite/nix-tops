{
  description = "source 1: builtins.warn in an output";
  outputs = { self }: {
    packages.x86_64-linux.default = builtins.warn "fixture builtins.warn" (derivation {
      name = "fixture";
      system = "x86_64-linux";
      builder = "/bin/sh";
    });
  };
}
