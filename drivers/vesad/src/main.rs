#![feature(alloc)]
#![feature(asm)]
#![feature(heap_api)]
#![feature(question_mark)]

extern crate alloc;
extern crate ransid;
extern crate syscall;

use std::cell::RefCell;
use std::fs::File;
use std::io::{Read, Write};
use std::{slice, thread};
use ransid::{Console, Event};
use syscall::{physmap, physunmap, Packet, Result, Scheme, MAP_WRITE, MAP_WRITE_COMBINE};

use display::Display;
use mode_info::VBEModeInfo;
use primitive::fast_set64;

pub mod display;
pub mod mode_info;
pub mod primitive;

struct DisplayScheme {
    console: RefCell<Console>,
    display: RefCell<Display>
}

impl Scheme for DisplayScheme {
    fn open(&self, _path: &[u8], _flags: usize) -> Result<usize> {
        Ok(0)
    }

    fn dup(&self, _id: usize) -> Result<usize> {
        Ok(0)
    }

    fn fsync(&self, _id: usize) -> Result<usize> {
        Ok(0)
    }

    fn write(&self, _id: usize, buf: &[u8]) -> Result<usize> {
        let mut display = self.display.borrow_mut();
        self.console.borrow_mut().write(buf, |event| {
            match event {
                Event::Char { x, y, c, color, .. } => display.char(x * 8, y * 16, c, color.data),
                Event::Rect { x, y, w, h, color } => display.rect(x * 8, y * 16, w * 8, h * 16, color.data),
                Event::Scroll { rows, color } => display.scroll(rows * 16, color.data)
            }
        });
        Ok(buf.len())
    }

    fn close(&self, _id: usize) -> Result<usize> {
        Ok(0)
    }
}

fn main() {
    let width;
    let height;
    let physbaseptr;

    {
        let mode_info = unsafe { &*(physmap(0x5200, 4096, 0).expect("vesad: failed to map VBE info") as *const VBEModeInfo) };

        width = mode_info.xresolution as usize;
        height = mode_info.yresolution as usize;
        physbaseptr = mode_info.physbaseptr as usize;

        unsafe { let _ = physunmap(mode_info as *const _ as usize); }
    }

    if physbaseptr > 0 {
        thread::spawn(move || {
            let mut socket = File::create(":display").expect("vesad: failed to create display scheme");

            let size = width * height;

            let onscreen = unsafe { physmap(physbaseptr as usize, size * 4, MAP_WRITE | MAP_WRITE_COMBINE).expect("vesad: failed to map VBE LFB") };
            unsafe { fast_set64(onscreen as *mut u64, 0, size/2) };

            let offscreen = unsafe { alloc::heap::allocate(size * 4, 4096) };
            unsafe { fast_set64(offscreen as *mut u64, 0, size/2) };

            let scheme = DisplayScheme {
                console: RefCell::new(Console::new(width/8, height/16)),
                display: RefCell::new(Display::new(width, height,
                    unsafe { slice::from_raw_parts_mut(onscreen as *mut u32, size) },
                    unsafe { slice::from_raw_parts_mut(offscreen as *mut u32, size) }
                ))
            };

            loop {
                let mut packet = Packet::default();
                socket.read(&mut packet).expect("vesad: failed to read display scheme");
                //println!("vesad: {:?}", packet);
                scheme.handle(&mut packet);
                socket.write(&packet).expect("vesad: failed to write display scheme");
            }
        });
    }
}