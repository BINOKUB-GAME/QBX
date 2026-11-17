// QBX Centurion - Sous-système de stockage
// Fichier : src/storage/mod.rs
// Description : Gestion sécurisée et conditionnelle du sous-système de stockage

pub mod ahci;
pub mod partitions;

pub fn initialiser() {
    crate::klog!("[STR] Verification et initialisation du stockage...");
    
    // On appelle l'initialisation AHCI de manière sécurisée
    ahci::initialiser();
    
    crate::klog!("[STR] Sous-système de stockage initialise (mode sans echec si aucun disque).");
}
