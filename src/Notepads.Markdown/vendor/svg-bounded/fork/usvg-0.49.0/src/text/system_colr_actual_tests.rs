// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Explicit four-key observation host. This module is never in a production build.

use crate::Cache;
use crate::{FontReplay, FontVariation, GlyphId, ProcessingContext, ReplayOutline};
use markdown_svg_budget::{
    Backend, BudgetVec, Error, FirstFailureRef, Limits, ResourceKind, ResourcesRef, Ticket,
};
use skrifa::raw::TableProvider;
use std::fmt;
use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Default)]
struct Ledger {
    used: AtomicUsize,
    next: AtomicUsize,
    calls: AtomicUsize,
    peak: AtomicUsize,
}
struct CounterOwner(Arc<Ledger>);
impl Backend for CounterOwner {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        let used = self.0.used.fetch_add(bytes, Ordering::SeqCst) + bytes;
        self.0.peak.fetch_max(used, Ordering::SeqCst);
        Ticket::new(
            bytes as u64,
            self.0.next.fetch_add(1, Ordering::SeqCst) as u32 + 1,
            1,
        )
    }
    fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), Error> {
        Ok(())
    }
    fn release_after_free(&self, t: Ticket) -> Result<(), Error> {
        let bytes = t.account_nonce() as usize;
        assert!(self.0.used.fetch_sub(bytes, Ordering::SeqCst) >= bytes);
        Ok(())
    }
    fn checkpoint(&self, _: u64) -> Result<(), Error> {
        self.0.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct FontBytes(BudgetVec<u8>);
impl AsRef<[u8]> for FontBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
struct FourKeyReplay {
    db: Arc<fontdb::Database>,
    id: fontdb::ID,
    resources: ResourcesRef,
    axes: Arc<[FontVariation]>,
}
impl fmt::Debug for FourKeyReplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FourKeyReplay")
    }
}
impl FontReplay for FourKeyReplay {
    fn request_resources(&self) -> Option<ResourcesRef> {
        Some(self.resources.clone())
    }
    fn prepare(
        &self,
        _: &crate::Text,
        _: &mut Arc<fontdb::Database>,
    ) -> Result<crate::ReplayText, ()> {
        Err(())
    }
    fn fontdb(&self) -> Result<Arc<fontdb::Database>, ()> {
        Ok(self.db.clone())
    }
    fn variations(&self, id: fontdb::ID) -> Result<Arc<[FontVariation]>, ()> {
        if id == self.id {
            Ok(self.axes.clone())
        } else {
            Err(())
        }
    }
    fn outline(&self, id: fontdb::ID, gid: u32) -> Result<ReplayOutline, ()> {
        self.resources.checkpoint(0).map_err(|_| ())?;
        if id == self.id && [1623, 2317, 4087, 4131].contains(&gid) {
            Ok(ReplayOutline::Ordinary)
        } else {
            Err(())
        }
    }
}

