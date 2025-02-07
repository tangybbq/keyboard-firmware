STACK_SIZE = 64K;

MEMORY {
    BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100
    FLASH : ORIGIN = 0x10000100, LENGTH = 2048K - 0x100

    /* Pick one of the two options for RAM layout     */

    /* OPTION A: Use all RAM banks as one big block   */
    /* Reasonable, unless you are doing something     */
    /* really particular with DMA or other concurrent */
    /* access that would benefit from striping        */
    RAM   : ORIGIN = 0x20000000, LENGTH = 264K - STACK_SIZE
    STACK0 : ORIGIN = ORIGIN(RAM) + LENGTH(RAM), LENGTH = STACK_SIZE

    /* OPTION B: Keep the unstriped sections separate */
    /* RAM: ORIGIN = 0x20000000, LENGTH = 256K        */
    /* SCRATCH_A: ORIGIN = 0x20040000, LENGTH = 4K    */
    /* SCRATCH_B: ORIGIN = 0x20041000, LENGTH = 4K    */
}

/* Jolt boards contains a 256-byte block 256 bytes before the 2MB boundary
 * in flash. */
_board_info = ORIGIN(BOOT2) + 2M - 256;

/* Use the explicit stack so we can use the MPU to protect it. */
_stack_end = ORIGIN(STACK0);
_stack_start = _stack_end + LENGTH(STACK0);
