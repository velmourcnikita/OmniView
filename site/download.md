# Скачать

Актуальные сборки публикуются в [GitHub Releases](https://github.com/velmourcnikita/OmniView/releases). Если выпуск пока не опубликован, используй папку dist/ из полного комплекта проекта.

- OmniView-0.4.0-win64.zip — Windows 10/11 x64.
- OmniView-0.4.0-win32.zip — Windows 10 x86; также запускается под WoW64.

Распакуй архив целиком. OmniView.exe запускает режим учителя, OmniView-Student.exe — ученика. Не смешивай файлы двух архитектур.

## Проверка файла

В PowerShell, из папки распакованного релиза:

~~~powershell
Get-FileHash .\OmniView.exe -Algorithm SHA256
~~~

Сравни с manifest.json из доверенного комплекта. Хеш выявляет изменение файла; сам по себе он не подтверждает издателя. Скрипт scripts/verify-release.ps1 проверяет все записи манифеста и сведения EXE.

Для архивов, собранных GitHub Actions по тегу и имеющих attestation:

~~~powershell
gh attestation verify OmniView-0.4.0-win64.zip --repo velmourcnikita/OmniView
~~~

Подтверждение происхождения появляется после успешного запуска соответствующего workflow. Локальные сборки не получают его автоматически.
