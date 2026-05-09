use crossterm::style::Color as CrosstermColor;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::BorderType;

// Dracula-inspired palette
pub const BG: Color = Color::Rgb(40, 42, 54); // #282a36
pub const BG_SIDEBAR: Color = Color::Rgb(33, 34, 44); // #21222c
pub const BG_SELECTED: Color = Color::Rgb(68, 71, 90); // #44475a
pub const FG: Color = Color::Rgb(248, 248, 242); // #f8f8f2
pub const FG_DIM: Color = Color::Rgb(98, 114, 164); // #6272a4
pub const ACCENT: Color = Color::Rgb(189, 147, 249); // #bd93f9 purple
pub const ACCENT2: Color = Color::Rgb(139, 233, 253); // #8be9fd cyan
pub const GREEN: Color = Color::Rgb(80, 250, 123); // #50fa7b
pub const YELLOW: Color = Color::Rgb(241, 250, 140); // #f1fa8c
pub const ORANGE: Color = Color::Rgb(255, 184, 108); // #ffb86c
pub const RED: Color = Color::Rgb(255, 85, 85); // #ff5555
pub const PINK: Color = Color::Rgb(255, 121, 198); // #ff79c6
pub const SEPARATOR: Color = Color::Rgb(68, 71, 90); // #44475a

pub const BORDER: BorderType = BorderType::Rounded;

pub fn crossterm_bg() -> CrosstermColor {
    CrosstermColor::Rgb {
        r: 40,
        g: 42,
        b: 54,
    }
}

pub fn style_normal() -> Style {
    Style::default().fg(FG).bg(BG)
}

pub fn style_dim() -> Style {
    Style::default().fg(FG_DIM).bg(BG)
}

pub fn style_accent() -> Style {
    Style::default()
        .fg(ACCENT)
        .bg(BG)
        .add_modifier(Modifier::BOLD)
}

pub fn style_accent2() -> Style {
    Style::default().fg(ACCENT2).bg(BG)
}

pub fn style_selected() -> Style {
    Style::default()
        .bg(BG_SELECTED)
        .fg(FG)
        .add_modifier(Modifier::BOLD)
}

pub fn style_success() -> Style {
    Style::default().fg(GREEN).bg(BG)
}

pub fn style_warning() -> Style {
    Style::default().fg(YELLOW).bg(BG)
}

pub fn style_error() -> Style {
    Style::default().fg(RED).bg(BG)
}

pub fn style_sidebar_bg() -> Style {
    Style::default().fg(FG).bg(BG_SIDEBAR)
}

pub fn style_sidebar_header() -> Style {
    Style::default()
        .fg(ACCENT)
        .bg(BG_SIDEBAR)
        .add_modifier(Modifier::BOLD)
}

pub fn style_sidebar_item() -> Style {
    Style::default().fg(FG).bg(BG_SIDEBAR)
}

pub fn style_sidebar_dim() -> Style {
    Style::default().fg(FG_DIM).bg(BG_SIDEBAR)
}

pub fn style_sidebar_selected() -> Style {
    Style::default()
        .fg(ACCENT2)
        .bg(BG_SELECTED)
        .add_modifier(Modifier::BOLD)
}

pub fn style_input_focused() -> Style {
    Style::default().fg(FG).bg(BG)
}

pub fn style_input_idle() -> Style {
    Style::default().fg(FG_DIM).bg(BG)
}

pub fn style_section_active() -> Style {
    Style::default()
        .fg(YELLOW)
        .bg(BG)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}

pub fn style_section_inactive() -> Style {
    Style::default().fg(FG_DIM).bg(BG)
}

pub fn style_task_running() -> Style {
    Style::default().fg(ORANGE).bg(BG)
}

pub fn style_review_pending() -> Style {
    Style::default().fg(YELLOW).bg(BG)
}

pub fn style_review_accepted() -> Style {
    Style::default().fg(GREEN).bg(BG)
}

pub fn style_review_rejected() -> Style {
    Style::default().fg(RED).bg(BG)
}

pub fn style_pink() -> Style {
    Style::default()
        .fg(PINK)
        .bg(BG)
        .add_modifier(Modifier::BOLD)
}

pub fn style_orange() -> Style {
    Style::default().fg(ORANGE).bg(BG)
}

pub fn style_border() -> Style {
    Style::default().fg(SEPARATOR).bg(BG)
}

pub fn style_border_accent() -> Style {
    Style::default().fg(ACCENT2).bg(BG)
}

pub fn style_border_pink() -> Style {
    Style::default().fg(PINK).bg(BG)
}

pub fn style_border_warning() -> Style {
    Style::default().fg(YELLOW).bg(BG)
}

pub fn style_cursor() -> Style {
    Style::default()
        .fg(ACCENT2)
        .bg(BG)
        .add_modifier(Modifier::SLOW_BLINK)
}
