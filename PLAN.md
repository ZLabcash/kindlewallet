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
- Lien Wi-Fi avec ZApp pour Zcash (code fait, à essayer sur la Kindle) : « Connect phone »
  allume le Wi-Fi derrière un pare-feu (seulement le port 7700 depuis le réseau local),
  QR d'appairage, canal chiffré (X25519 + HKDF + ChaCha20-Poly1305) qui porte le
  protocole zfirm ; vérification, pages de revue, « Hold to sign » (1,5 s). Wi-Fi coupé
  et pare-feu remis en quittant Zcash, au verrouillage, en veille, à la sortie.
  Test sur Mac : `kindlewallet --serve DIR --auto-approve`.
- Galerie zkSNARKs hors ligne (10 000 portraits, onglet « Mine » envoyé par ZApp) :
  `nft/build.py` prépare kindle/kindlewallet/nft.

## Étapes
1. Jailbreak : fait le 12/10/2026 avec SpiderCat (un livre à ouvrir, sans pubs).
   Mises à jour bloquées par le jailbreak.
2. Installation : copier kindle/kindlewallet et kindle/documents à la racine de la
   Kindle, ouvrir « KindleWallet » dans la bibliothèque (scriptlet ; KUAL ne sert plus).
   Si le toucher est décalé : /mnt/us/kindlewallet/touch.conf avec `swap`, `invx`, `invy`.
   Journal : /mnt/us/kindlewallet/log.txt.
3. Démarrage direct sur KindleWallet (mode dédié), pare-feu : seulement l'iPhone en Wi-Fi local.
4. Lien Wi-Fi local avec ZApp : Zcash fait (à vérifier sur la Kindle : Wi-Fi via lipc
   sans l'interface Amazon, iptables), puis BTC/ETH.
5. Restaurer depuis 24 mots (clavier à l'écran).
