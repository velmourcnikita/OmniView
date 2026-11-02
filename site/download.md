# Скачать

В [GitHub](https://github.com/velmourcnikita/OmniView) доступны исходники и проверки программы. Готовые архивы к релизам не прикладываются. Программу можно собрать по инструкции в README.

- Вариант x64 предназначен для Windows 10/11 x64.
- Вариант x86 предназначен для Windows 10 x86 и также запускается под WoW64.

Распакуй архив целиком. OmniView.exe запускает режим учителя, OmniView-Student.exe — ученика. Не смешивай файлы двух архитектур.

## Проверка файла

В PowerShell, из папки распакованного релиза:

~~~powershell
Get-FileHash .\OmniView.exe -Algorithm SHA256
~~~

Сравни с manifest.json из доверенного комплекта. Хеш выявляет изменение файла; сам по себе он не подтверждает издателя. Скрипт scripts/verify-release.ps1 проверяет все записи манифеста и сведения EXE.

Для архивов, собранных GitHub Actions по тегу и имеющих attestation:

~~~powershell
gh attestation verify OmniView-0.5.1-win64.zip --repo velmourcnikita/OmniView
~~~

Подтверждение происхождения появляется после успешного запуска соответствующего workflow. Локальные сборки не получают его автоматически.
