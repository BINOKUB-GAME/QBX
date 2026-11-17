// =========================================================================
// Nom du script       : src/main.rs
// Numéro de révision  : Révision 1.17
// Description         : Point d'entrée du noyau avec scrutation polling active,
//                       décodeur ScancodeSet1 et suppression du verrouillage 
//                       Caps Lock forcé pour permettre la saisie en minuscules.
// =========================================================================

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

// --- [MODULES INTERNES DU NOYAU] ---
mod vga_buffer;
mod interrupts;
mod shell;
mod power;
mod commandes;
mod clavier_queue;
pub mod fs;
pub mod allocator;
pub mod memory;
pub mod journal;
pub mod task;
pub mod session;
pub mod pci;
pub mod net;
pub mod storage;
pub mod scripting;
pub mod syscalls;

use core::panic::PanicInfo;
use x86_64::VirtAddr;
use memory::LimineFrameAllocator;
use pc_keyboard::{Keyboard, ScancodeSet1, layouts, HandleControl};
use spin::Mutex;
use lazy_static::lazy_static;
use limine::{BaseRevision, RequestsStartMarker, RequestsEndMarker};
use limine::request::{HhdmRequest, MemmapRequest, FramebufferRequest};

// --- [REQUÊTES ET MARQUEURS PROTOCOLE LIMINE] ---
#[used]
#[link_section = ".requests_start_marker"]
static REQUESTS_START: RequestsStartMarker = RequestsStartMarker::new();

#[used]
#[link_section = ".requests"]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[link_section = ".limine_requests"]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[link_section = ".limine_requests"]
static MEMMAP_REQUEST: MemmapRequest = MemmapRequest::new();

#[used]
#[link_section = ".limine_requests"]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[link_section = ".requests_end_marker"]
static REQUESTS_END: RequestsEndMarker = RequestsEndMarker::new();

// --- [POINT D'ENTRÉE DU NOYAU] ---
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // 1. Initialisation immédiate du Framebuffer et affichage d'une barre ROUGE en haut
    if let Some(fb_response) = FRAMEBUFFER_REQUEST.response() {
        if let Some(fb) = fb_response.framebuffers().first() {
            let ptr = fb.address() as *mut u32;
            let pitch = fb.pitch as usize / 4;
            unsafe {
                for y in 0..40 {
                    for x in 0..fb.width as usize {
                        *ptr.add(y * pitch + x) = 0x00_FF_00_00;
                    }
                }
            }
            vga_buffer::init_framebuffer(
                ptr,
                fb.width as usize,
                fb.height as usize,
                fb.pitch as usize,
            );
        }
    }

    assert!(BASE_REVISION.is_supported(), "Erreur : Version de protocole Limine non supportée !");

    let hhdm_response = HHDM_REQUEST.response().expect("Erreur : HHDM request non fournie par Limine");
    let mem_map_response = MEMMAP_REQUEST.response().expect("Erreur : Memmap request non fournie par Limine");

    println!("  ____  ____  __  __");
    println!(" / __ \\|  _ \\ \\ \\/ /");
    println!("| |  | | |_) ) >  < ");
    println!("| |__| |  _ < / /\\ \\");
    println!(" \\___\\_\\____//_/  \\_\\");
    println!("=== QBX - EXP (Québec UNIX) v0.1 ===");
    println!("Initialisation du système (UEFI / Limine / Framebuffer)...\n");

    // 2. Initialisation globale IDT, PIC et démasquage des IRQs matérielles
    interrupts::init();

    // 3. Initialisation pagination physique et virtuelle (HHDM)
    let phys_mem_offset = VirtAddr::new(hhdm_response.offset);
    memory::enregistrer_offset_physique(hhdm_response.offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    
    let memory_map = mem_map_response.entries();
    let mut frame_allocator = unsafe { LimineFrameAllocator::init(memory_map) };

    // 4. Montage du tas initial de 2 Mio
    if let Err(e) = allocator::init_heap(&mut mapper, &mut frame_allocator) {
        panic!("Échec de l'initialisation du Heap : {:?}", e);
    }

    // 5. Contexte mémoire global
    memory::init_contexte(mapper, frame_allocator);
    println!("Heap 2 MiB : OK");

    // 6. Initialisation du sous-système multitâche
    task::initialiser();

    // 7.0 Détection matérielle PCI et réseau
    pci::initialiser();

    // 7.1 Stockage et session
    storage::initialiser();
    session::charger_depuis_disque();
    journal::charger_depuis_disque();
    fs::charger_depuis_disque();

    // 8. Réseau
    net::initialiser();

    // Journalisation des étapes
    crate::klog!("[KRN] QBX Microkernel v0.1 démarre (UEFI/Limine/Framebuffer)");
    crate::klog!("[CPU] Initialisation IDT, PIC et démasquage IRQ terminés");
    crate::klog!("[MEM] Pagination physique et virtuelle initialisée");
    crate::klog!("[ALC] Heap de 2 MiB initialisé avec succès");
    crate::klog!("[VFS] Système de fichiers en mémoire monté sur '/'");
    crate::klog!("[SHL] Shell interactif initialisé");

    println!("Système prêt.\n");
    print!("qbx:{}{} ", fs::chemin_actuel(), session::symbole_prompt());

    // Activation des interruptions matérielles
    x86_64::instructions::interrupts::enable();

    // --- [BOUCLE DE GESTION DU SHELL - POLLING ACTIF] ---
    loop {
        // Scrutation active permanente du port clavier PS/2
        interrupts::scruter_clavier_ps2();

        let scancode_opt = x86_64::instructions::interrupts::without_interrupts(|| {
            clavier_queue::SCANCODE_QUEUE.lock().pop()
        });

        if let Some(scancode) = scancode_opt {
            let mut kb = KEYBOARD_DECODER.lock();
            if let Ok(Some(key_event)) = kb.add_byte(scancode) {
                if let Some(key) = kb.process_keyevent(key_event) {
                    let editeur_actif = commandes::edt::EDITEUR.lock().est_actif();

                    if editeur_actif {
                        commandes::edt::EDITEUR.lock().traiter_touche(key);
                    } else {
                        match key {
                            pc_keyboard::DecodedKey::Unicode(character) => {
                                shell::SHELL.lock().introduire_caractere(character);
                            }
                            pc_keyboard::DecodedKey::RawKey(key_code) => match key_code {
                                pc_keyboard::KeyCode::ArrowUp => shell::SHELL.lock().historique_precedent(),
                                pc_keyboard::KeyCode::ArrowDown => shell::SHELL.lock().historique_suivant(),
                                _ => {}
                            }
                        }
                    }
                }
            }
        } else {
            core::hint::spin_loop();
        }
    }
}

#[panic_handler]
fn gestionnaire_panic(information: &PanicInfo) -> ! {
    println!("{}", information);
    loop {
        x86_64::instructions::hlt();
    }
}

lazy_static! {
    static ref KEYBOARD_DECODER: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> =
        Mutex::new(Keyboard::new(
            ScancodeSet1::new(),
            layouts::Us104Key,
            HandleControl::Ignore,
        ));
}
