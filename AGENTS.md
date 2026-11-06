# AGENTS.md - Protocolo y Gobernanza Multi-Agente para Pomotroid

Este documento define la gobernanza, límites de contexto, contratos de interfaz y criterios de parada para los subagentes autónomos que operan en el entorno **Antigravity**. Todo agente debe operar estrictamente dentro de sus límites para prevenir colisiones de escritura, saturación de contexto y regresiones en la integración Rust-Tauri-Svelte.

---

## 1. Topología del Proyecto y Stack Tecnológico

- **Frontend:** SvelteKit 2 + Svelte 5 (sintaxis de runes: `$state`, `$derived`, `$props`), TypeScript 6, Vite 8, `@inlang/paraglide-js` (i18n tipado), CSS Custom Properties dinámicas.
- **Desktop Core (Backend):** Tauri v2 (`@tauri-apps/api`, plugins oficiales), Rust (Edición 2021), Tokio, Axum, Rodio/Symphonia, Tiny-Skia, Notify, Rusqlite (SQLite embebido).
- **Headless Server:** Subproyecto independiente en `server/` (Rust + Axum + Tokio + Rusqlite).
- **Estructura de Directorios Clave:**
  - `src/`: UI, rutas (`main`, `settings`, `stats`, `mini`), componentes, stores y cliente IPC.
  - `src-tauri/`: Núcleo de Tauri, comandos Rust (`commands.rs`), permisos (`capabilities/`), temporizador, audio, tray y base de datos.
  - `static/`: Temas JSON (`static/themes/`), assets de audio (.wav) e iconos.
  - `project.inlang/`: Catálogos multilingües de Paraglide.
  - `server/`: Servidor daemon auxiliar.
  - `scripts/`: Herramientas de despliegue, empaquetado y seeders.

---

## 2. Matriz de Roles y Límites de Contexto

Los subagentes tienen estrictamente acotado su alcance de lectura y escritura. Queda terminantemente prohibido alterar archivos fuera del rango asignado.

| Rol | Responsabilidad | Lectura Permitida | Modificación Permitida | Límites Estrictos (Prohibido) |
| :--- | :--- | :--- | :--- | :--- |
| **`architect`** | Orquestación, análisis de impacto, desglose funcional y diseño de arquitectura. | Todo el repositorio. | Planes técnicos, `AGENTS.md`, especificaciones en `openspec/`. | **No modifica código ejecutable** en `src/`, `src-tauri/` ni `server/`. |
| **`frontend-dev`** | Vistas y componentes Svelte 5, estado reactivo, temas CSS e i18n. | `src/**/*`, `static/**/*`, `project.inlang/**/*`, `package.json`, `CLAUDE.md`. | `src/routes/**/*`, `src/lib/components/**/*`, `src/lib/stores/**/*`, `src/app.css`, `static/themes/**/*`, `project.inlang/**/*`. | **No toca `src-tauri/` ni `server/`**. No llama directamente a `@tauri-apps/api/core::invoke()` fuera de `src/lib/ipc/index.ts`. |
| **`backend-dev`** | Motor del timer, audio Rodio, tray icon Tiny-Skia, WebSocket Axum, atajos. | `src-tauri/**/*`, `src/lib/types.ts`, `src/lib/ipc/**/*`, `server/**/*`. | `src-tauri/src/**/*` (excepto migraciones históricas), `src-tauri/Cargo.toml`, `src-tauri/capabilities/**/*`, `server/src/**/*`. | **No modifica rutas ni componentes Svelte** en `src/routes/` o `src/lib/components/`. |
| **`data-contracts`** | Esquemas SQLite, tipos compartidos Rust-TS y wrappers de comandos IPC. | `src-tauri/src/db/**/*`, `src-tauri/src/settings/**/*`, `src-tauri/src/commands.rs`, `src/lib/types.ts`, `src/lib/ipc/**/*`. | `src/lib/types.ts`, `src/lib/ipc/index.ts`, `src-tauri/src/db/migrations.rs` (solo agregar nuevas), `src-tauri/src/db/queries.rs`, `src-tauri/src/settings/defaults.rs`. | **No altera migraciones históricas** aplicadas ni modifica componentes visuales. |
| **`qa-tester`** | Validación de compilación, ejecución de tests, linters y comprobación de contratos. | Todo el repositorio. | Archivos de prueba (`src-tauri/src/**/*test*`), scripts de chequeo. | **No altera código de producción** para hacer que los tests pasen artificialmente. |

---

## 3. Protocolo de Sincronización y Contratos de Datos (Anti-Drift)

Para garantizar consistencia y evitar discrepancias entre Rust, SQLite y TypeScript:

1. **Fuente de Verdad de Tipos:**
   - La verdad reside en las estructuras Rust (`src-tauri/src/settings/mod.rs`, `src-tauri/src/timer/mod.rs`, etc.) serializadas con `serde` (convención obligatoria: `snake_case`).
   - `src/lib/types.ts` es el único espejo autorizado en el Frontend (`TimerState`, `Settings`, `Theme`, `SessionRecord`). Cualquier cambio en Rust debe sincronizarse de inmediato en este archivo.

