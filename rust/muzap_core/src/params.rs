/// Подстановка переменных в Params из strategies.ini
///
/// Поддержка:
/// - %BIN%
/// - %LISTS%
/// - %GameFilterTCP%
/// - %GameFilterUDP%
/// - EXCL_MARK -> ! (нужно, потому что в bat/registry есть экранирование !)
pub fn substitute_params(params: &str, bin: &str, lists: &str, tcp: &str, udp: &str) -> String {
    params
        .replace("%BIN%", bin)
        .replace("%LISTS%", lists)
        .replace("%GameFilterTCP%", tcp)
        .replace("%GameFilterUDP%", udp)
        .replace("EXCL_MARK", "!")
}
