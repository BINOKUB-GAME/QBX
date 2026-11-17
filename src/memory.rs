// =========================================================================
// Nom du script       : src/memory.rs
// Numéro de révision  : Révision 0.7
// Description         : Gestionnaire de pagination, allocateur de frames 
//                       et mappage MMIO explicite (corrections des warnings).
// =========================================================================

use x86_64::{
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, 
        PhysFrame, Size4KiB, Translate,
    },
    PhysAddr, VirtAddr,
};
use limine::memmap::MEMMAP_USABLE;
use spin::Mutex;
use core::sync::atomic::{AtomicU64, Ordering};

// --- [SECTION 1 : INITIALISATION DE LA PAGINATION] ---

pub unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let level_4_table = active_level_4_table(physical_memory_offset);
    OffsetPageTable::new(level_4_table, physical_memory_offset)
}

unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();
    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    &mut *page_table_ptr
}

// --- [SECTION 2 : ALLOCATEUR DE FRAMES LIMINE] ---

pub struct LimineFrameAllocator {
    memory_map: &'static [&'static limine::memmap::Entry],
    next: usize,
}

impl LimineFrameAllocator {
    pub unsafe fn init(memory_map: &'static [&'static limine::memmap::Entry]) -> Self {
        LimineFrameAllocator {
            memory_map,
            next: 0,
        }
    }

    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        let usable_regions = self.memory_map.iter().filter(|r| r.type_ == MEMMAP_USABLE);
        let addr_ranges = usable_regions.map(|r| r.base..r.base + r.length);
        let frame_addresses = addr_ranges.flat_map(|r| (r.start..r.end).step_by(4096));
        frame_addresses.map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }
}

unsafe impl FrameAllocator<Size4KiB> for LimineFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}

// --- [SECTION 3 : CONTEXTE MÉMOIRE GLOBAL] ---

pub struct ContexteMemoire {
    pub mapper: OffsetPageTable<'static>,
    pub frame_allocator: LimineFrameAllocator,
}

pub static CONTEXTE_MEMOIRE: Mutex<Option<ContexteMemoire>> = Mutex::new(None);

pub fn init_contexte(mapper: OffsetPageTable<'static>, frame_allocator: LimineFrameAllocator) {
    *CONTEXTE_MEMOIRE.lock() = Some(ContexteMemoire {
        mapper,
        frame_allocator,
    });
}

// --- [SECTION 4 : CONVERSIONS PHYSIQUE / VIRTUELLE] ---

static OFFSET_MEMOIRE_PHYSIQUE: AtomicU64 = AtomicU64::new(0);

pub fn enregistrer_offset_physique(offset: u64) {
    OFFSET_MEMOIRE_PHYSIQUE.store(offset, Ordering::SeqCst);
}

pub fn physique_vers_virtuelle(adresse_physique: u64) -> u64 {
    adresse_physique + OFFSET_MEMOIRE_PHYSIQUE.load(Ordering::Relaxed)
}

pub fn traduire_virtuelle_vers_physique(virt: u64) -> Option<u64> {
    let guard = CONTEXTE_MEMOIRE.lock();
    guard.as_ref().and_then(|ctx| {
        ctx.mapper.translate_addr(VirtAddr::new(virt)).map(|p| p.as_u64())
    })
}

// --- [SECTION 5 : MAPPAGE EXPLICITE DES REGISTRES MMIO] ---

pub fn mapper_mmio(phys_addr: u64, taille: u64) -> Option<u64> {
    let mut guard = contexte_memo_global();
    let ctx = guard.as_mut()?;

    let start_frame = PhysFrame::containing_address(PhysAddr::new(phys_addr & !0xFFF));
    let end_frame = PhysFrame::containing_address(PhysAddr::new(phys_addr + taille - 1));

    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE;

    for frame in PhysFrame::range_inclusive(start_frame, end_frame) {
        let virt_addr = physique_vers_virtuelle(frame.start_address().as_u64());
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(virt_addr));

        if ctx.mapper.translate_page(page).is_err() {
            unsafe {
                match ctx.mapper.map_to(page, frame, flags, &mut ctx.frame_allocator) {
                    Ok(flush) => flush.flush(),
                    Err(_) => return None,
                }
            }
        }
    }

    Some(physique_vers_virtuelle(phys_addr))
}

fn contexte_memo_global() -> spin::MutexGuard<'static, Option<ContexteMemoire>> {
    CONTEXTE_MEMOIRE.lock()
}
