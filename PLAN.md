# KindleWallet — plan

Portefeuille hors ligne sur une Kindle jailbreakée (Kindle 10e gén. / Paperwhite 4,
logiciel 5.18.1, série G090VB…). Une phrase de 24 mots, créée sur la Kindle, donne :
Zcash shielded (Orchard, même code que ZFlip), Bitcoin (BIP 84, bc1q…), Ethereum
(BIP 44, 0x…). Phrase chiffrée par PIN (Argon2id + AES-256-GCM) dans
/mnt/us/kindlewallet/wallet.zkv.

## Fait
- Dérivations vérifiées contre les vecteurs officiels (tests/vectors.rs).
- Interface tactile : création, 24 mots, PIN, recevoir (QR + adresse, « New address »
  = adresse suivante, chaque ouverture d'une crypto = adresse neuve), mots, clé de
  lecture Zcash, verrouiller. Aperçu sur Mac : `./build.sh` → preview/*.png.
- Binaire Kindle statique ARMv7 + extension KUAL (kual/kindlewallet).

## Étapes
1. Jailbreak AdBreak (pubs activées, Kindle remplie pour bloquer les mises à jour).
2. KUAL + MRPI, copier kual/kindlewallet dans extensions/, lancer « KindleWallet wallet ».
   Si le toucher est décalé : /mnt/us/kindlewallet/touch.conf avec `swap`, `invx`, `invy`.
   Journal : /mnt/us/kindlewallet/log.txt.
3. Démarrage direct sur KindleWallet (mode dédié), pare-feu : seulement l'iPhone en Wi-Fi local.
4. Lien Wi-Fi local avec l'app iPhone (protocole ZFlip sur TCP, appairage par code) :
   Zcash d'abord (signature des envois), puis BTC/ETH.
5. Restaurer depuis 24 mots (clavier à l'écran).
