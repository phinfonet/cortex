use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::Widget,
};

use super::theme;

pub struct KeybindingBar<'a> {
    pub bindings: &'a [(&'a str, &'a str)],
}

impl Widget for KeybindingBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let spans: Vec<Span> = self
            .bindings
            .iter()
            .enumerate()
            .flat_map(|(i, (key, label))| {
                let separator = if i > 0 {
                    vec![Span::styled("  ·  ", theme::style_dim())]
                } else {
                    vec![]
                };
                let key_span = Span::styled(format!("[{}]", key), theme::style_accent2());
                let label_span = Span::styled(format!(" {}", label), theme::style_dim());
                separator
                    .into_iter()
                    .chain([key_span, label_span])
                    .collect::<Vec<_>>()
            })
            .collect();

        ratatui::widgets::Block::default()
            .style(theme::style_normal())
            .render(area, buf);
        Line::from(spans)
            .style(theme::style_normal())
            .render(area, buf);
    }
}
