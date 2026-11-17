/* ==============================================================================
 * Fichier : linker.ld (Révision 1.5)
 * Description : Script de liaison higher-half avec préservation de toutes les sections de requêtes Limine
 * ============================================================================== */

ENTRY(_start)

PHDRS
{
    text    PT_LOAD FLAGS((1 << 0) | (1 << 2)); /* Exécutable + Lecture */
    rodata  PT_LOAD FLAGS((1 << 2));            /* Lecture seule */
    data    PT_LOAD FLAGS((1 << 1) | (1 << 2)); /* Écriture + Lecture */
}

SECTIONS
{
    . = 0xffffffff80000000;

    .text : {
        *(.text .text.*)
    } :text

    . = ALIGN(0x1000);

    .rodata : {
        *(.rodata .rodata.*)
    } :rodata

    . = ALIGN(0x1000);

    .data : {
        *(.data .data.*)
        KEEP(*(.requests_start_marker))
        KEEP(*(.limine_requests))
        KEEP(*(.requests))
        KEEP(*(.requests_end_marker))
    } :data

    .bss : {
        *(.bss .bss.*)
        *(COMMON)
    } :data

    /DISCARD/ : {
        *(.eh_frame)
        *(.note .note.*)
    }
}
