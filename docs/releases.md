# Releases

## Building the packages

From the project folder:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/release.ps1
```

This builds the NSIS installer, the MSI and a portable ZIP, and puts them in a new folder under `target\releases`. Add `-NsisOnly` to skip the MSI.

Set the AniList and MyAnimeList client IDs before you run it, or people won't be able to log in (see [Setup](setup.md#logging-in)).

## Signing

To sign the packages, set `PATCHOULI_SIGNING_THUMBPRINT` to the thumbprint of an Authenticode certificate in your Windows certificate store, with its private key. Without it you get unsigned packages, and Windows SmartScreen warns people when they run the installer.

## Updates

Installed copies look for a new version when they start, if "Check for updates automatically" is on, and you can check by hand under Settings › Application. For the updater to accept a release, the installers have to be signed with the updater key:

1. Set `TAURI_SIGNING_PRIVATE_KEY` to the path of the updater's private key, and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to its password.
2. Run the script with `-SignedUpdates`. This also writes the signatures and `latest.json`.
3. Make a public GitHub release tagged `v<version>` and attach the installers, signatures, ZIP and `latest.json`.

The script points the updater at `oguzaliarslan/patchouli`. Use `-Repository owner/repo` to build for a different repository.

People using the portable ZIP update by closing the app and unzipping the new version over the old folder. Keep the `data` folder.

## Data that updates by itself

The daily check also downloads the latest episode relations and player list from their upstream projects. If that fails, the app keeps using the last copy it got, or the one it shipped with. A new player list takes effect after a restart. If you put your own `anime-relations.txt` in the data folder, that one is used instead.

## Installer artwork

The images in `src-tauri/installer/` are drawn by `scripts/installer_images.py`. To change them, edit the script and run `python scripts/installer_images.py` (you need Pillow).
