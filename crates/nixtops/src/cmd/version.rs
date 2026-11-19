//! `nixtops version`: protocol, package and tool versions, all taken from the build.

use crate::{build_info, exit::Output};
use clap::Args as ClapArgs;

#[derive(Debug, ClapArgs)]
pub struct Args {}

/// The text of `version` for the given build values.
pub fn render(protocol: &str, package: &str, judge: &str, tools: &[(&str, &str)]) -> String {
    let mut text = format!("nixtops {package}\nprotocol {protocol}\n");
    if tools.is_empty() {
        text.push_str("tools: none pinned (built outside the kit's flake)\n");
        return text;
    }
    text.push_str(&format!("judge {judge}\ntools\n"));
    for (name, version) in tools {
        text.push_str(&format!("  {name} {version}\n"));
    }
    text
}

pub fn run(_args: &Args) -> Output {
    let tools = build_info::parse_tools(build_info::TOOLS);
    Output::ok(render(
        build_info::PROTOCOL,
        build_info::PACKAGE,
        build_info::JUDGE_PATH,
        &tools,
    ))
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn render_lists_every_tool_after_the_header() {
        let text = render("P", "K", "/j/nix", &[("a", "1"), ("b", "2")]);
        assert_eq!(
            text,
            "nixtops K\nprotocol P\njudge /j/nix\ntools\n  a 1\n  b 2\n"
        );
    }

    #[test]
    fn render_says_when_nothing_is_pinned() {
        let text = render("P", "K", "", &[]);
        assert_eq!(
            text,
            "nixtops K\nprotocol P\ntools: none pinned (built outside the kit's flake)\n"
        );
    }
}
