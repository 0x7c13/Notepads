macro_rules! character_set {
    () => {{
        [false; 256]
    }};

    ($value:literal $(,$rest:literal)*) => {{
        const A: &[u8] = $value;
        let mut a = character_set!($($rest),*);
        let mut i = 0;
        while i < A.len() {
            a[A[i] as usize] = true;
            i += 1;
        }
        a
    }}
}

pub(crate) use character_set;
