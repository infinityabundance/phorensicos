// phost_kernel — Kernel Heap Allocator
//
// A minimal first-fit free-list allocator over a static heap region.
// Single-CPU (boot-time) safe: no locks, no atomics. Every block carries
// a header with size + used flag; free blocks form a doubly-linked list.
//
// This is deliberately small and auditable — the whole thing is ~150 lines.
// It is NOT a general-purpose allocator (no coalescing across arbitrary
// free-list shuffling on free; it does coalesce adjacent free blocks).

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;

/// Heap size: 8 MiB. QEMU's default RAM is 128 MiB; 8 MiB is ample for the
/// boot-time compositor/shell surfaces and leaves the rest for the kernel.
pub const HEAP_SIZE: usize = 8 * 1024 * 1024;

/// Alignment for every allocation: 16 bytes (matches x86-64 max scalar align).
const ALIGN: usize = 16;

/// Free-list node header. Stored inside free blocks.
#[repr(C)]
struct FreeNode {
    size: usize,          // total block size including this header
    used: bool,           // false for free blocks
    next: *mut FreeNode,
    prev: *mut FreeNode,
}

impl FreeNode {
    unsafe fn as_ptr(&self) -> *mut u8 {
        self as *const FreeNode as *mut u8
    }
}

/// Static heap storage — zero-initialized (in .bss).
static mut HEAP_STORAGE: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

/// The kernel heap allocator.
pub struct KernelAllocator;

// Single-CPU boot-time allocator: the static is only ever accessed by the
// BSP before SMP bring-up, so mutable-reference-to-static is sound here.
#[allow(static_mut_refs)]

/// Initialize the free list over the static heap. Called once from the
/// kernel entry point before any allocation happens.
///
/// # Safety
/// Must be called exactly once, before any allocation.
pub unsafe fn init() {
    let head = HEAP_STORAGE.as_mut_ptr() as *mut FreeNode;
    (*head).size = HEAP_SIZE;
    (*head).used = false;
    (*head).next = ptr::null_mut();
    (*head).prev = ptr::null_mut();
}

/// Size of the allocator header.
const HEADER_SIZE: usize = core::mem::size_of::<FreeNode>();

unsafe fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

/// Walk the free list looking for a block large enough for `size` payload
/// plus header. Returns the chosen block's header pointer, or null.
unsafe fn find_fit(size: usize) -> *mut FreeNode {
    let head = HEAP_STORAGE.as_mut_ptr() as *mut FreeNode;
    let mut cur = head;
    while !cur.is_null() {
        let block = &*cur;
        if !block.used && block.size >= size + HEADER_SIZE {
            return cur;
        }
        cur = block.next;
    }
    ptr::null_mut()
}

/// Split a free block: keep `needed` bytes for the allocation, turn the
/// remainder into a new free block.
unsafe fn split_block(node: *mut FreeNode, needed: usize) {
    let block = &mut *node;
    if block.size - needed >= HEADER_SIZE + ALIGN {
        let remainder = (node as *mut u8).add(needed) as *mut FreeNode;
        (*remainder).size = block.size - needed;
        (*remainder).used = false;
        (*remainder).next = block.next;
        (*remainder).prev = node;
        if !block.next.is_null() {
            (*(block.next)).prev = remainder;
        }
        block.next = remainder;
        block.size = needed;
    }
}

/// Coalesce a freed block with its successor if the successor is also free.
unsafe fn coalesce(node: *mut FreeNode) {
    let block = &mut *node;
    let next = block.next;
    if !next.is_null() && !(*next).used {
        block.size += (*next).size;
        block.next = (*next).next;
        if !block.next.is_null() {
            (*(block.next)).prev = node;
        }
    }
}

unsafe impl GlobalAlloc for KernelAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = align_up(layout.size().max(1), ALIGN);
        let needed = size + HEADER_SIZE;

        let node = find_fit(size);
        if node.is_null() {
            return ptr::null_mut();
        }

        split_block(node, needed);

        let block = &mut *node;
        block.used = true;
        // Return a pointer past the header, aligned to the requested alignment.
        let payload = (node as *mut u8).add(HEADER_SIZE);
        align_up(payload as usize, layout.align().max(ALIGN)) as *mut u8
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        if ptr.is_null() {
            return;
        }
        // Recover the header: it lives HEADER_SIZE bytes before the payload.
        let node = (ptr as *mut u8).sub(HEADER_SIZE) as *mut FreeNode;
        (*node).used = false;
        coalesce(node);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
            return self.alloc(layout);
        }
        let old_size = {
            let node = (ptr as *mut u8).sub(HEADER_SIZE) as *mut FreeNode;
            (*node).size - HEADER_SIZE
        };
        if new_size <= old_size {
            return ptr;
        }
        let new_layout = Layout::from_size_align(new_size, layout.align()).unwrap_or(layout);
        let new_ptr = self.alloc(new_layout);
        if !new_ptr.is_null() {
            ptr::copy_nonoverlapping(ptr, new_ptr, old_size);
            self.dealloc(ptr, layout);
        }
        new_ptr
    }
}
