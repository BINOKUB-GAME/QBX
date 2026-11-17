#!/usr/bin/env bash
# ==============================================================================
# Nom du script : build_image.sh
# Révision : 1.0
# Description : Génération d'une image disque GPT, formatage de la partition
#               EFI (FAT32), et intégration de Limine et du noyau QBX.
# Intégrations : 
#   1. creation_image_disque - Alloue une image brute de 1 Go.
#   2. partitionnement_gpt   - Configure la table GPT et la partition EFI.
#   3. formatage_efi         - Formate la partition en FAT32.
#   4. integration_fichiers  - Copie Limine, limine.conf et le noyau QBX.
# ==============================================================================

set -e

IMAGE_NAME="disk.img"
IMAGE_SIZE="1G"
MOUNT_DIR="./mnt_efi"

# Fonction 1 : Création de l'image disque brute
creation_image_disque() {
    echo "[1/4] Création de l'image disque ${IMAGE_NAME} (${IMAGE_SIZE})..."
    rm -f "${IMAGE_NAME}"
    qemu-img create -f raw "${IMAGE_NAME}" "${IMAGE_SIZE}"
}

# Fonction 2 : Partitionnement GPT avec une partition EFI
partitionnement_gpt() {
    echo "[2/4] Application de la table de partitions GPT..."
    parted -s "${IMAGE_NAME}" mklabel gpt
    parted -s "${IMAGE_NAME}" mkpart ESP fat32 2048s 262143s
    parted -s "${IMAGE_NAME}" set 1 esp on
}

# Fonction 3 : Formatage de la partition EFI en FAT32 via mtools/loop
formatage_efi() {
    echo "[3/4] Formatage de la partition EFI en FAT32..."
    # Utilisation d'un offset de 2048 secteurs * 512 octets = 1048576 octets
    OFFSET=$((2048 * 512))
    SIZE=$(((262143 - 2048 + 1) * 512))
    
    # Création d'un sous-fichier représentant uniquement la partition EFI pour le formatage
    dd if="${IMAGE_NAME}" of=efi_part.img bs=512 skip=2048 count=$((262143 - 2048 + 1)) status=none
    mkfs.fat -F 32 efi_part.img
    dd if=efi_part.img of="${IMAGE_NAME}" bs=512 seek=2048 count=$((262143 - 2048 + 1)) conv=notrunc status=none
    rm -f efi_part.img
}

# Fonction 4 : Copie des fichiers de boot (Limine, config, noyau)
integration_fichiers() {
    echo "[4/4] Intégration des fichiers dans la partition EFI..."
    
    # Utilisation de mtools pour manipuler l'image FAT32 sans droits root complexes
    export MTOOLS_SKIP_CHECK=1
    OFFSET=$((2048 * 512))
    
    # Création des dossiers nécessaires
    mmd -i "${IMAGE_NAME}"@@${OFFSET} ::/EFI
    mmd -i "${IMAGE_NAME}"@@${OFFSET} ::/EFI/BOOT
    mmd -i "${IMAGE_NAME}"@@${OFFSET} ::/boot

    # Copie du binaire EFI de Limine (assure-toi d'avoir le fichier BOOTX64.EFI localement)
    if [ -f "BOOTX64.EFI" ]; then
        mcopy -i "${IMAGE_NAME}"@@${OFFSET} BOOTX64.EFI ::/EFI/BOOT/
    else
        echo "[AVERTISSEMENT] BOOTX64.EFI introuvable à la racine. Télécharge-le depuis les releases de Limine."
    fi

    # Copie du fichier de configuration Limine
    if [ -f "limine.conf" ]; then
        mcopy -i "${IMAGE_NAME}"@@${OFFSET} limine.conf ::/
    fi

    # Copie du noyau QBX compilé
    if [ -f "target/x86_64-unknown-none/debug/qbx" ]; then
        mcopy -i "${IMAGE_NAME}"@@${OFFSET} target/x86_64-unknown-none/debug/qbx ::/boot/qbx
    else
        echo "[ERREUR] Le noyau QBX compilé est introuvable. Exécute d'abord cargo build."
        exit 1
    fi

    echo "Image disque GPT/UEFI prête avec succès !"
}

# --- Exécution séquentielle ---
creation_image_disque
partitionnement_gpt
formatage_efi
integration_fichiers
