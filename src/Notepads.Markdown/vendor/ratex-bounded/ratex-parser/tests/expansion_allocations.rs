//! Process-isolated allocation regression. The ceiling is a test guard, not an
//! advertised process-memory limit. Run with --nocapture to retain diagnostics.
use std::alloc::{GlobalAlloc, Layout, System};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

struct CountingAllocator;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static TRACK: AtomicBool = AtomicBool::new(false);

const RESOURCE_LIMIT: &str = "Macro expansion resource limit exceeded";
const CALL_LIMIT: &str = "Too many expansions";
// Name, source byte length, expected rejection under the public default budget.
// Keep the small reported inputs: they may reach the call limit first, but must
// still stay below the allocation ceiling. Their larger variants must exercise
// the resource limit before reaching the call limit.
const CASES: &[(&str, usize, &str)] = &[
    ("report1", 260, RESOURCE_LIMIT),
    ("report2", 354, RESOURCE_LIMIT),
    ("edef", 309, RESOURCE_LIMIT),
    ("recursive", 2060, RESOURCE_LIMIT),
    ("recursive_bmod_report", 21, CALL_LIMIT),
    ("recursive_amp_report", 79, CALL_LIMIT),
    ("recursive_rows_report", 143, CALL_LIMIT),
    ("recursive_bmod_1k", 1041, RESOURCE_LIMIT),
    ("recursive_amp_1k", 1037, RESOURCE_LIMIT),
    ("recursive_rows_1k", 1039, RESOURCE_LIMIT),
    ("mhchem", 20906, RESOURCE_LIMIT),
];

fn allocated(size: usize) {
    let live = LIVE.fetch_add(size, Ordering::Relaxed) + size;
    if TRACK.load(Ordering::Relaxed) {
        PEAK.fetch_max(live, Ordering::Relaxed);
    }
}
// SAFETY: All operations delegate unchanged layouts/pointers to System. The
// counters use no allocation and never affect the allocation's lifetime.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            allocated(layout.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            allocated(layout.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe {
            System.dealloc(p, layout);
        }
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(p, old, new_size) };
        if !result.is_null() {
            LIVE.fetch_sub(old.size(), Ordering::Relaxed);
            allocated(new_size);
        }
        result
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn payload(case: &str) -> String {
    match case {
        "report1" => concat!(
            r"\def\zz{zzzzzzzzzz}",
            r"\edef\aa{\zz\zz\zz\zz\zz\zz\zz\zz\zz\zz}",
            r"\edef\bb{\aa\aa\aa\aa\aa\aa\aa\aa\aa\aa}",
            r"\edef\cc{\bb\bb\bb\bb\bb\bb\bb\bb\bb\bb}",
            r"\edef\dd{\cc\cc\cc\cc\cc\cc\cc\cc\cc\cc}",
            r"\edef\ee{\dd\dd\dd\dd\dd\dd\dd\dd\dd\dd}",
            r"\edef\ff{\ee\ee\ee\ee\ee\ee\ee\ee\ee\ee}x"
        )
        .into(),
        "report2" => {
            let mut s = String::from(r"\edef\m0{xxxxxxxx}");
            for k in 1..=18 {
                s += &format!("\n\\edef\\m{k}{{\\m{}\\m{}}}", k - 1, k - 1);
            }
            s + "\n\\m18"
        }
        "edef" => {
            let mut s = String::from(r"\edef\ma{xxxxxxxx}");
            for (a, b) in ('a'..='r').zip('b'..='s') {
                s += &format!("\\edef\\m{b}{{\\m{a}\\m{a}}}");
            }
            s + r"\ms"
        }
        "recursive" => format!("\\def\\a{{{}\\a}}\\a", "b".repeat(2048)),
        "recursive_bmod_report" => r"\def\a{bbbb\bmod\a}\a".into(),
        "recursive_amp_report" => format!("\\def\\x{{1{}\\x}}\\x", r"\&".repeat(33)),
        "recursive_rows_report" => format!("\\def\\x{{1{}X+\\x}}\\x", r"\\".repeat(64)),
        "recursive_bmod_1k" => format!("\\def\\a{{{}\\bmod\\a}}\\a", "b".repeat(1024)),
        "recursive_amp_1k" => format!("\\def\\x{{1{}\\x}}\\x", r"\&".repeat(512)),
        "recursive_rows_1k" => format!("\\def\\x{{1{}X+\\x}}\\x", r"\\".repeat(512)),
        "mhchem" => {
            // Many small nested machines reach the structure budget without
            // conflating allocation safety with long flat-input regex work.
            let group = format!("\\ce{{{}}} + ", "H + ".repeat(50) + "O");
            format!("\\ce{{{}O}}", group.repeat(100))
        }
        _ => panic!("unknown child case"),
    }
}

#[test]
fn bounded_expansion_allocations() {
    const ENV: &str = "RATEX_EXPANSION_ALLOCATION_CHILD";
    if let Ok(case) = std::env::var(ENV) {
        let &(_, input_bytes, rejection) = CASES
            .iter()
            .find(|(name, _, _)| *name == case)
            .expect("unknown child case");
        // Exclude one-time lazy initialization from the parse-window measurement.
        ratex_parser::parse(r"\ce{H2O}+x").unwrap();
        let input = payload(&case);
        assert_eq!(input.len(), input_bytes, "payload changed: {case}");
        let baseline = LIVE.load(Ordering::Relaxed);
        PEAK.store(baseline, Ordering::Relaxed);
        TRACK.store(true, Ordering::Relaxed);
        let start = Instant::now();
        let result = ratex_parser::parse(&input);
        TRACK.store(false, Ordering::Relaxed);
        let peak = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
        let elapsed = start.elapsed();
        let error = result.unwrap_err();
        eprintln!("case={case} input_bytes={} peak_increment_bytes={peak} elapsed={elapsed:?} error={error}", input.len());
        assert!(
            error.message.contains(rejection),
            "unexpected rejection for {case}: {error}"
        );
        assert!(
            peak < 64 * 1024 * 1024,
            "unexpected allocation amplification for {case}: {peak}"
        );
        return;
    }
    for &(case, _, _) in CASES {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "bounded_expansion_allocations", "--nocapture"])
            .env(ENV, case)
            .spawn()
            .unwrap();
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "child {case} failed: {status}");
                break;
            }
            if start.elapsed() > Duration::from_secs(30) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("child {case} timed out");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
