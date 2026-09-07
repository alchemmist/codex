# Antex — промежуточная передача работы

Дата: 2026-09-07. Ветка: `antex`. Работа поставлена на паузу по просьбе пользователя.
PLAN.md целиком НЕ выполнен; релизная готовность НЕ заявляется.

## Что является рабочим результатом

В `codex-rs/antex/` находится самостоятельный Cargo workspace: `core`,
`provider-openai`, `runtime`, `tui`, `cli`, `extension-protocol`.
Новый frontend запускается явно командой `antex tui` и обращается к `AgentRun`
напрямую, без app-server и зависимости от legacy Codex core. Есть также `exec`.
Запуск без подкоманды пока не переключён на новый TUI.

Это проверенный Linux development checkpoint, а не готовая замена установленного
агента: реальный запрос к ChatGPT под пользовательской подпиской ещё не прошёл.
Интеграционные тесты используют настоящий новый kernel с подставным provider.
MCP, workflows и subagents через новые расширения пока недоступны.

## Состояние относительно PLAN.md

| Фаза | Реализованная часть | Что остаётся |
| --- | --- | --- |
| 0: baseline | Зафиксированы исходные данные, fixtures и fallback | Чистые платформенные сборки и полные замеры отложены пользователем до release preflight |
| 1: identity | Antex identity и отдельный development CLI; безопасный import в legacy-пути | Полный аудит брендинга; перенос migrate в независимый CLI; installer/update cutover |
| 2: kernel | Provider-neutral agent, bounded events/context, tool validation, steer/follow-up, interrupt, retries, approvals/questions | Формальная приёмка всех API/размерных gates; завершение общего cutover |
| 3: OpenAI | OAuth browser/device, accounts/refresh, models, bounded HTTP/SSE Responses и provider continuation | Живой login/turn, compatibility identity probe, аудит WebSocket/transport parity |
| 4: runtime | read/write/edit/shell, permissions, sandbox, JSONL sessions, resume/fork, compaction, AGENTS/skills, images | Перенос старых данных в новую модель сессии; полная security/platform приёмка |
| 5: TUI | Работающий прямой frontend и значительная часть исходного UX | Полный baseline parity, reflow/palette stress, оставшиеся bindings, размеры модулей, default entry cutover |
| 6: extensions | Только protocol scaffold и четыре теста | Host, lifecycle/isolation, registry/trust, SDK, Rust/Python conformance fixtures |
| 7: first-party extensions | Не перенесены | MCP, explicit subagents, workflows, tmux logging и прочие обязательные расширения |
| 8: удаление legacy | Переносимые presentation-файлы перемещены с сохранением истории | Удаление старой инфраструктуры, source-path bridges и запрещённых зависимостей |
| 9: rename | Не выполнена | Финальная структура каталогов и переименование GitHub repository |
| 10: release | Не выполнена | Release/install/update pipeline, бюджеты обеих платформ, live acceptance и 0.0.1 |

Checkboxes целых фаз оставлены незакрытыми намеренно: наличие реализации не
означает прохождение всех completion criteria. Исторические цифры внутри PLAN
описывают прежние срезы; актуальная контрольная проверка приведена ниже.

## Существенные реализованные детали

- Kernel сохраняет принятый ввод на границе отмены, не переигрывает частично
  выданный ответ и валидирует аргументы до запуска инструмента.
- Runtime использует ограниченный доступ к workspace, атомарную запись и
  newline-safe edit; Linux shell работает через bubblewrap/seccomp, отменяет
  группу процессов и ограничивает вывод. Full permissions — явный обход sandbox,
  а не обещание изоляции. Автоматического unsandboxed fallback нет.
- Append-only JSONL хранит ветви, результаты и checkpoint compaction; исходная
  история остаётся доступной через transcript. UI state и очередь ввода отделены
  от model history. Очередь согласуется с фактически закоммиченными сообщениями.
- TUI сохраняет inline scrollback, ant skins, Markdown, syntax/diffs, Unicode,
  Vim/Russian commands, rich undo/redo, защиту от paste burst, stash с картинками,
  clipboard copy, image attachment, approvals/questions, model/session/fork
  pickers, transcript paging, темы и отменяемые foreground operations.
- По SSH чтение изображения из clipboard удалённой машины не подменяет локальное:
  пользователь получает подсказку использовать `/image`. Copy поддерживает
  существующий tmux/OSC52 путь.
- Protocol scaffold задаёт JSON-RPC/NDJSON, frame/text bounds и capability checks.
  Он не запускает процессы. Требование «Agent action только после явной команды
  пользователя» должен реализовать будущий host; сейчас это не готовая защита.

## Что проверено

Все сборки, Rust-тесты и Clippy выполнялись на deimos, не на Mac.

1. Финальный focused run шести новых пакетов: **893 passed, 1 skipped**, 5.139 s
   выполнения тестов после компиляции. Лог локально:
   `/tmp/antex-checkpoint-tests.log`.
2. Legacy TUI после переноса paste detector: **4352 passed, 6 skipped**, 22.690 s.
   Один существующий startup-draft тест прошёл на повторе (flaky), а не с первого
   запуска. Лог: `/tmp/antex-legacy-paste-tests.log`. Это не весь legacy workspace.
3. Свежая debug-сборка `antex tui` в tmux с чистой конфигурацией и отдельным home:
   editable startup, `/help`, закрытие Escape, ввод `checkpoint-draft`, stash и
   восстановление Ctrl+S, `/status`, `/quit` с exit 0. Без авторизованного аккаунта.
   Ранее отдельно проверялось узкое окно; полным resize/reflow acceptance это не является.
