use std::io;

use crossterm::{
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
};

pub fn print_colored_tag(tag: &str, color: Color, msg: &str) {
    let mut stdout = io::stdout();
    let _ = execute!(
        stdout,
        SetForegroundColor(color),
        Print(format!("{tag} ")),
        ResetColor,
        Print(msg),
        Print("\n")
    );
}

pub fn print_colored_inline(text: &str, color: Color) {
    let mut stdout = io::stdout();
    let _ = execute!(stdout, SetForegroundColor(color), Print(text), ResetColor);
}
