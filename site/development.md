# Сборка и тесты

```powershell
cargo test --bin omniview --locked
cargo build --release
powershell -File scripts/package.ps1 -Architecture x64 -SkipBuild
```

Для 32-битной версии используется `i686-pc-windows-gnullvm` и LLVM-MinGW:

```powershell
rustup target add i686-pc-windows-gnullvm
cargo test --bin omniview --target i686-pc-windows-gnullvm --locked
powershell -File scripts/package.ps1 -Architecture x86
```

Проверки включают TLS-паринг, сообщения, согласие на показ, захват, настройки, WebView smoke-test и Playwright-проверку интерфейса. Испытания на реальном старом железе и большой школьной сети всё ещё нужно проводить отдельно.
