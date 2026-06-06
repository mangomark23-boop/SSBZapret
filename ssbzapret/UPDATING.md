# Автообновление SSBZapret (как в Play Market)

Приложение умеет само проверять GitHub Releases, показывать баннер «Доступно
обновление» и по кнопке скачивать + ставить новую версию. Билдить заново
каждый раз нужно только тебе (разработчику) — пользователи обновляются в один клик.

## Как это работает

1. При запуске приложение запрашивает `latest.json` с GitHub Releases.
2. Если версия в `latest.json` выше текущей — снизу появляется баннер.
3. Пользователь жмёт «Обновить» → приложение качает установщик (`.exe` от NSIS),
   проверяет его **подпись**, ставит и перезапускается.
4. Подпись обязательна: без неё Tauri откажется ставить апдейт. Поэтому нужен
   ключ (см. ниже).

## Шаг 1. Сгенерировать ключ подписи (один раз)

На своей машине (там, где собираешь):

```powershell
npm run tauri signer generate -- -w %USERPROFILE%\.tauri\ssbzapret.key
# или, если ставил CLI глобально:
tauri signer generate -w %USERPROFILE%\.tauri\ssbzapret.key
```

Команда выведет:
- **приватный ключ** (файл `ssbzapret.key` + пароль) — храни в секрете, НИКОМУ не давай, в git НЕ коммить;
- **публичный ключ** (строка) — он идёт в конфиг.

Вставь публичный ключ в `src-tauri/tauri.conf.json`:

```json
"plugins": {
  "updater": {
    "endpoints": ["https://github.com/OWNER/REPO/releases/latest/download/latest.json"],
    "pubkey": "СЮДА_ПУБЛИЧНЫЙ_КЛЮЧ",
    "windows": { "installMode": "passive" }
  }
}
```

И замени `OWNER/REPO` на свой GitHub-репозиторий (например `mark/ssbzapret`).

## Шаг 2. Создать репозиторий на GitHub

1. Создай публичный репозиторий, например `ssbzapret`.
2. Запомни путь `OWNER/REPO` — он уже прописан в endpoints.

## Шаг 3. Собрать релиз с подписью

Перед сборкой задай переменные окружения с приватным ключом и паролем:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content $env:USERPROFILE\.tauri\ssbzapret.key -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "твой_пароль"
npm run tauri build
```

После сборки в `src-tauri/target/release/bundle/nsis/` появятся:
- `SSBZapret_<версия>_x64-setup.exe` — установщик;
- `SSBZapret_<версия>_x64-setup.exe.sig` — подпись (короткая строка внутри файла).

## Шаг 4. Опубликовать релиз

1. На GitHub → Releases → Draft a new release.
2. Tag версии, например `v0.2.0` (должен совпадать с `version` в `tauri.conf.json` и `Cargo.toml`).
3. Прикрепи `*-setup.exe`.
4. Прикрепи файл `latest.json` (шаблон ниже).
5. Publish.

### Шаблон latest.json

```json
{
  "version": "0.2.0",
  "notes": "Что нового в этой версии",
  "pub_date": "2026-06-06T00:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "СОДЕРЖИМОЕ_ФАЙЛА_.sig",
      "url": "https://github.com/OWNER/REPO/releases/download/v0.2.0/SSBZapret_0.2.0_x64-setup.exe"
    }
  }
}
```

- `signature` — это весь текст из файла `*-setup.exe.sig` (открой блокнотом, скопируй).
- `url` — прямая ссылка на установщик в этом релизе.
- `version` — без префикса `v`.

## Шаг 5. Поднять версию для следующего апдейта

Каждый новый релиз: подними `version` в **обоих** файлах:
- `src-tauri/tauri.conf.json` → `"version"`
- `src-tauri/Cargo.toml` → `version`

Собери, выложи новый релиз + новый `latest.json`. Пользователи увидят баннер при
следующем запуске.

---

## Можно автоматизировать (по желанию)

Если не хочется руками собирать `latest.json` и грузить файлы — есть
GitHub Action `tauri-apps/tauri-action`, который при пуше тега сам собирает на
Windows-раннере, подписывает и публикует релиз + `latest.json`. Тогда нужно
положить приватный ключ и пароль в GitHub Secrets, а winws.exe/cygwin1.dll —
в репозиторий или скачивать их шагом CI. Скажи — настрою workflow.
