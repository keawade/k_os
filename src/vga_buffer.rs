use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use volatile::Volatile;

/**
 * Rust module that encapsulates the unsafe writing to the vga text buffer and
 * presents a safe and convenient interface for external use.
 */

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct ColorCode(u8);

impl ColorCode {
    fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;

struct Buffer {
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

pub struct Writer {
    column_position: usize,
    color_code: ColorCode,
    buffer: &'static mut Buffer,
}

impl Writer {
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            // Call new line if the character to write is '/n'
            b'\n' => self.new_line(),
            byte => {
                // Call new line before proceeding if we're out of space in the current line
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line();
                }

                let row = BUFFER_HEIGHT - 1;
                let col = self.column_position;

                let color_code = self.color_code;
                // Set the buffer character at the current position to the incoming character
                self.buffer.chars[row][col].write(ScreenChar {
                    ascii_character: byte,
                    color_code,
                });
                // Increment the current column position
                self.column_position += 1;
            }
        }
    }

    fn new_line(&mut self) {
        // Starting on the second row from the top
        for row in 1..BUFFER_HEIGHT {
            // And for every character
            for col in 0..BUFFER_WIDTH {
                // Grab each character
                let character = self.buffer.chars[row][col].read();
                // And overwrite the previous row at the same position with the character we just grabbed
                self.buffer.chars[row - 1][col].write(character);
            }
        }

        // Clear bottom row
        self.clear_row(BUFFER_HEIGHT - 1);
        // Reset cursor to beginning of line
        self.column_position = 0;
    }

    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };

        // Replace every character in the specified row with blank characters
        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }

    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                // printable ASCII byte or newline: ' ' through '~'
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                // Print '■' for bytes outside of the allowed ASCII range
                _ => self.write_byte(0xfe),
            }
        }
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

// Create static writer instance so we don't have to keep creating them everywhere
lazy_static! {
    pub static ref WRITER: Mutex<Writer> = Mutex::new(Writer {
        column_position: 0,
        color_code: ColorCode::new(Color::Yellow, Color::Black),
        buffer: unsafe { &mut *(0xb8000 as *mut Buffer) },
    });
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga_buffer::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    // Prevent write from dead locking when an interrupt tries to lock the spin mutex
    // TODO: Come back and implement a solution that doesn't require disabling interrupts
    interrupts::without_interrupts(|| {
        WRITER.lock().write_fmt(args).unwrap();
    })
}

#[test_case]
fn test_println_simple() {
    println!("test_println_simple output");
}

#[test_case]
fn test_println_output() {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    let s = "Some test string that fits on a single line";
    // Disable interrupts to prevent deadlock
    interrupts::without_interrupts(|| {
        // Lock writer explicitly
        let mut writer = WRITER.lock();
        // Use writeln!() to allow printing to an already locked writer
        // Print a leading '\n' as the timer handler may already have printed some '.' characters to the current line
        writeln!(writer, "\n{}", s).expect("writeln failed");
        for (i, c) in s.chars().enumerate() {
            let screen_char = writer.buffer.chars[BUFFER_HEIGHT - 2][i].read();
            assert_eq!(char::from(screen_char.ascii_character), c);
        }
    })
}