4. Scoped `just fix` для шести новых пакетов завершился успешно. Остались warnings
   от переносимых presentation-модулей. Для этого вызова использовано
   `-A unused-imports`, чтобы не удалить импорты, используемые legacy consumers
   общих source-path файлов. Это не проверка с `-D warnings`.
   Лог: `/tmp/antex-checkpoint-clippy.log`; строк `Fixed` в нём нет.
5. `just bazel-lock-update` на deimos завершился; `MODULE.bazel.lock` совпадает
   локально и удалённо. Лог: `/tmp/antex-checkpoint-bazel.log`.
6. Локальный `just fmt` и `git diff --check` выполнены; локальной сборки не было.
   После fmt/fix тесты повторно не запускались согласно правилам репозитория.
7. Ранее пройдены Linux sandbox/security cases и Mac-target cross-Clippy runtime.
   Cross-Clippy не заменяет сборку/запуск всего продукта на macOS.

## Что НЕ проверено или не готово

- Реальный OAuth login + model turn + tool cycle с ChatGPT Plus/Pro. Предыдущий
  device code истёк без авторизации; его нельзя использовать снова. Чужие или
  fallback credentials не переносились. Работающего login-процесса не оставлено.
- Текущий полный продукт на macOS arm64, Mac clipboard/Seatbelt в реальном запуске.
- Финальные release size, startup latency, idle RSS, clean build time и LOC/crate
  budgets. Старый замер CLI 10.74 MiB был ДО TUI и не характеризует текущий бинарник.
- Полный argument-comment lint после последних TUI-переносов; ранее проходил
  только для более раннего core/provider/runtime/CLI среза.
- Полная эквивалентность старому UI: некоторые перенесённые keybindings ещё не
  подключены; крупные keymap/wrapping/custom_terminal требуют дальнейшего разбиения.
- Extensions end-to-end, credential isolation и restart/cancellation policy host.
- Deletion deny list, окончательный rename, installer/update и релизные артефакты.

## Как продолжить на deimos

Checkout: `/home/antonmoss/antex-work/codex`.
Tools: `/home/antonmoss/antex-tools` (Rust 1.95, just, nextest, clang 17,
bubblewrap 0.12, Bazel). Использовать helper: он задаёт PATH, CARGO_HOME,
RUSTUP_HOME и build flags. Рабочий каталог команды helper — `codex-rs`.

Из локального корня репозитория, только удалённое исполнение:

```sh
python3 scripts/antex-remote.py --tty \
  --host deimos.vla.yp-c.yandex.net \
  --checkout /home/antonmoss/antex-work/codex \
  --tools /home/antonmoss/antex-tools \
  env ANTEX_BWRAP=/home/antonmoss/antex-tools/bin/bwrap \
  just antex --home /home/antonmoss/antex-work/tui-acceptance.HiuwJF tui
```

Home smoke-проверки не содержит авторизованного аккаунта. Для реальной работы
нужно отдельно выполнить `login --device-code` с выбранным изолированным home
и лично пройти авторизацию. Сначала убедиться, что хранение аккаунта на этом
удалённом хосте приемлемо. Не копировать данные установленного fallback.

Для тестов убрать `--tty`, заменить `just antex ...` на:

```sh
just test --manifest-path antex/Cargo.toml \
  -p antex-core -p antex-provider-openai -p antex-runtime \
  -p antex-tui -p antex-cli -p antex-extension-protocol
```

Проверенный бинарник: `codex-rs/antex/target/debug/antex` в удалённом checkout.
Tmux server: `antex-smoke-clean`, session `tui-smoke`; checkpoint window 1
оставлена dead/exit 0 с remain-on-exit для просмотра, не с работающим агентом.

## Git и сохранность данных

- До финальной фиксации checkpoint: `00315cafb2`; дополнительные коммиты:
  `7ca2a11e27` (форматирование pager Escape regression), `5023dc411f`
  (protocol scaffold). Этот документ и PLAN записаны следующим отдельным коммитом.
- На момент подготовки handoff локальный `origin/antex` указывает на
  `00315cafb2`. Финальные checkpoint-коммиты не пушились. Проверять актуальное
  состояние перед следующим push; merge/release/rename не выполнялись.
- Remote Git HEAD ранее был `e02200b228`; тестировалось более свежее дерево,
  синхронизированное файлами, а не этот старый коммит сам по себе. Не считать
  удалённый checkout чистым и не делать reset. Перед pull сохранить/сопоставить
  remote diff и generated files. Старые validation stashes оставлены намеренно.
- Пользовательский untracked `research/` не изменялся и не включён в коммиты.
- `/Users/antonmoss/.local/bin/codex` сохранён; SHA256 повторно проверен:
  `be16a880b76ea5c6d4a38e61ff1f4fa86de15306078513d639cc1716df3528b2`.
  Локальный 0.0.14 остаётся fallback; публикация отсутствующего Release не нужна.

## Следующий небольшой этап

Сначала проверить checkpoint с настоящим аккаунтом и согласовать достаточный
минимум TUI parity, прежде чем переключать default entry. Затем отдельно
реализовывать extension host и conformance fixtures, не смешивая это с удалением
legacy. Host должен явно проверять origin Agent actions и получать sandbox
launcher из composition root; нельзя занять protocol stdin тем же FD, которым
shell сейчас передаёт seccomp filter. После этого — first-party extensions,
удаление bridges/legacy, budgets, rename и release gates в порядке PLAN.

До нового запроса пользователя работа остановлена на этом checkpoint.
