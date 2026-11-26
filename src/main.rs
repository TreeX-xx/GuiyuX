#![windows_subsystem = "windows"]

mod collection;
mod config;
mod language;
mod shell;
#[cfg(test)]
mod test_desktop;
mod ui;

use language::Language;
use std::cell::Cell;

thread_local! {
    static LANGUAGE: Cell<Language> = const { Cell::new(Language::System) };
}

fn language() -> Language {
    LANGUAGE.get()
}

fn change_language(choice: Language) -> std::io::Result<()> {
    let path = config::path()?;
    let mut settings = config::load(&path)?;
    settings.language = choice;
    config::save(&path, &settings)?;
    LANGUAGE.set(choice);
    Ok(())
}

fn run() -> std::io::Result<()> {
    use std::fs::{self, File, TryLockError};
    let path = config::path()?;
    fs::create_dir_all(path.parent().ok_or(std::io::ErrorKind::InvalidInput)?)?;
    let instance = File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_file_name("instance.lock"))?;
    match instance.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Ok(()),
        Err(TryLockError::Error(error)) => return Err(error),
    }
    let _apartment = shell::Apartment::open()?;
    let settings = config::load(&path)?;
    LANGUAGE.set(settings.language);
    ui::APPEARANCE_STYLE.set(settings.appearance);
    let result = ui::run(settings);
    let restore = collection::desktop::restore_on_exit();
    result.and(restore)
}

fn main() {
    if let Err(error) = run() {
        ui::report(&error);
        std::process::exit(1);
    }
}
