{
  description = "source 3: an app without meta";
  outputs = { self }: {
    apps.x86_64-linux.default = {
      type = "app";
      program = "/bin/true";
    };
  };
}
