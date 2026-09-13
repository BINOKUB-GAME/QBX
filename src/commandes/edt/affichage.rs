// QBX EDT Affichage - Révision 1.1
// Fichier : src/commandes/edt/affichage.rs
// Description : Rendu direct en mémoire VGA (0xb8000) avec indicateur L/C, flash de sauvegarde et ligne de commande interactive

use crate::commandes::edt::moteur::Editeur;

pub const LARGEUR: usize = 80;
pub const HAUTEUR_EDIT: usize = 23;
const VGA_BUFFER: *mut u8 = 0xb8000 as *mut u8;

// --- [FONCTION 1 : ecrire_vga] ---
// Description : Écrit directement un octet et sa couleur aux coordonnées (x, y) en mémoire vidéo.
pub fn ecrire_vga(x: usize, y: usize, octet: u8, couleur: u8) {
    let offset = (y * LARGEUR + x) * 2;
    unsafe {
        *VGA_BUFFER.add(offset) = octet;
        *VGA_BUFFER.add(offset + 1) = couleur;
    }
}

// --- [FONCTION 2 : effacer_ecran_complet] ---
// Description : Balaye l'intégralité des 25 lignes de l'écran pour le remplir d'espaces.
pub fn effacer_ecran_complet() {
    for y in 0..25 {
        for x in 0..LARGEUR {
            ecrire_vga(x, y, b' ', 0x07);
        }
    }
}

// --- [FONCTION 3 : dessiner_barre_statut] ---
// Description : Trace la ligne de statut ou la ligne de commande interactive selon le mode de l'éditeur.
pub fn dessiner_barre_statut(editeur: &Editeur, nom_fichier: &str, ligne: usize, col: usize) {
    for x in 0..LARGEUR {
        ecrire_vga(x, 24, b'-', 0x07);
    }

    let mut pos = 0;

    if editeur.en_mode_commande {
        // Mode Commande actif (style vi sur la ligne du bas)
        let prompt = ":";
        for b in prompt.bytes() {
            if pos < LARGEUR {
                ecrire_vga(pos, 24, b, 0x0e); // Jaune vif pour le prompt
                pos += 1;
            }
        }
        for &b in &editeur.tampon_commande {
            if pos < LARGEUR {
                ecrire_vga(pos, 24, b, 0x0f); // Blanc éclatant pour la saisie
                pos += 1;
            }
        }
    } else if editeur.sauvegarde_flash {
        let entete = " [ Sauvegarde avec succes ! ] ";
        for b in entete.bytes() {
            if pos < LARGEUR {
                ecrire_vga(pos, 24, b, 0x0a);
                pos += 1;
            }
        }
    } else {
        let entete = alloc::format!(" L:{} C:{} | Fichier: ", ligne + 1, col + 1);
        for b in entete.bytes() {
            if pos < LARGEUR {
                ecrire_vga(pos, 24, b, 0x0f);
                pos += 1;
            }
        }
        for b in nom_fichier.bytes() {
            if pos < LARGEUR {
                ecrire_vga(pos, 24, b, 0x0e);
                pos += 1;
            }
        }
        let suite = " | [F2] Enregistrer | [ESC] Quitter";
        for b in suite.bytes() {
            if pos < LARGEUR {
                ecrire_vga(pos, 24, b, 0x0f);
                pos += 1;
            }
        }
    }

    while pos < LARGEUR {
        ecrire_vga(pos, 24, b' ', 0x0f);
        pos += 1;
    }
}

// --- [FONCTION 4 : dessiner_interface] ---
// Description : Efface l'écran, calcule la position du curseur (L/C) et restitue le texte visible.
pub fn dessiner_interface(editeur: &Editeur) {
    effacer_ecran_complet();

    let nom = if editeur.taille_nom > 0 {
        core::str::from_utf8(&editeur.nom_fichier[..editeur.taille_nom]).unwrap_or("inconnu")
    } else {
        "[Sans nom]"
    };

    let mut ligne = 0;
    let mut col = 0;
    for i in 0..editeur.curseur_pos.min(editeur.tampon.len()) {
        if editeur.tampon[i] == b'\n' {
            ligne += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    
    dessiner_barre_statut(editeur, nom, ligne, col);

    let mut byte_debut_affichage = 0;
    let mut lignes_compteur = 0;
    
    if editeur.ligne_debut > 0 {
        for (i, &b) in editeur.tampon.iter().enumerate() {
            if lignes_compteur == editeur.ligne_debut {
                byte_debut_affichage = i;
                break;
            }
            if b == b'\n' {
                lignes_compteur += 1;
            }
            byte_debut_affichage = i + 1;
        }
    }

    let mut screen_x = 0;
    let mut screen_y = 0;
    let mut abs_line_num = editeur.ligne_debut;

    if editeur.option_l && screen_y < HAUTEUR_EDIT {
        let num_str = if abs_line_num + 1 < 10 {
            alloc::format!("{}  ", abs_line_num + 1)
        } else if abs_line_num + 1 < 100 {
            alloc::format!("{} ", abs_line_num + 1)
        } else {
            alloc::format!("{}", abs_line_num + 1)
        };
        for &b in num_str.as_bytes() {
            ecrire_vga(screen_x, screen_y, b, 0x08);
            screen_x += 1;
        }
    }

    let (sel_min, sel_max) = if let Some(debut) = editeur.selection_debut {
        (debut.min(editeur.curseur_pos), debut.max(editeur.curseur_pos))
    } else {
        (0, 0)
    };
    let en_selection = editeur.selection_debut.is_some();

    for (i, &byte) in editeur.tampon.iter().enumerate().skip(byte_debut_affichage) {
        let est_curseur = i == editeur.curseur_pos && !editeur.en_mode_commande;
        let dans_bloc = en_selection && i >= sel_min && i < sel_max;
        
        let couleur = if est_curseur {
            0x70
        } else if dans_bloc {
            0x17
        } else {
            0x07
        };

        if byte == b'\n' {
            if est_curseur {
                ecrire_vga(screen_x, screen_y, b' ', 0x70);
            }
            screen_x = 0;
            screen_y += 1;
            abs_line_num += 1;

            if screen_y >= HAUTEUR_EDIT {
                break;
            }

            if editeur.option_l {
                let num_str = if abs_line_num + 1 < 10 {
                    alloc::format!("{}  ", abs_line_num + 1)
                } else if abs_line_num + 1 < 100 {
                    alloc::format!("{} ", abs_line_num + 1)
                } else {
                    alloc::format!("{}", abs_line_num + 1)
                };
                for &b in num_str.as_bytes() {
                    ecrire_vga(screen_x, screen_y, b, 0x08);
                    screen_x += 1;
                }
            }
        } else {
            ecrire_vga(screen_x, screen_y, byte, couleur);
            screen_x += 1;
            if screen_x >= LARGEUR {
                screen_x = 0;
                screen_y += 1;
            }
        }

        if screen_y >= HAUTEUR_EDIT {
            break;
        }
    }

    if editeur.curseur_pos >= editeur.tampon.len() && screen_y < HAUTEUR_EDIT && !editeur.en_mode_commande {
        ecrire_vga(screen_x, screen_y, b'_', 0x0f);
    }
}
