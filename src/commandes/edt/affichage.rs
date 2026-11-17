// =========================================================================
// Nom du script       : src/commandes/edt/affichage.rs
// Numéro de révision  : Révision 2.1
// Description         : Rendu graphique plein écran de l'éditeur via le Framebuffer 
//                       adapté au zoom x2 et nettoyage complet à l'ouverture.
// =========================================================================

use crate::commandes::edt::moteur::Editeur;
use crate::vga_buffer;

pub const LARGEUR: usize = 50; // Nombre de colonnes adaptées au zoom x2
pub const HAUTEUR_EDIT: usize = 20; // Nombre de lignes visibles dans la zone d'édition

// --- [FONCTION 1 : vga_attr_to_argb] ---
// Description : Convertit les attributs de couleur VGA en ARGB 32-bit.
fn vga_attr_to_argb(attr: u8) -> (u32, u32) {
    let fg = attr & 0x0F;
    let bg = (attr >> 4) & 0x0F;

    let to_rgb = |c: u8, bright: bool| -> u32 {
        match c & 0x07 {
            0 => if bright { 0x00_55_55_55 } else { 0x00_00_00_00 },
            1 => if bright { 0x00_55_55_FF } else { 0x00_00_00_AA },
            2 => if bright { 0x00_00_FF_00 } else { 0x00_00_AA_00 }, // Vert terminal vif
            3 => if bright { 0x00_55_FF_FF } else { 0x00_00_AA_AA },
            4 => if bright { 0x00_FF_55_55 } else { 0x00_AA_00_00 },
            5 => if bright { 0x00_FF_55_FF } else { 0x00_AA_00_AA },
            6 => if bright { 0x00_FF_FF_55 } else { 0x00_AA_55_00 },
            7 => if bright { 0x00_FF_FF_FF } else { 0x00_AA_AA_AA },
            _ => 0x00_FF_FF_FF,
        }
    };

    (to_rgb(fg & 0x07, (fg & 0x08) != 0), to_rgb(bg & 0x07, false))
}

// --- [FONCTION 2 : ecrire_vga] ---
// Description : Dessine un caractère avec le zoom x2 aux coordonnées grille (x, y).
pub fn ecrire_vga(x: usize, y: usize, octet: u8, couleur: u8) {
    if x >= LARGEUR || y > HAUTEUR_EDIT + 1 { return; }
    let (fg, bg) = vga_attr_to_argb(couleur);

    vga_buffer::avec_framebuffer(|addr, width, height, pitch| {
        // Position avec décalage de marge (20 pixels) et taille zoomée (16x32 par caractère)
        let px = 20 + (x * 16);
        let py = 20 + (y * 32);
        let code = (octet as usize).min(vga_buffer::FONT_8X16.len() - 1);
        let glyph = &vga_buffer::FONT_8X16[code];

        unsafe {
            for (row, &byte_row) in glyph.iter().enumerate() {
                for col in 0..8 {
                    let bit_on = (byte_row & (1 << (7 - col))) != 0;
                    
                    // Application du zoom x2 (blocs de 2x2 pixels)
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let target_x = px + (col * 2) + dx;
                            let target_y = py + (row * 2) + dy;

                            if target_x < width && target_y < height {
                                let color = if bit_on { fg } else { bg };
                                *addr.add(target_y * pitch + target_x) = color;
                            }
                        }
                    }
                }
            }
        }
    });
}

// --- [FONCTION 3 : effacer_ecran_complet] ---
// Description : Efface toute la zone de l'éditeur en noir pour éviter les superpositions.
pub fn effacer_ecran_complet() {
    vga_buffer::avec_framebuffer(|addr, width, height, pitch| {
        unsafe {
            for y in 20..(height - 20) {
                for x in 20..(width - 20) {
                    *addr.add(y * pitch + x) = 0x00_00_00_00;
                }
            }
        }
    });
}

// --- [FONCTION 4 : dessiner_barre_statut] ---
// Description : Trace la barre de statut en bas de la zone d'édition.
pub fn dessiner_barre_statut(nom_fichier: &str, ligne: usize, col: usize, sauvegarde_flash: bool) {
    let statut_y = HAUTEUR_EDIT + 1;
    for x in 0..LARGEUR {
        ecrire_vga(x, statut_y, b'-', 0x07);
    }

    let mut pos = 0;
    let entete = if sauvegarde_flash {
        alloc::string::String::from(" [ Succes ! ] ")
    } else {
        alloc::format!(" L:{} C:{} | ", ligne + 1, col + 1)
    };

    for b in entete.bytes() {
        if pos < LARGEUR {
            ecrire_vga(pos, statut_y, b, if sauvegarde_flash { 0x0a } else { 0x0f });
            pos += 1;
        }
    }

    if !sauvegarde_flash {
        for b in nom_fichier.bytes() {
            if pos < LARGEUR {
                ecrire_vga(pos, statut_y, b, 0x0e);
                pos += 1;
            }
        }
        let suite = " | [F2]:Sauv [ESC]:Quit";
        for b in suite.bytes() {
            if pos < LARGEUR {
                ecrire_vga(pos, statut_y, b, 0x0f);
                pos += 1;
            }
        }
    }

    while pos < LARGEUR {
        ecrire_vga(pos, statut_y, b' ', 0x0f);
        pos += 1;
    }
}

// --- [FONCTION 5 : dessiner_interface] ---
// Description : Rafraîchit l'affichage complet de l'éditeur.
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
    
    dessiner_barre_statut(nom, ligne, col, editeur.sauvegarde_flash);

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
        let num_str = alloc::format!("{:2} ", abs_line_num + 1);
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
        let est_curseur = i == editeur.curseur_pos;
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
                let num_str = alloc::format!("{:2} ", abs_line_num + 1);
                for &b in num_str.as_bytes() {
                    ecrire_vga(screen_x, screen_y, b, 0x08);
                    screen_x += 1;
                }
            }
        } else {
            // Filtrer les retours chariot parasites
            if byte != b'\r' {
                ecrire_vga(screen_x, screen_y, byte, couleur);
                screen_x += 1;
                if screen_x >= LARGEUR {
                    screen_x = 0;
                    screen_y += 1;
                }
            }
        }

        if screen_y >= HAUTEUR_EDIT {
            break;
        }
    }

    if editeur.curseur_pos >= editeur.tampon.len() && screen_y < HAUTEUR_EDIT {
        ecrire_vga(screen_x, screen_y, b'_', 0x0f);
    }
}
