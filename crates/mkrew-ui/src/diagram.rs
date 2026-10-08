//! Mermaid diagrams of the Docs preview (SPEC §12, ADR 0021): ```mermaid blocks drawn to SVG
//! by `mermaid-rs-renderer` (pure Rust, no browser) in the theme's colors, both appearances
//! at once, shown with `gpui::img`. Slow the first time (it loads the system fonts): run it
//! off the UI thread.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use gpui::{Image, ImageFormat};
use mermaid_rs_renderer::ir::DiagramKind;
use mermaid_rs_renderer::{
    LayoutConfig, compute_layout, measure_svg_dimensions, parse_mermaid_strict, render_svg,
};

use crate::theme::{self, Theme};

#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub light: Arc<Image>,
    pub dark: Arc<Image>,
    /// Natural size, in px.
    pub width: f32,
    pub height: f32,
}

impl Diagram {
    /// The drawing for `theme`'s appearance.
    pub fn image(&self, theme: &Theme) -> Arc<Image> {
        Arc::clone(if theme.dark { &self.dark } else { &self.light })
    }
}

/// Draws `source`; `Err` says why it cannot be drawn (shown above its source).
pub fn render(source: &str) -> Result<Diagram, String> {
    // The renderer panics on some half-typed sources (e.g. `A[/]`); this runs on a gpui
    // background thread, where a panic aborts the whole UI.
    std::panic::catch_unwind(AssertUnwindSafe(|| draw(source)))
        .unwrap_or_else(|_| Err("the diagram renderer crashed on this source".to_owned()))
}

fn draw(source: &str) -> Result<Diagram, String> {
    let parsed = parse_mermaid_strict(source).map_err(|error| error.to_string())?;
    let config = LayoutConfig::default();
    // One layout per appearance: some layouts (pie, journey, mindmap) bake the theme's
    // colors in.
    let draw = |theme: &Theme| {
        let colors = colors(theme, parsed.graph.kind);
        let layout = compute_layout(&parsed.graph, &colors, &config);
        let size = measure_svg_dimensions(&layout, &config, None);
        let svg = render_svg(&layout, &colors, &config);
        (
            Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())),
            size,
        )
    };
    let (light, light_size) = draw(&Theme::light());
    let (dark, dark_size) = draw(&Theme::dark());
    // Both share fonts and sizes, so the sizes match; the larger one never clips either.
    Ok(Diagram {
        light,
        dark,
        width: light_size.width.max(dark_size.width),
        height: light_size.height.max(dark_size.height),
    })
}

/// The renderer's theme in Mkrew's tokens (SPEC §14) for a `kind` diagram.
fn colors(theme: &Theme, kind: DiagramKind) -> mermaid_rs_renderer::Theme {
    let hex = |color: u32| format!("#{color:06X}");
    mermaid_rs_renderer::Theme {
        font_family: theme::DIAGRAM_FONT.to_owned(),
        font_size: theme::doc::BODY,
        background: hex(theme.surface),
        primary_color: hex(theme.surface_raised),
        primary_text_color: hex(theme.text),
        primary_border_color: hex(theme.text_muted),
        line_color: hex(theme.text_muted),
        secondary_color: hex(theme.surface_raised),
        tertiary_color: hex(theme.surface),
        // Pie labels of thin slices sit outside the pie on this box, in the slice-label color
        // (`surface`), so the box takes the text color there.
        edge_label_background: hex(if kind == DiagramKind::Pie {
            theme.text
        } else {
            theme.surface
        }),
        cluster_background: hex(theme.bg),
        cluster_border: hex(theme.border),
        text_color: hex(theme.text),
        sequence_actor_fill: hex(theme.surface_raised),
        sequence_actor_border: hex(theme.text_muted),
        sequence_actor_line: hex(theme.text_muted),
        sequence_note_fill: hex(theme.terminal_match_bg),
        sequence_note_border: hex(theme.status_needs_input),
        sequence_activation_fill: hex(theme.surface_raised),
        sequence_activation_border: hex(theme.text_muted),
        pie_title_text_color: hex(theme.text),
        // Workspace colors stand out (≥ 3:1) from every surface, so surface-colored labels read
        // on them in both appearances.
        pie_colors: std::array::from_fn(|index| hex(token_color(theme, index))),
        pie_section_text_color: hex(theme.surface),
        pie_legend_text_color: hex(theme.text),
        pie_stroke_color: hex(theme.surface),
        pie_outer_stroke_color: hex(theme.border),
        pie_opacity: 1.0,
        git_colors: std::array::from_fn(|index| hex(token_color(theme, index))),
        git_inv_colors: std::array::from_fn(|_| hex(theme.surface)),
        git_branch_label_colors: std::array::from_fn(|_| hex(theme.surface)),
        git_commit_label_color: hex(theme.text),
        git_commit_label_background: hex(theme.surface_raised),
        git_tag_label_color: hex(theme.text),
        git_tag_label_background: hex(theme.surface_raised),
        git_tag_label_border: hex(theme.border),
        ..mermaid_rs_renderer::Theme::modern()
    }
}

/// The `index`th series color: the workspace colors, repeated.
fn token_color(theme: &Theme, index: usize) -> u32 {
    theme.workspace_colors[index % theme.workspace_colors.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flowcharts_draw_in_both_appearances_and_bad_sources_say_why() {
        let drawn = render("flowchart LR\n  A[Plan] --> B{Review}\n  B -->|ok| C[Ship]\n").unwrap();
        assert!(drawn.width > 0.0 && drawn.height > 0.0);
        let svg = |image: &Arc<Image>| String::from_utf8(image.bytes.clone()).unwrap();
        let surface = |theme: Theme| format!("#{:06X}", theme.surface);
        assert!(svg(&drawn.dark).contains(&surface(Theme::dark())));
        assert!(svg(&drawn.light).contains(&surface(Theme::light())));
        assert_ne!(drawn.dark, drawn.light);
        let bad = render("flowchart LR\n  A -->").unwrap_err();
        assert!(bad.contains("A -->"), "{bad}");
    }

    #[test]
    fn renderer_crashes_become_a_reason() {
        let crashed = render("flowchart LR\nA[/]\n").unwrap_err();
        assert!(crashed.contains("crashed"), "{crashed}");
    }

    #[test]
    fn pie_labels_read_on_every_slice_and_outside_in_both_appearances() {
        for theme in [Theme::dark(), Theme::light()] {
            let colors = colors(&theme, DiagramKind::Pie);
            let label = parse(&colors.pie_section_text_color);
            for fill in colors
                .pie_colors
                .iter()
                .chain([&colors.edge_label_background])
            {
                let ratio = theme::contrast(label, parse(fill));
                assert!(ratio >= 3.0, "{fill} vs label: {ratio}");
            }
        }
        // A 3 % slice has its label outside, on the label box.
        let drawn = render("pie\n  \"A\": 95\n  \"B\": 3\n").unwrap();
        let svg = |image: &Arc<Image>| String::from_utf8(image.bytes.clone()).unwrap();
        for (image, theme) in [(&drawn.dark, Theme::dark()), (&drawn.light, Theme::light())] {
            let svg = svg(image);
            let first = format!("#{:06X}", theme.workspace_colors[0]);
            assert!(svg.contains(&first), "slices in this appearance's colors");
            let label_box = format!("ry=\"2\" fill=\"#{:06X}\" stroke=\"none\"", theme.text);
            assert!(svg.contains(&label_box), "outside label box");
        }
    }

    fn parse(hex: &str) -> u32 {
        u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap()
    }
}
