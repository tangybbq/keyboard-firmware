/* Show heap info */

#include <zephyr/kernel.h>
#include <zephyr/sys/sys_heap.h>

// To be able to use this, the 'static' must be commented out of this declaration.
extern struct sys_heap z_malloc_heap;

void show_heap_stats(void)
{
	struct sys_memory_stats stats;

	sys_heap_runtime_stats_get(&z_malloc_heap, &stats);
	printk("Heap free: %u\n", stats.free_bytes);
	printk("    alloc: %u\n", stats.allocated_bytes);
	printk("max alloc: %u\n", stats.max_allocated_bytes);
}
