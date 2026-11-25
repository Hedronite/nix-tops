# Returns checks.<system>.nixtops-<tool>{,-findings} on the first supported system in `systems`
# and {} on the others, using filtered source (no IFD).
{ nixpkgs }:
{
  self,
  systems,
  config ? (self + "/nix-tops.toml"),
  exclude ? [ ],
}:
let
  inherit (nixpkgs) lib;
  _cfg = builtins.fromTOML (builtins.readFile config);
  lintCfg = _cfg.lint or { };

  supportedSystems = [
    "aarch64-darwin"
    "x86_64-linux"
    "aarch64-linux"
  ];
  eligible = builtins.filter (s: builtins.elem s supportedSystems) systems;
  lintSystem =
    if eligible == [ ] then
      throw "nix-tops lib.mkChecks: no supported system in systems"
    else
      builtins.head eligible;

  sortedExclude = builtins.sort builtins.lessThan exclude;
  excludeTag = "\nexclude:" + builtins.concatStringsSep "\n" sortedExclude;

  statixCfgRel = (lintCfg.statix or { }).config or "statix.toml";
  statixCfgExists = builtins.pathExists (self + "/${statixCfgRel}");
  statixCfgHash = builtins.hashString "sha256" (
    (if statixCfgExists then builtins.readFile (self + "/${statixCfgRel}") else "default") + excludeTag
  );

  deadnixFail = (lintCfg.deadnix or { }).fail or true;
  deadnixCfgHash = builtins.hashString "sha256" ("fail=${lib.boolToString deadnixFail}" + excludeTag);

  treefmtCfgRel =
    if builtins.pathExists (self + "/treefmt.toml") then
      "treefmt.toml"
    else if builtins.pathExists (self + "/.treefmt.toml") then
      ".treefmt.toml"
    else
      null;
  nixfmtCfgHash = builtins.hashString "sha256" (
    (if treefmtCfgRel != null then builtins.readFile (self + "/${treefmtCfgRel}") else "default-nixfmt")
    + excludeTag
  );

  gitleaksCfgRel = if builtins.pathExists (self + "/.gitleaks.toml") then ".gitleaks.toml" else null;
  gitleaksIgnoreRel =
    if builtins.pathExists (self + "/.gitleaksignore") then ".gitleaksignore" else null;
  gitleaksCfgHash = builtins.hashString "sha256" (
    (if gitleaksCfgRel != null then builtins.readFile (self + "/${gitleaksCfgRel}") else "default")
    + "|"
    + (if gitleaksIgnoreRel != null then builtins.readFile (self + "/${gitleaksIgnoreRel}") else "")
    + excludeTag
  );

  # `lib.fileset.toSource` accepts only a Nix path; a flake `self` is a store path string, so
  # `lib.sources.cleanSourceWith` filters it with the same `.nix`-only and config-file rule.
  filteredFor =
    extraFiles:
    if builtins.isPath self then
      let
        nixAll = lib.fileset.fileFilter (file: file.hasExt "nix") self;
        nixOnly =
          if exclude == [ ] then
            nixAll
          else
            lib.fileset.difference nixAll (
              lib.fileset.unions (map (p: lib.fileset.maybeMissing (self + "/${p}")) exclude)
            );
      in
      lib.fileset.toSource {
        root = self;
        fileset = lib.fileset.unions ([ nixOnly ] ++ map (rel: self + "/${rel}") extraFiles);
      }
    else
      let
        prefix = "${toString self}/";
        isExcluded = rel: builtins.any (p: rel == p || lib.hasPrefix "${p}/" rel) exclude;
      in
      lib.sources.cleanSourceWith {
        src = self;
        filter =
          path: type:
          let
            rel = lib.removePrefix prefix path;
          in
          if isExcluded rel then
            false
          else if type == "directory" then
            true
          else
            (lib.hasSuffix ".nix" rel) || (builtins.elem rel extraFiles);
      };

  nixfmtSrc = filteredFor (lib.optional (treefmtCfgRel != null) treefmtCfgRel);
  statixSrc = filteredFor (lib.optional statixCfgExists statixCfgRel);
  deadnixSrc = filteredFor [ ];
  gitleaksSrc = self;

  mkLintChecks =
    system:
    let
      pkgs = nixpkgs.legacyPackages.${system};
      kit = import ../tools.nix { inherit pkgs; };
      py = pkgs.python3;
      parser = ./parse_findings.py;
      defaultTreefmtCfg = pkgs.writeText "treefmt.toml" ''
        [formatter.nix]
        command = "nixfmt"
        includes = ["*.nix"]
      '';

      mkVerdict =
        tool: findings: failOnFinding:
        pkgs.runCommand "nixtops-${tool}" { nativeBuildInputs = [ py ]; } ''
          python3 ${parser} verdict ${findings} "$out" ${lib.boolToString failOnFinding}
        '';

      nixfmtFindings =
        pkgs.runCommand "nixtops-nixfmt-findings"
          {
            nativeBuildInputs = [
              kit.tools.treefmt
              kit.tools.nixfmt
              py
            ];
            passthru.forSystem = targetSystem: (mkLintChecks targetSystem).nixtops-nixfmt-findings;
          }
          ''
            work="$TMPDIR/work"
            mkdir -p "$work"
            cp -r "${nixfmtSrc}/." "$work/"
            chmod -R u+w "$work"
            cfg=${if treefmtCfgRel != null then "\"$work/${treefmtCfgRel}\"" else "\"${defaultTreefmtCfg}\""}
            raw="$TMPDIR/raw.out"
            treefmt --ci --no-cache --walk filesystem --config-file "$cfg" --tree-root "$work" >"$raw" 2>&1 || true
            python3 ${parser} findings nixfmt ${lib.escapeShellArg kit.tools.nixfmt.version} ${lib.escapeShellArg nixfmtCfgHash} "" "$raw" "$out"
          '';

      statixFindings =
        pkgs.runCommand "nixtops-statix-findings"
          {
            nativeBuildInputs = [
              kit.tools.statix
              py
            ];
            passthru.forSystem = targetSystem: (mkLintChecks targetSystem).nixtops-statix-findings;
          }
          ''
            raw="$TMPDIR/raw.out"
            statix check -o json ${lib.optionalString statixCfgExists "-c ${statixSrc}/${statixCfgRel}"} "${statixSrc}" >"$raw" || true
            python3 ${parser} findings statix ${lib.escapeShellArg kit.tools.statix.version} ${lib.escapeShellArg statixCfgHash} "${statixSrc}" "$raw" "$out"
          '';

      deadnixFindings =
        pkgs.runCommand "nixtops-deadnix-findings"
          {
            nativeBuildInputs = [
              kit.tools.deadnix
              py
            ];
            passthru.forSystem = targetSystem: (mkLintChecks targetSystem).nixtops-deadnix-findings;
          }
          ''
            raw="$TMPDIR/raw.out"
            deadnix -o json "${deadnixSrc}" >"$raw" || true
            python3 ${parser} findings deadnix ${lib.escapeShellArg kit.tools.deadnix.version} ${lib.escapeShellArg deadnixCfgHash} "${deadnixSrc}" "$raw" "$out"
          '';

      gitleaksFindings =
        pkgs.runCommand "nixtops-gitleaks-findings"
          {
            nativeBuildInputs = [
              kit.tools.gitleaks
              py
            ];
            passthru.forSystem = targetSystem: (mkLintChecks targetSystem).nixtops-gitleaks-findings;
          }
          ''
            raw="$TMPDIR/raw.json"
            gitleaks detect --no-git -s "${gitleaksSrc}" -f json -r "$raw" --exit-code 0 ${
              lib.optionalString (gitleaksCfgRel != null) "-c ${gitleaksSrc}/${gitleaksCfgRel}"
            } ${lib.optionalString (gitleaksIgnoreRel != null) "-i ${gitleaksSrc}"} >/dev/null 2>&1 || true
            python3 ${parser} findings gitleaks ${lib.escapeShellArg kit.tools.gitleaks.version} ${lib.escapeShellArg gitleaksCfgHash} "${gitleaksSrc}" "$raw" "$out"
          '';
    in
    {
      nixtops-nixfmt-findings = nixfmtFindings;
      nixtops-nixfmt = mkVerdict "nixfmt" nixfmtFindings true;
      nixtops-statix-findings = statixFindings;
      nixtops-statix = mkVerdict "statix" statixFindings true;
      nixtops-deadnix-findings = deadnixFindings;
      nixtops-deadnix = mkVerdict "deadnix" deadnixFindings deadnixFail;
      nixtops-gitleaks-findings = gitleaksFindings;
      nixtops-gitleaks = mkVerdict "gitleaks" gitleaksFindings true;
    };
in
builtins.listToAttrs (
  map (system: {
    name = system;
    value = if system == lintSystem then mkLintChecks system else { };
  }) systems
)
