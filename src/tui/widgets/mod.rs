use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Widget,
};

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
                    vec![Span::raw("  ")]
                } else {
                    vec![]
                };
                let key_span = Span::styled(
                    format!("[{}]", key),
                    Style::default().fg(Color::Cyan),
                );
                let label_span = Span::raw(format!(" {}", label));
                separator
                    .into_iter()
                    .chain([key_span, label_span])
                    .collect::<Vec<_>>()
            })
            .collect();

        Line::from(spans).render(area, buf);
    }
}