// Hash the exact immutable in-memory input through existing local .NET crypto.
// The pipe contains no filename and neither process writes font bytes to disk.
// This adds no Cargo dependency, product API, FFI, or font interpreter.
fn input_sha256(bytes: &[u8]) -> String {
    let script = "$h=[Security.Cryptography.SHA256]::Create();[BitConverter]::ToString($h.ComputeHash([Console]::OpenStandardInput())).Replace('-','')";
    let mut child = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .creation_flags(0x0800_0000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("local SHA256 process");
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "local SHA256 failed");
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn path_count(group: &crate::Group) -> usize {
    group
        .children()
        .iter()
        .map(|node| match node {
            crate::Node::Group(g) => path_count(g),
            crate::Node::Path(_) => 1,
            _ => panic!("unexpected non-path COLR scene leaf"),
        })
        .sum()
}

#[test]
#[ignore = "requires the sealed four-glyph system-font input and an explicit output directory"]
fn system_colr_actual_four_keys() {
    let font_path = PathBuf::from(
        std::env::var_os("NOTEPADS_MARKDOWN_SVG_COLR_ACTUAL_FONT").expect("explicit sealed font path"),
    );
    let out = PathBuf::from(
        std::env::var_os("NOTEPADS_MARKDOWN_SVG_COLR_ACTUAL_OUTPUT").expect("explicit output directory"),
    );
    assert!(out.is_dir());
    let bytes = std::fs::read(&font_path).unwrap();
    assert_eq!(bytes.len(), 12_934_920);
    let sha = input_sha256(&bytes);
    assert_eq!(
        sha,
        "451DF9BB5546055AF625FF86AA72695DF43E8E170B4C6C1316FE44E9C7A22F7C"
    );
    let ledger = Arc::new(Ledger::default());
    let resources = ResourcesRef::new_prepaid(
        CounterOwner(ledger.clone()),
        FirstFailureRef::new_prepaid(),
        Limits::default(),
    );
    let processing = Arc::new(ProcessingContext::with_resources(resources.clone()));
    let mut paid = BudgetVec::new(resources.clone(), ResourceKind::Font);
    paid.try_extend_from_slice(&bytes).unwrap();
    assert_eq!(&*paid, bytes.as_slice());
    drop(bytes);
    let source: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(FontBytes(paid));
    let mut ids = [fontdb::ID::dummy()];
    let db = fontdb::Database::new()
        .try_extend_in(
            &resources,
            1,
            |_| {
                Ok(fontdb::FaceRegistration {
                    source: source.clone(),
                    index: 0,
                    family: "Actual-Segoe-UI-Emoji",
                })
            },
            &mut ids,
        )
        .unwrap();
    assert!(db.is_funded());
    let id = ids[0];
    let db = Arc::new(db);
    let (registered, index) = db.face_source(id).unwrap();
    assert_eq!(index, 0);
    assert!(Arc::ptr_eq(registered.binary_data().unwrap(), &source));
    drop(registered);
    let axes: Arc<[FontVariation]> = vec![
        FontVariation::new(*b"ital", 0.0),
        FontVariation::new(*b"slnt", 0.0),
        FontVariation::new(*b"wdth", 100.0),
        FontVariation::new(*b"wght", 400.0),
    ]
    .into();
    let instance = super::font_instance::FontInstance::new(
        source.as_ref().as_ref(),
        0,
        &axes,
        Some(&resources),
    )
    .unwrap()
    .expect("same source font instance");
    assert_eq!(instance.font().head().unwrap().units_per_em(), 2048);
    assert!(instance.font().fvar().is_err());
    assert!(instance.validated_location().unwrap().coords().is_empty());
    drop(instance);
    let replay = Arc::new(FourKeyReplay {
        db: db.clone(),
        id,
        resources: resources.clone(),
        axes: axes.clone(),
    });
    let mut cache = Cache::with_processing(processing.clone(), db.clone());
    cache.font_replay = Some(replay.clone());
    let mut glyphs = Vec::new();
    for gid in [1623, 2317, 4087, 4131] {
        let tree = cache
            .fontdb_colr(id, GlyphId(gid), &axes, svgtypes::Color::black())
            .expect("real Some COLR producer returned Tree");
        assert!(resources.failure().is_none());
        let paths = path_count(tree.root());
        let svg = tree.to_string(&crate::WriteOptions::default());
        let name = format!("glyph-{gid}.svg");
        let path = out.join(&name);
        assert!(!path.exists());
        std::fs::write(path, svg).unwrap();
        // The generic SVG writer omits fr; preserve every actual Tree fr here.
        let radials = tree
            .radial_gradients()
            .iter()
            .map(|g| {
                format!(
                    "{{\"id\":{:?},\"fr\":{},\"frBits\":{}}}",
                    g.id(),
                    g.fr().get(),
                    g.fr().get().to_bits()
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        glyphs.push(format!("{{\"glyphID\":{gid},\"svg\":\"{name}\",\"scenePathCount\":{paths},\"radials\":[{radials}]}}"));
        println!("ACTUAL_COLR gid={gid} scene_paths={paths} svg_file={name}");
        drop(tree);
    }
    drop(cache);
    drop(replay);
    drop(db);
    drop(source);
    drop(axes);
    assert!(resources.failure().is_none());
    assert_eq!((processing.counts().2, processing.counts().3), (0, 0));
    drop(processing);
    resources.validate_retired().unwrap();
    assert_eq!(ledger.used.load(Ordering::SeqCst), 0);
    println!(
        "ACTUAL_COLR_LEDGER checkpoints={} requested_live_peak={} used=0 retired=true",
        ledger.calls.load(Ordering::SeqCst),
        ledger.peak.load(Ordering::SeqCst)
    );
    let result = format!(
        "{{\"schema\":\"notepads.markdown.four-key-colr-actual-tree.v1\",\"fontSha256\":\"{sha}\",\"fontBytes\":12934920,\"faceIndex\":0,\"palette\":0,\"foregroundRgba\":[0,0,0,255],\"normalizedAxes\":[],\"nativeCanonicalAxes\":[[\"ital\",0],[\"slnt\",0],[\"wdth\",100],[\"wght\",400]],\"sameBinaryDatabaseOwner\":true,\"fundedDatabase\":true,\"processingSome\":true,\"producer\":\"Cache::fontdb_colr -> DatabaseExt::colr -> Some FontInstance/ColorGlyph paint -> GeneratedColorGlyph Tree\",\"windowsFontBytesCopiedToDisk\":false,\"writerCoordinatesPrecision\":8,\"writerTransformsPrecision\":8,\"glyphs\":[{}],\"retired\":true,\"liveTickets\":0,\"lateConversions\":0,\"lateRenders\":0}}\n",
        glyphs.join(",")
    );
    assert!(!out.join("ACTUAL.json").exists());
    std::fs::write(out.join("ACTUAL.json"), result).unwrap();
}
