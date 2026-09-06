use ratatui::DefaultTerminal;

/// Initialize the terminal: enable raw mode, enter alternate screen.
///
/// Delegates to ratatui's built-in init (new in 0.29/0.30), which also
/// installs a panic hook that restores the terminal before delegating to
/// the previous hook — the same behavior our hand-rolled hook provided,
/// so color-eyre's panic report still prints on a sane screen.
pub fn init() -> color_eyre::Result<DefaultTerminal> {
    Ok(ratatui::try_init()?)
}

/// Restore the terminal to its original state.
pub fn restore() -> color_eyre::Result<()> {
    ratatui::try_restore()?;
    Ok(())
}
