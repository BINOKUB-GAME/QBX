// =========================================================================
// Nom du script       : src/interrupts.rs
// Numéro de révision  : Révision 0.14
// Description         : Gestionnaire d'interruptions (Exceptions x86, horloge, 
//                       et fonction de scrutation polling clavier PS/2 pour 
//                       contourner les limitations APIC/IOAPIC sous UEFI/OVMF).
// =========================================================================

use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::registers::control::Cr2;
use x86_64::instructions::port::Port;
use crate::{println, print, clavier_queue, klog};
use lazy_static::lazy_static;
use spin::Mutex;
use pic8259::ChainedPics;

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard,
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }

    fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);
        idt[InterruptIndex::Timer.as_usize()].set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_usize()].set_handler_fn(keyboard_interrupt_handler);
        idt
    };
}

// --- [FONCTION 1] Initialisation globale IDT, PIC et activation du port PS/2 ---
pub fn init() {
    IDT.load();
    unsafe {
        PICS.lock().initialize();
        PICS.lock().write_masks(0, 0);

        let mut command_port = Port::<u8>::new(0x64);
        let mut timeout = 100_000;
        while (command_port.read() & 2) != 0 && timeout > 0 {
            timeout -= 1;
        }
        command_port.write(0xAE); // Activer le port PS/2 1
    }
}

// --- [FONCTION 2] Scrutation clavier en mode Polling (contournement IOAPIC UEFI) ---
pub fn scruter_clavier_ps2() {
    let mut status_port = Port::<u8>::new(0x64);
    let mut data_port = Port::<u8>::new(0x60);

    unsafe {
        // Si le buffer de sortie du contrôleur PS/2 contient un octet prêt
        if (status_port.read() & 0x01) != 0 {
            let scancode = data_port.read();
            clavier_queue::ajouter_scancode(scancode);
        }
    }
}

// --- [FONCTION 3] Gestionnaire d'exception Breakpoint ---
extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

// --- [FONCTION 4] Gestionnaire d'interruption 14 : Page Fault #PF ---
extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    let adresse_fautive = Cr2::read();

    println!("\n================ ERREUR SYSTEME QBX ================");
    println!("Type              : Défaut de page (Page Fault #PF)");
    println!("Adresse fautive   : {:?}", adresse_fautive);
    println!("Code d'erreur     : {:?}", error_code);
    println!("Pointeur d'ordre  : {:#?}", stack_frame.instruction_pointer);
    println!("====================================================");

    klog!("[ERR] Defaut de page intercepte a {:?}", adresse_fautive);

    loop {
        x86_64::instructions::hlt();
    }
}

// --- [FONCTION 5] Gestionnaire d'interruption 32 : Horloge PIT (Timer) ---
extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
    crate::task::cadencer_preemption();
}

// --- [FONCTION 6] Gestionnaire d'interruption 33 : Clavier PS/2 ---
extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };

    clavier_queue::ajouter_scancode(scancode);

    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}
