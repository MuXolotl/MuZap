//! Заготовки под будущий muzap_launcher.
//! Сейчас модуль не компилируется, если feature `launcher` не включена.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherSection {
    Сервис,
    Настройки,
    Обновления,
    Инструменты,
}
