# KindleWallet — plan

Portefeuille hors ligne sur une Kindle jailbreakée (Kindle 10e gén., logiciel 5.18.1,
série G090VB…). Une phrase de 24 mots, créée sur la Kindle, donne 10 cryptos : BTC, ETH,
ZEC (shielded seulement), BNB, SOL, XMR, Robinhood Chain, Base, Arbitrum, Quantus.
Accueil : les logos, 3 par ligne ; chaque crypto a son écran (QR, nouvelle adresse,
et ses extras : clé de lecture Zcash, zpub Bitcoin, clé de vue et 25 mots Monero).
Phrase chiffrée par PIN (Argon2id + AES-256-GCM) dans /mnt/us/kindlewallet/wallet.zkv.

## Fait
- Dérivations vérifiées contre les vecteurs officiels (tests/vectors.rs).
- Interface tactile : création, 24 mots, PIN, recevoir (QR + adresse, « New address »
  = adresse suivante, chaque ouverture d'une crypto = adresse neuve), mots, clé de
  lecture Zcash, verrouiller. Aperçu sur Mac : `./build.sh` → preview/*.png.
- Binaire Kindle statique ARMv7 + extension KUAL (kual/kindlewallet).

## Étapes
1. Jailbreak : fait le 12/10/2026 avec SpiderCat (un livre à ouvrir, sans pubs).
   Mises à jour bloquées par le jailbreak.
2. Installation : copier kindle/kindlewallet et kindle/documents à la racine de la
   Kindle, ouvrir « KindleWallet » dans la bibliothèque (scriptlet ; KUAL ne sert plus).
   Si le toucher est décalé : /mnt/us/kindlewallet/touch.conf avec `swap`, `invx`, `invy`.
   Journal : /mnt/us/kindlewallet/log.txt.
3. Démarrage direct sur KindleWallet (mode dédié), pare-feu : seulement l'iPhone en Wi-Fi local.
4. Lien Wi-Fi local avec l'app iPhone (protocole ZFlip sur TCP, appairage par code) :
   Zcash d'abord (signature des envois), puis BTC/ETH.
5. Restaurer depuis 24 mots (clavier à l'écran).
