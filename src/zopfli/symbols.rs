use libc::{c_int, size_t};

pub const ZOPFLI_NUM_LL: size_t = 288;
pub const ZOPFLI_NUM_D: size_t = 32;

pub const ZOPFLI_WINDOW_SIZE: size_t = 32768;
pub const ZOPFLI_WINDOW_MASK: size_t = 32767; // ZOPFLI_WINDOW_SIZE - 1
pub const ZOPFLI_MAX_MATCH: size_t = 258;
pub const ZOPFLI_MIN_MATCH: size_t = 3;
pub const ZOPFLI_CACHE_LENGTH: size_t = 8;

const LENGTH_SYMBOL_TABLE: [c_int; 259] = [
    0, 0, 0,
    257, 258, 259, 260, 261, 262, 263, 264,
    265, 265, 266, 266, 267, 267, 268, 268,
    269, 269, 269, 269, 270, 270, 270, 270,
    271, 271, 271, 271, 272, 272, 272, 272,
    273, 273, 273, 273, 273, 273, 273, 273,
    274, 274, 274, 274, 274, 274, 274, 274,
    275, 275, 275, 275, 275, 275, 275, 275,
    276, 276, 276, 276, 276, 276, 276, 276,
    277, 277, 277, 277, 277, 277, 277, 277,
    277, 277, 277, 277, 277, 277, 277, 277,
    278, 278, 278, 278, 278, 278, 278, 278,
    278, 278, 278, 278, 278, 278, 278, 278,
    279, 279, 279, 279, 279, 279, 279, 279,
    279, 279, 279, 279, 279, 279, 279, 279,
    280, 280, 280, 280, 280, 280, 280, 280,
    280, 280, 280, 280, 280, 280, 280, 280,
    281, 281, 281, 281, 281, 281, 281, 281,
    281, 281, 281, 281, 281, 281, 281, 281,
    281, 281, 281, 281, 281, 281, 281, 281,
    281, 281, 281, 281, 281, 281, 281, 281,
    282, 282, 282, 282, 282, 282, 282, 282,
    282, 282, 282, 282, 282, 282, 282, 282,
    282, 282, 282, 282, 282, 282, 282, 282,
    282, 282, 282, 282, 282, 282, 282, 282,
    283, 283, 283, 283, 283, 283, 283, 283,
    283, 283, 283, 283, 283, 283, 283, 283,
    283, 283, 283, 283, 283, 283, 283, 283,
    283, 283, 283, 283, 283, 283, 283, 283,
    284, 284, 284, 284, 284, 284, 284, 284,
    284, 284, 284, 284, 284, 284, 284, 284,
    284, 284, 284, 284, 284, 284, 284, 284,
    284, 284, 284, 284, 284, 284, 284, 285,
];

/// Gets the symbol for the given length, cfr. the DEFLATE spec.
/// Returns symbol in range [257-285] (inclusive).
#[no_mangle]
#[allow(non_snake_case)]
pub extern fn ZopfliGetLengthSymbol(length: c_int) -> c_int {
    LENGTH_SYMBOL_TABLE[length as usize]
}

/// Gets the symbol for the given dist, cfr. the DEFLATE spec.
#[no_mangle]
#[allow(non_snake_case)]
pub extern fn ZopfliGetDistSymbol(dist: c_int) -> c_int {
    match dist {
        0...4 => dist - 1,
        5...6 => 4,
        7...8 => 5,
        9...12 => 6,
        13...16 => 7,
        17...24 => 8,
        25...32 => 9,
        33...48 => 10,
        49...64 => 11,
        65...96 => 12,
        97...128 => 13,
        129...192 => 14,
        193...256 => 15,
        257...384 => 16,
        385...512 => 17,
        513...768 => 18,
        769...1024 => 19,
        1025...1536 => 20,
        1537...2048 => 21,
        2049...3072 => 22,
        3073...4096 => 23,
        4097...6144 => 24,
        6145...8192 => 25,
        8193...12288 => 26,
        12289...16384 => 27,
        16385...24576 => 28,
        _ => 29,
    }
}

const LENGTH_EXTRA_BITS: [c_int; 259] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 0
];

/// Gets the amount of extra bits for the given length, cfr. the DEFLATE spec.
#[no_mangle]
#[allow(non_snake_case)]
pub extern fn ZopfliGetLengthExtraBits(l: c_int) -> c_int {
    LENGTH_EXTRA_BITS[l as usize]
}

/// Gets the amount of extra bits for the given dist, cfr. the DEFLATE spec.
#[no_mangle]
#[allow(non_snake_case)]
pub extern fn ZopfliGetDistExtraBits(dist: c_int) -> c_int {
    if dist < 5 {
        0
    } else {
        match dist {
            5...8 => 1,
            9...16 => 2,
            17...32 => 3,
            33...64 => 4,
            65...128 => 5,
            129...256 => 6,
            257...512 => 7,
            513...1024 => 8,
            1025...2048 => 9,
            2049...4096 => 10,
            4097...8192 => 11,
            8193...16384 => 12,
            _ => 13,
        }
    }
}
