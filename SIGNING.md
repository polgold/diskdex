# Firma y notarización (macOS)

El `.dmg` que genera `npm run tauri build` sale **sin firmar**. Funciona en la Mac
donde lo compilás, pero en **otra** Mac Gatekeeper lo bloquea ("app de un
desarrollador no identificado" / "está dañada"). Para distribuirlo sin fricción
hace falta un **Apple Developer ID** y notarización.

> ⚠️ El Apple Developer ID NO se puede generar por script: es una inscripción al
> **Apple Developer Program** (US$99/año) atada a tu Apple ID e identidad. Hay
> que hacerla a mano una vez. Una vez inscripto, todo lo de abajo es automático.

## 1. Inscribirse y crear el certificado (una vez)

1. Inscribite en https://developer.apple.com/programs/ (US$99/año).
2. En Xcode → Settings → Accounts → agregá tu Apple ID → **Manage Certificates**
   → `+` → **Developer ID Application**. (O desde developer.apple.com → Certificates.)
3. Verificá que quedó en el llavero:
   ```bash
   security find-identity -v -p codesigning
   # debe listar: "Developer ID Application: Tu Nombre (TEAMID)"
   ```

## 2. Credenciales de notarización (una vez)

Generá una **app-specific password** en https://appleid.apple.com (sección
Seguridad) y guardá un perfil de notarización:
```bash
xcrun notarytool store-credentials diskdex-notary \
  --apple-id "TU_APPLE_ID@example.com" \
  --team-id "TEAMID" \
  --password "xxxx-xxxx-xxxx-xxxx"   # app-specific password
```

## 3. Build firmado + notarizado

Tauri firma y notariza solo si encuentra estas variables de entorno:
```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: Tu Nombre (TEAMID)"
export APPLE_ID="TU_APPLE_ID@example.com"
export APPLE_PASSWORD="xxxx-xxxx-xxxx-xxxx"   # la app-specific password
export APPLE_TEAM_ID="TEAMID"

CI=true npm run tauri build
```
Tauri firma el `.app`, lo notariza con Apple y le hace *staple* del ticket. El
`.dmg` resultante abre sin advertencias en cualquier Mac.

## Mientras tanto (sin Developer ID)

- Para **probar local** en tu propia Mac: `npm run tauri dev` (no necesita firma).
- Si pasás el `.dmg` sin firmar a otra persona, que lo abra con
  **clic derecho → Abrir** (acepta una vez), o que corra:
  ```bash
  xattr -dr com.apple.quarantine /Applications/DiskDex.app
  ```
