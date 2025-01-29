macro_rules! find_range {
    ($bytes:expr, $rv:expr) => {{
        let rv = $rv.as_bytes();

        let start = rv.as_ptr() as usize - $bytes.as_ptr() as usize;
        let end = start + rv.len();

        debug_assert_eq!(rv, &$bytes[start..end]);

        start..end
    }};
}
