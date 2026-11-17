# Windows releases

Run `powershell -ExecutionPolicy Bypass -File scripts/release.ps1` from the project root. It stages the x64 NSIS and MSI installers and a portable ZIP in a new folder under `target/releases`. Use `-NsisOnly` when building just the NSIS installer and ZIP.

Set `PATCHOULI_SIGNING_THUMBPRINT` to the thumbprint of an Authenticode certificate with its private key available in the Windows certificate store. Omit it to build unsigned Windows packages.

For an update release, set `TAURI_SIGNING_PRIVATE_KEY` to the path of the updater private key and provide its password through `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Run the release script with `-SignedUpdates` to generate installer signatures and `latest.json`. The updater requires signatures that bind each installer to the version in the manifest. Attach the generated installers, signatures, ZIP and manifest to the public GitHub release tagged `v<version>`. The script uses `oguzaliarslan/patchouli` by default. Use `-Repository owner/repo` to build for another release repository.

Installed copies can check for updates under **Settings ? Application**. Portable users can extract the new ZIP into their existing portable installation folder while the app is closed. Keep the existing `data` folder.

Automatic checks also refresh episode relations and player definitions daily from their upstream repositories. The app uses bundled data if the refresh fails and no prior cache exists. Updated player definitions apply on restart. A custom `anime-relations.txt` in the data folder takes precedence.
