#!/usr/bin/env python3
"""Package the static ARM64 build with a public example, never local secrets."""

import hashlib
import shutil
import struct
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main():
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    name = f"cts-departures-v{version}-freebox-aarch64"
    output = ROOT / "target" / f"release-v{version}"
    folder = output / name
    folder.mkdir(parents=True, exist_ok=True)
    binary = ROOT / "target/aarch64-unknown-linux-musl/release/cts-departures"
    data = binary.read_bytes()
    assert data[:6] == b"\x7fELF\x02\x01", "Expected a 64-bit little-endian ELF"
    assert struct.unpack_from("<H", data, 18)[0] == 183, "Expected ARM64"
    offset = struct.unpack_from("<Q", data, 32)[0]
    size, count = struct.unpack_from("<HH", data, 54)
    assert all(struct.unpack_from("<I", data, offset + i * size)[0] != 3 for i in range(count)), "Binary requires a dynamic loader"
    shutil.copy2(binary, folder / "cts-departures")
    (folder / "cts-departures").chmod(0o755)
    (folder / "config.example.toml").write_text('''# Copy to config.toml for a NEW installation only.
# Keep your existing config.toml when upgrading.
cts_api_token = "REPLACE_WITH_YOUR_CTS_TOKEN"
cts_monitoring_ref = ["298B", "298A"]
cts_polling_interval_minutes = 2
cts_max_stop_visits = 10
cts_simulation = false
cts_always_query = true
listen_addr = "127.0.0.1:3000"

pixoo64_enabled = true
pixoo64_address = "192.168.1.42:80"
pixoo64_simulation = false
pixoo64_lines_per_screen = 4
pixoo64_tram_screen_seconds = 6
pixoo64_moment_screen_seconds = 2

# Weather remains optional for the web UI, and is never drawn on the PIXOO64.
meteoblue_enabled = false
birthday_enabled = false
jour_j_enabled = false
''')
    (folder / "INSTALLATION.md").write_text(f'''# CTS Departures {version} — Freebox ARM64

Binaire Linux aarch64 statique (musl), sans dépendance à la glibc de la VM.
Cette archive ne contient aucune configuration personnelle ni clé API.

## Mise à jour d'une installation existante

1. Vérifier l'archive téléchargée avec le fichier SHA256SUMS joint à la release.
2. Extraire l'archive puis vérifier son contenu : `sha256sum -c SHA256SUMS`.
3. Sauvegarder le binaire actuel de l'instance concernée.
4. Copier uniquement `cts-departures` vers cette instance, par exemple avec
   `scp cts-departures user@freebox:~/cts-jaures/cts-departures.new`.
5. Sur la VM, dans ce dossier, reprendre les permissions et le groupe du binaire
   existant avant le remplacement :

```sh
chmod --reference=cts-departures cts-departures.new
chgrp --reference=cts-departures cts-departures.new
mv cts-departures.new cts-departures
sudo systemctl restart cts-jaures
systemctl is-active cts-jaures
```

Adapter le compte, le dossier et le nom du service à l'installation.
Conserver `config.toml`, les anniversaires, les clés et les protections du service.
Un compte dédié au service doit garder le droit d'exécuter le nouveau binaire.

## Nouvelle installation

Copier `config.example.toml` en `config.toml`, saisir son propre jeton CTS,
les arrêts et l'adresse du PIXOO64. Le serveur écoute par défaut sur la boucle
locale ; configurer un reverse proxy pour l'accès web selon les besoins.
Protéger le fichier de configuration car il contient le jeton CTS.
Lancer `./cts-departures config.toml`, puis configurer son service systemd.

## Configuration du cycle depuis le navigateur

Ouvrir Configuration → Pixoo64, après Arrêt et Jour J. Cet onglet et son API
ne sont disponibles que si `pixoo64_enabled = true` sur l’instance.
Choisir Station, Strasbourg miniature, une alternance par tournée ou par quai,
puis personnaliser chaque étape : écran, quai et durée (1 à 300 secondes).
Les modèles et le cycle suivent les quais sélectionnés dans Arrêt, même avec
un seul quai. Les étapes des quais retirés sont omises. Si tous les quais sont
remplacés, la première étape de chaque design s’adapte aux nouveaux quais.
Enregistrer et appliquer conserve le cycle dans `config.toml` et l’applique
immédiatement, sans redémarrage. Le compte du service doit pouvoir écrire
ce fichier. L’aperçu montre le cycle enregistré, pas les modifications en cours.

## Cycle initial Station / Strasbourg miniature

Cycle : Station A → Station B → anniversaires → événements → miniature A →
miniature B → anniversaires → événements, puis retour à Station A.
Les quais suivent l'ordre configuré ; seuls les moments activés sont affichés.
Station conserve ses 1 à 4 lignes et son mini tram illustrant le temps d'attente.
Chaque miniature affiche les prochains C et D de son quai uniquement, dans deux
colonnes fixes, avec le prochain tram et son compteur à la couleur de sa ligne.
Sa cathédrale et sa lumière évoluent entre aube, jour, coucher du soleil et nuit.
Le cycle solaire est calculé localement pour Strasbourg, sans service météo.
Sans cycle personnalisé, la durée de base reste réglée par `pixoo64_tram_screen_seconds`.
Le quai Jean Jaurès vers Gare Centrale / Poteries (référence CTS 298B) reste
visible deux fois plus longtemps dans les deux designs : 12 s contre 6 s par défaut.
Après enregistrement d’un cycle, chaque étape utilise exactement sa durée.
Les anniversaires et événements sans contenu sont ignorés.
Configurer les deux quais d'une station pour obtenir les deux directions ;
l'exemple inclut les références 298B et 298A de Jean Jaurès.
La météo n'occupe plus de zone sur le PIXOO64 ; l'interface web est conservée.
''')
    shutil.copy2(ROOT / "LICENSE", folder / "LICENSE")
    expected = {"cts-departures", "config.example.toml", "INSTALLATION.md", "LICENSE", "SHA256SUMS"}
    assert {p.name for p in folder.iterdir()} <= expected, "Unexpected distribution content"
    entries = sorted(path for path in folder.iterdir() if path.name != "SHA256SUMS")
    sums = ''.join(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n" for path in entries)
    (folder / "SHA256SUMS").write_text(sums)
    archive = output / f"{name}.zip"
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as bundle:
        for path in sorted(folder.iterdir()):
            bundle.write(path, arcname=f"{name}/{path.name}")
    with zipfile.ZipFile(archive) as bundle:
        assert bundle.testzip() is None
        assert len(bundle.namelist()) == len(expected)
    (output / "SHA256SUMS").write_text(f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n")
    print(f"Prepared {archive} ({archive.stat().st_size:,} bytes)")
    print("Contents: " + ", ".join(sorted(expected)))


if __name__ == "__main__":
    main()
