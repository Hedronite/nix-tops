{ runCommand }: runCommand "quickstart-hello" { } "mkdir -p $out && echo hello > $out/hello"
