use crossterm::style::Color;
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::{collections::HashMap, fs};

use super::{StatuslineStyle, Style, Theme, TokenStyle};

static SYNTAX_HIGHLIGHTING_MAP: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("constant", "constant");
    m.insert("entity.name.type", "type");
    m.insert("support.type", "type");
    m.insert("entity.name.function.constructor", "constructor");
    m.insert("variable.other.enummember", "constructor");
    m.insert("entity.name.function", "function");
    m.insert("meta.function-call", "function");
    m.insert("entity.name.function.member", "function.method");
    m.insert("variable.function", "function.method");
    m.insert("entity.name.function.macro", "function.macro");
    m.insert("support.function.macro", "function.macro");
    m.insert("variable.other.member", "property");
    m.insert("variable.other.property", "property");
    m.insert("variable.parameter", "variable.parameter");
    m.insert("entity.name.label", "label");
    m.insert("comment", "comment");
    m.insert("punctuation.definition.comment", "comment");
    m.insert("punctuation.section.block", "punctuation.bracket");
    m.insert("punctuation.definition.brackets", "punctuation.bracket");
    m.insert("punctuation.separator", "punctuation.delimiter");
    m.insert("punctuation.accessor", "punctuation.delimiter");
    m.insert("keyword", "keyword");
    m.insert("keyword.control", "keyword");
    m.insert("support.type.primitive", "type.builtin");
    m.insert("keyword.type", "type.builtin");
    m.insert("variable.language", "variable.builtin");
    m.insert("support.variable", "variable.builtin");
    m.insert("string.quoted.double", "string");
    m.insert("string.quoted.single", "string");
    m.insert("constant.language", "constant.builtin");
    m.insert("constant.numeric", "constant.builtin");
    m.insert("constant.character", "constant.builtin");
    m.insert("constant.character.escape", "escape");
    m.insert("keyword.operator", "operator");
    m.insert("storage.modifier.attribute", "attribute");
    m.insert("meta.attribute", "attribute");
    m
});

pub fn parse_vscode_theme(file: &str) -> anyhow::Result<Theme> {
    let contents = fs::read_to_string(file)?;
    let vscode_theme: VsCodeTheme = serde_json::from_str(&contents)?;

    let token_styles = vscode_theme
        .token_colors
        .into_iter()
        .map(|tc| tc.try_into())
        .collect::<Result<Vec<TokenStyle>, _>>()?;

    let gutter_style = Style {
        fg: vscode_theme
            .colors
            .iter()
            .find(|(c, _)| **c == "editorLineNumber.foreground".to_string())
            .map(|(_, hex)| {
                parse_rgb(hex.as_str().expect("editorLineNumber.foreground is string")).unwrap()
            }),
        bg: vscode_theme
            .colors
            .iter()
            .find(|(c, _)| **c == "editorLineNumber.background".to_string())
            .map(|(_, hex)| {
                parse_rgb(hex.as_str().expect("editorLineNumber.background is string")).unwrap()
            }),
        ..Default::default()
    };

    let statusline_style = StatuslineStyle {
        outer_style: Style {
            fg: Some(Color::Rgb { r: 0, g: 0, b: 0 }),
            bg: Some(Color::Rgb {
                r: 184,
                g: 144,
                b: 243,
            }),
            bold: true,
            ..Default::default()
        },
        outer_chars: [' ', '', '', ' '],
        inner_style: Style {
            fg: Some(Color::Rgb {
                r: 255,
                g: 255,
                b: 255,
            }),
            bg: Some(Color::Rgb {
                r: 67,
                g: 70,
                b: 89,
            }),
            bold: true,
            ..Default::default()
        },
    };

    Ok(Theme {
        name: vscode_theme.name.unwrap_or_default(),
        style: Style {
            fg: Some(parse_rgb(
                vscode_theme
                    .colors
                    .get("editor.foreground")
                    .expect("editor.foreground is present")
                    .as_str()
                    .expect("editor.foreground is string"),
            )?),
            bg: Some(parse_rgb(
                vscode_theme
                    .colors
                    .get("editor.background")
                    .expect("editor.background is present")
                    .as_str()
                    .expect("editor.background is string"),
            )?),
            bold: false,
            italic: false,
        },
        token_styles,
        gutter_style,
        statusline_style,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VsCodeTheme {
    name: Option<String>,
    #[serde(rename = "type")]
    typ: Option<String>,
    colors: Map<String, Value>,
    token_colors: Vec<VsCodeTokenColor>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VsCodeTokenColor {
    name: Option<String>,
    scope: VsCodeScope,
    settings: Map<String, Value>,
}

impl TryFrom<VsCodeTokenColor> for TokenStyle {
    type Error = anyhow::Error;

    fn try_from(tc: VsCodeTokenColor) -> Result<Self, Self::Error> {
        let mut style = Style::default();

        if let Some(fg) = tc.settings.get("foreground") {
            style.fg =
                Some(parse_rgb(fg.as_str().expect("fg is string")).expect("parsing rgb works"));
        }

        if let Some(bg) = tc.settings.get("background") {
            style.bg =
                Some(parse_rgb(bg.as_str().expect("bg is string")).expect("parsing rgb works"));
        }

        if let Some(font_styles) = tc.settings.get("fontStyle") {
            style.bold = font_styles
                .as_str()
                .expect("font_styles is string")
                .contains("bold");
            style.italic = font_styles
                .as_str()
                .expect("font_styles is string")
                .contains("italic");
        }

        Ok(Self {
            name: tc.name,
            scope: tc.scope.into(),
            style,
        })
    }
}

fn translate_scope(vscode_scope: String) -> String {
    let vscode_scope = SYNTAX_HIGHLIGHTING_MAP
        .get(&vscode_scope.as_str())
        .map(|s| s.to_string())
        .unwrap_or(vscode_scope);
    return vscode_scope;
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum VsCodeScope {
    Single(String),
    Multiple(Vec<String>),
}

impl From<VsCodeScope> for Vec<String> {
    fn from(scope: VsCodeScope) -> Self {
        match scope {
            VsCodeScope::Single(s) => vec![translate_scope(s)],
            VsCodeScope::Multiple(v) => v.into_iter().map(translate_scope).collect(),
        }
    }
}

fn parse_rgb(s: &str) -> anyhow::Result<Color> {
    if !s.starts_with("#") {
        anyhow::bail!("Invalid color format : {s}");
    }

    if s.len() != 7 {
        anyhow::bail!("Format must be in #rrggbb, got : {s}");
    }

    let r = u8::from_str_radix(&s[1..=2], 16)?;
    let g = u8::from_str_radix(&s[3..=4], 16)?;
    let b = u8::from_str_radix(&s[5..=6], 16)?;

    // println!("{r}, {g}, {b}");

    Ok(Color::Rgb { r, g, b })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_vscode_theme() {
        let theme = parse_vscode_theme("./src/fixtures/frappe.json").unwrap();
        println!("{:#?}", theme);
    }

    #[test]
    fn test_parse_rgb() {
        let rgb = "#08afBB";
        let rgb = parse_rgb(rgb);
        println!("{rgb:#?}");
    }
}
