<div align="center">

# 🛡️ MuZap

**Менеджер стратегий [zapret](https://github.com/bol-van/zapret) для Windows**

Устанавливает DPI-bypass как службу, переключает режимы, обновляется сам и тестирует стратегии — всё через одно меню.

[![GitHub Release](https://img.shields.io/github/v/release/MuXolotl/MuZap?style=flat-square&color=4f8ef7)](https://github.com/MuXolotl/MuZap/releases/latest)
[![GitHub Downloads](https://img.shields.io/github/downloads/MuXolotl/MuZap/total?style=flat-square&color=4f8ef7)](https://github.com/MuXolotl/MuZap/releases)

[**Скачать последний релиз →**](https://github.com/MuXolotl/MuZap/releases/latest)

</div>

---

> [!CAUTION]
> ### ВАЖНО
> Если у вас уже установлен `Zapret` или `GoodbyeDPI` — **отключите их перед запуском MuZap**. Они используют один и тот же драйвер WinDivert и конфликтуют между собой.

> [!WARNING]
> ### Антивирус может ругаться — это нормально
> MuZap использует драйвер **WinDivert** для перехвата и фильтрации пакетов. Это легитимный инструмент (аналог iptables/NFQUEUE из Linux), но антивирусы нередко относят его к категории `RiskTool` или `Not-a-virus:PUA` — просто потому что такой же драйвер могут использовать вредоносные программы.
>
> Сам по себе WinDivert вирусом не является. Драйвер `WinDivert64.sys` подписан для загрузки в 64-битное ядро Windows.
>
> Если антивирус удаляет файлы — добавьте папку MuZap в исключения или отключите обнаружение PUA.

> [!IMPORTANT]
> Все бинарные файлы в папке [`bin/`](./bin) взяты из [zapret-win-bundle](https://github.com/bol-van/zapret-win-bundle/tree/master/zapret-winws). Всегда проверяйте контрольные суммы файлов, которые скачиваете из интернета.

---

## 🚀 Быстрый старт

1. [**Скачайте архив**](https://github.com/MuXolotl/MuZap/releases/latest) со страницы последнего релиза.

2. **Распакуйте** в папку без кириллицы и спецсимволов. Например: `C:\MuZap\`

3. **Запустите `MuZap.bat`** — скрипт сам запросит права администратора через UAC.

4. Выберите **Service → Install / Change Strategy**, укажите стратегию — готово.

> Если ни одна стратегия не помогла — попробуйте другие. Их много. Запустите **Tools → Run Tests**, чтобы автоматически найти лучшую для вашего провайдера.

---

## ✨ Что умеет MuZap

| Функция | Описание |
|---|---|
| 🔧 **Установка как служба** | Стратегия запускается автоматически вместе с Windows |
| 🔄 **Самообновление** | Скачивает и применяет новую версию прямо из меню |
| 🧪 **Тесты стратегий** | Стандартные HTTP-тесты и DPI-checkers по всем стратегиям |
| 📋 **Диагностика** | Проверяет конфликты, прокси, VPN, hosts, TCP timestamps и др. |
| 🎮 **Game Filter** | Расширяет обход на UDP/TCP портах выше 1023 (игры и другие приложения) |
| 🌐 **IPSet Filter** | Переключение режима обхода по IP-спискам |
| 📡 **Обновление hosts** | Исправляет веб-версию Telegram и голосовые чаты Discord |
| 📊 **Телеметрия** | Анонимно отправляет результаты тестов — помогает улучшать стратегии |

---

## 📁 Структура файлов

```
MuZap/
├── MuZap.bat               — главное меню: управление сервисом, настройки, обновления
├── muzap.ini               — пользовательские настройки (версия, фильтры, телеметрия)
├── strategies.ini          — набор стратегий DPI-bypass
│
├── lists/
│   ├── list-general.txt        — основной список доменов для обхода
│   ├── list-google.txt         — домены YouTube/Google
│   ├── list-exclude.txt        — исключения доменов
│   ├── ipset-all.txt           — IP-адреса и подсети для обхода
│   ├── ipset-exclude.txt       — исключения IP
│   └── *-user.txt              — ваши личные добавления (создаются при первом запуске)
│
├── utils/
│   ├── test_muzap.ps1          — тестирование стратегий
│   ├── update.ps1              — загрузка и применение обновлений
│   ├── telemetry.ps1           — отправка анонимной телеметрии
│   ├── hosts_manage.ps1        — управление MuZap-блоком в system hosts
│   ├── config_set.ps1          — запись настроек в muzap.ini
│   └── targets.txt             — список целей для тестов
│
└── bin/                    — бинарники (winws.exe, WinDivert и др.)
```

---

## 🗂️ Меню

<details>
<summary><b>Service</b> — управление Windows-службой</summary>

- **Install / Change Strategy** — выбрать стратегию из `strategies.ini` и установить как службу
- **Restart** — перезапустить службу MuZap
- **Remove** — остановить и удалить службу
- **Status** — показать статус службы и процесса `winws.exe`

</details>

<details>
<summary><b>Settings</b> — настройки</summary>

- **Game Filter** — режим обхода для игр и UDP-приложений:
  - `off` — отключён
  - `TCP+UDP` / `TCP` / `UDP` — выбрать нужные протоколы
  > После изменения MuZap предложит автоматически переустановить службу.
- **IPSet Filter** — режим работы с IP-списком:
  - `none` — заглушка, обход по IP отключён
  - `any` — все IP проходят через фильтр
  - `loaded` — только IP из `ipset-all.txt`
  > После изменения MuZap предложит перезапустить службу.
- **Telemetry** — анонимная отправка результатов тестов (провайдер, регион, страна, оценки стратегий). IP не хранится.

</details>

<details>
<summary><b>Updates</b> — обновления</summary>

- **Update IPSet List** — загрузить актуальный список IP из репозитория
- **Update Hosts File** — обновить MuZap-блок в системном `hosts` (нужно для веб-Telegram и голосовых чатов Discord)
- **Remove Hosts Entries** — удалить MuZap-блок из `hosts`
- **Check for Updates** — проверить наличие новой версии MuZap и обновиться

</details>

<details>
<summary><b>Tools</b> — инструменты</summary>

- **Run Diagnostics** — автоматическая проверка типовых проблем:
  - Base Filtering Engine, прокси, TCP timestamps, Adguard, Killer, Intel CNS, Check Point, SmartByte, VPN, hosts, WinDivert-конфликты, GoodbyeDPI и другие
  - В конце предлагается очистить кэш Discord
- **Run Tests** — запуск тестов стратегий:
  - **Standard** — HTTP/HTTPS и ping по целям из `targets.txt`
  - **DPI checkers** — TCP 16-20 KB freeze тест на провайдерах из открытой базы

</details>

---

## ➕ Добавление своих адресов

Файлы `*-user.txt` создаются автоматически при первом запуске и **не перезаписываются при обновлении**:

| Файл | Что добавлять |
|---|---|
| `lists/list-general-user.txt` | Домены для обхода (поддомены учитываются автоматически) |
| `lists/list-exclude-user.txt` | Домены-исключения (не обходить) |
| `lists/ipset-exclude-user.txt` | IP-адреса и подсети-исключения |

---

## ❓ Частые вопросы

<details>
<summary><b>Не работает Discord / YouTube / Telegram</b></summary>

1. Убедитесь, что в браузере настроен **Secure DNS** (DNS-over-HTTPS).
2. Попробуйте другие стратегии — **Tools → Run Tests** найдёт лучшую автоматически.
3. Для проблем с голосовыми чатами Discord или веб-версией Telegram — **Updates → Update Hosts File**.
4. Запустите **Tools → Run Diagnostics** — часто там видна причина.

</details>

<details>
<summary><b>Стратегия перестала работать</b></summary>

Это нормально. Провайдеры периодически обновляют DPI-оборудование и старые стратегии перестают работать. Попробуйте другие стратегии из меню, особенно `FAKE_TLS_AUTO_*` и `ALT*`.

Если совсем ничего не помогает — создайте [issue](https://github.com/MuXolotl/MuZap/issues).

</details>

<details>
<summary><b>Античит ругается на WinDivert</b></summary>

Смотрите инструкцию от bol-van: [zapret-win-bundle/windivert-hide](https://github.com/bol-van/zapret-win-bundle/tree/master/windivert-hide)

</details>

<details>
<summary><b>WinDivert остался в службах после удаления</b></summary>

Найдите название службы:
```cmd
driverquery | find "Divert"
```

Затем остановите и удалите:
```cmd
sc stop НАЗВАНИЕ
sc delete НАЗВАНИЕ
```

</details>

---

## ⚖️ Лицензия и атрибуция

Проект распространяется на условиях лицензии [MIT](LICENSE.txt).

**Основан на:**
- [Flowseal/zapret-discord-youtube](https://github.com/Flowseal/zapret-discord-youtube) — форк, с которого начинался MuZap
- [bol-van/zapret](https://github.com/bol-van/zapret) — оригинальный проект, core-логика DPI-bypass
- [bol-van/zapret-win-bundle](https://github.com/bol-van/zapret-win-bundle) — источник `winws.exe` и WinDivert
- [basil00/WinDivert](https://github.com/basil00/WinDivert) — драйвер перехвата трафика

---

<div align="center">

Если MuZap помог — поставьте ⭐, это мотивирует развивать проект дальше.

</div>
