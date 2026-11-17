{
  description = "source 3: an unknown flake output";
  outputs = { self }: {
    fooBar.x86_64-linux.default = { };
  };
}