2. **Aislamiento Estricto de IPC:**
   - Ningún componente o ruta Svelte invocará `invoke()` o `listen()` directamente.
   - Todo comando Tauri debe estar encapsulado en una función fuertemente tipada dentro de `src/lib/ipc/index.ts`.

3. **Invariantes y Conversión de Unidades:**
   - **Tiempo:** En SQLite se almacena en **minutos** (`INTEGER`). En Rust (`Settings`) y en TypeScript (`src/lib/types.ts`) se maneja siempre en **segundos** (`u64` / `number`).
   - **Volumen:** En SQLite se almacena como entero de **`0` a `100`**. En Rust y TypeScript se normaliza como flotante de **`0.0` a `1.0`** (`f32` / `number`).

4. **Mapeo Clave/Valor en SQLite (Legacy Keys):**
   - Ciertas claves en SQLite difieren de los nombres en Rust (ej. en SQLite `work_rounds` mapea al campo `long_break_interval` en Rust).
   - Al agregar o modificar cualquier ajuste (`setting`), debe actualizarse obligatoriamente la tríada:
     1. Constante de clave en `src-tauri/src/settings/mod.rs`.
     2. Valor por defecto en `src-tauri/src/settings/defaults.rs`.
     3. Deserializador del comando `settings_set` en `src-tauri/src/commands.rs`.

---

## 4. Reglas de Operación y Restricciones Críticas

- **Bloqueo de Dependencias:** Está estrictamente prohibido ejecutar `cargo add` o `npm install <paquete>` sin autorización explícita del usuario en el hilo principal.
- **Inmutabilidad de Migraciones de BD:** Las constantes históricas en `src-tauri/src/db/migrations.rs` (`MIGRATION_1`, `MIGRATION_2`, etc.) son inmutables. Toda alteración de esquema debe plasmarse en una nueva constante `MIGRATION_N` y registrarse en la secuencia de `run()`.
- **Arquitectura de Ventana Frameless:** Pomotroid corre sin decoraciones de ventana nativas. Cualquier vista nueva debe respetar la barra de arrastre (`src/lib/components/Titlebar.svelte`) y la compatibilidad con el factor de escala dinámico (`uiScale` aplicado vía CSS `zoom`).
- **Svelte 5 Runes:** Usar exclusivamente sintaxis moderna de Svelte 5 (`$state()`, `$derived()`, `$effect()`, `$props()`). No usar la sintaxis obsoleta `let:`, `$:` o `export let`.
- **Formato de Commits:** Respetar la convención *Conventional Commits* (`feat:`, `fix:`, `refactor:`, `test:`, `chore:`).

---

## 5. Pipeline de Validación Obligatoria

Antes de dar por finalizada una tarea, el subagente correspondiente o el agente `qa-tester` debe ejecutar las verificaciones pertinentes según los archivos modificados:

### Verificación Frontend
```powershell
# 1. Compilación de traducciones (obligatorio si cambiaron strings en project.inlang)
npm run paraglide:compile

# 2. Comprobación de tipos SvelteKit y TypeScript
npm run check

# 3. Verificación de formato Prettier
npx prettier --check .

# 4. Build de producción estático del Frontend
npm run build
```

### Verificación Backend
```powershell
# 1. Chequeo de sintaxis y tipos en Rust (rápido)
cargo check --manifest-path src-tauri/Cargo.toml

# 2. Suite de pruebas unitarias
cargo test --manifest-path src-tauri/Cargo.toml

# 3. Linter Clippy (cero advertencias toleradas)
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

### Smoke Test de Integración
```powershell
# Compilación completa de la aplicación de escritorio en modo debug
npm run tauri build -- --debug
```

---

## 6. Criterios de Parada Inmediata (Escalación al Hilo Principal)

El subagente debe suspender su ejecución y devolver el control al usuario/hilo principal si ocurre cualquiera de los siguientes escenarios:

1. **Bucle de Compilación:** Persistencia de un error de `npm run check` o `cargo check` tras dos (2) intentos fallidos consecutivos de corrección.
2. **Ruptura Imprevista de IPC:** Una modificación en Rust altera una firma existente en `commands.rs` y requiere cambios en el frontend que no estaban definidos en el alcance de la tarea.
3. **Solicitud de Nuevas Dependencias:** La solución técnica requiere un crate externo de Rust o un paquete de npm no presente en los manifiestos actuales.
4. **Permisos de Tauri:** El cambio exige habilitar capacidades o permisos nuevos en `src-tauri/capabilities/default.json`.
5. **Conflicto en Esquemas de BD:** Se detecta necesidad de modificar o eliminar columnas en tablas SQLite con datos de sesiones o configuraciones existentes sin una estrategia de migración no destructiva.