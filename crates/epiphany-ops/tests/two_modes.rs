//! The two-mode fuzz in CI (`epiphany_ops::fuzz::modes`), and every history
//! it has found failing, minimized and committed under `tests/two_modes/`.
//!
//! A committed history is one envelope per line in the envelope text form,
//! after `#` lines. Two of them are read here: `# class:` names the failure
//! the fuzz found (an effect split, the objects or canonical bytes differing,
//! or an invariant the graph-aware score breaks), and `# expect:` says
//! whether the history must now reduce alike (`agree`) or still shows that
//! failure (`split`), for a failure not yet fixed. A fix flips its histories
//! from `split` to `agree`; a `split` history whose failure has gone fails
//! here until it is flipped, so a fix cannot pass unrecorded, and one that
//! shows a second failure beside its own fails too. A failure the
//! owner has deferred is `# expect: deferred`, with a `# deferred:` line
//! giving the reason: its class must be a cause the owner deferred by name
//! (`modes::deferred`), its history must still fail as its class alone, as
//! a `split` one does, and it is run by an ignored test, which requires it to
//! agree, so `cargo test -- --ignored` shows it failing until the fix lands.
//!
//! The CI budget runs a small number of generated histories, in two chunks of
//! different lengths, and requires that every failure it finds is a committed
//! `split` or `deferred` class, and that every operation kind and payload is
//! authored and applied. The local budget is the `fuzz_modes` example.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use epiphany_ops::fuzz::modes;

/// The CI budget, in chunks: histories, their first seed, and the operations
/// each authors after its genesis. The longer chunk reaches states the shorter
/// does not within its histories (review 1's L4: a planted break the first
/// chunk alone missed).
const CI_BUDGET: [(u64, u64, usize); 2] = [(96, 0x4A_0001, 24), (64, 0x4A_0001, 64)];

/// The CI budget's runs, one per chunk with its authored count, shared by the
/// tests that read them.
fn ci_run() -> &'static [(usize, modes::Report)] {
    static REPORTS: OnceLock<Vec<(usize, modes::Report)>> = OnceLock::new();
    REPORTS.get_or_init(|| {
        CI_BUDGET
            .iter()
            .map(|&(histories, seed, authored)| (authored, modes::run(seed, histories, authored)))
            .collect()
    })
}

/// What a committed history declares.
#[derive(PartialEq)]
enum Expect {
    Agree,
    Split,
    /// Deferred by the owner, with the reason.
    Deferred(String),
}

struct Committed {
    file: String,
    class: String,
    expect: Expect,
    history: Vec<epiphany_ops::OperationEnvelope>,
}

fn committed() -> Vec<Committed> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/two_modes");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("the committed histories' directory")
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let file = path.file_name().unwrap().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&path).expect("readable");
            let header = |key: &str| {
                text.lines()
                    .find_map(|l| l.strip_prefix(&format!("# {key}: ")))
                    .unwrap_or_else(|| panic!("{file} has no `# {key}:` line"))
                    .to_owned()
            };
            let class = header("class");
            let expect = match header("expect").as_str() {
                "split" => Expect::Split,
                "agree" => Expect::Agree,
                "deferred" => Expect::Deferred(header("deferred")),
                other => panic!("{file}: `# expect: {other}` is not split, agree or deferred"),
            };
            let history = modes::parse(&text).unwrap_or_else(|e| panic!("{file}: {e:?}"));
            assert!(!history.is_empty(), "{file} holds no envelope");
            Committed {
                file,
                class,
                expect,
                history,
            }
        })
        .collect()
}

/// Whether `found` shows `class` and nothing else: a history declared to
/// split or deferred must show exactly its declared failure, so a second
/// break joining it does not pass unseen (review 2's L5).
fn shows_only(found: &[modes::Finding], class: &str) -> bool {
    !found.is_empty() && found.iter().all(|f| f.class == class)
}

#[test]
fn every_committed_history_reduces_as_it_declares() {
    let all = committed();
    assert!(!all.is_empty(), "no committed history was read");
    for c in &all {
        assert!(
            modes::valid(&c.history),
            "{}: names an object no operation its author saw minted",
            c.file
        );
        if matches!(c.expect, Expect::Deferred(_)) {
            assert!(
                modes::deferred(&c.class),
                "{}: declared deferred, but `{}` is no cause the owner deferred",
                c.file,
                c.class
            );
        }
        let found = modes::findings(&c.history);
        if c.expect != Expect::Agree {
            assert!(
                shows_only(&found, &c.class),
                "{}: declared to split as `{}` alone, but finds {:?}; if the fix is in, \
                 flip it to `# expect: agree`",
                c.file,
                c.class,
                found.iter().map(|f| &f.class).collect::<Vec<_>>()
            );
        } else {
            assert!(
                found.is_empty(),
                "{}: declared to agree, but finds {:#?}",
                c.file,
                found
            );
        }
    }
}

/// A history showing its declared class and another fails its declaration:
/// the never-seen and seen-deleted histories, their replicas renamed apart and
/// run as one history, show both classes, so neither alone is what the
/// history shows, while each history by itself shows its own.
#[test]
fn a_history_showing_a_second_class_fails_its_declaration() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/two_modes");
    let read = |file: &str| std::fs::read_to_string(dir.join(file)).expect("readable");
    let never_seen = read("110-invariant-region-extents.txt");
    let seen_deleted = read("145-invariant-region-extents.txt")
        .replace("#x0000000000000001", "#x0000000000000004")
        .replace("#x0000000000000003", "#x0000000000000006");
    let alone = |text: &str| modes::findings(&modes::parse(text).expect("parses"));
    assert!(shows_only(&alone(&never_seen), modes::REGION_NEVER_SEEN));
    assert!(shows_only(
        &alone(&seen_deleted),
        modes::REGION_SEEN_DELETED
    ));
    let mut both = modes::parse(&never_seen).expect("parses");
    both.extend(modes::parse(&seen_deleted).expect("parses"));
    let found = modes::findings(&both);
    assert!(found.iter().any(|f| f.class == modes::REGION_NEVER_SEEN));
    assert!(found.iter().any(|f| f.class == modes::REGION_SEEN_DELETED));
    assert!(!shows_only(&found, modes::REGION_NEVER_SEEN));
    assert!(!shows_only(&found, modes::REGION_SEEN_DELETED));
}

/// Every region overlap made by creating or filling regions is deferred
/// (owner's rulings D48 and D50 to D52), whatever its cause and however many
/// authors: two authors each creating a region over the same time and staves,
/// neither having seen the other's; an author creating one where its own
/// delete of a region, refused in the merged history, or its own undo of the
/// region's transaction, blocked there, left none in its view; or an author
/// placing one where its view holds one. Refusing a region's creation or fill
/// where it would overlap one the merged history keeps needs region extents
/// compared in both modes, which X5 brings, closing the class; until then the
/// histories are kept and run here.
#[test]
#[ignore = "every region overlap made by creating or filling regions is deferred \
            (D48, D50 to D52) until X5 refuses it by region extents compared in \
            both modes"]
fn every_deferred_history_reduces_alike() {
    let deferred: Vec<Committed> = committed()
        .into_iter()
        .filter(|c| matches!(c.expect, Expect::Deferred(_)))
        .collect();
    assert!(!deferred.is_empty(), "no deferred history was read");
    for c in &deferred {
        let found = modes::findings(&c.history);
        assert!(
            found.is_empty(),
            "{} (deferred: {}): finds {:#?}",
            c.file,
            match &c.expect {
                Expect::Deferred(reason) => reason.as_str(),
                _ => unreachable!(),
            },
            found
        );
    }
}

#[test]
fn the_ci_budget_finds_no_failure_that_is_not_committed() {
    let known: BTreeSet<String> = committed()
        .into_iter()
        .filter(|c| c.expect != Expect::Agree)
        .map(|c| c.class)
        .collect();
    let unknown: Vec<_> = ci_run()
        .iter()
        .flat_map(|(authored, report)| {
            report
                .findings
                .iter()
                .map(move |finding| (authored, finding))
        })
        .filter(|(_, (class, _))| !known.contains(*class))
        .map(|(authored, (class, (seed, history, finding)))| {
            format!(
                "{class}\n  seed {seed:#x}, {authored} authored, {} envelopes; minimize \
                 with the fuzz_modes example and commit it\n  {}",
                history.len(),
                finding.detail
            )
        })
        .collect();
    assert!(unknown.is_empty(), "{}", unknown.join("\n"));
}

#[test]
fn the_ci_budget_authors_and_applies_every_kind() {
    let mut missing = Vec::new();
    for kind in modes::Coverage::kinds() {
        let count = |pick: fn(&modes::Coverage) -> &std::collections::BTreeMap<String, u64>| {
            ci_run()
                .iter()
                .map(|(_, report)| pick(&report.coverage).get(&kind).copied().unwrap_or(0))
                .sum::<u64>()
        };
        let authored = count(|c| &c.authored);
        let applied = count(|c| &c.applied);
        if authored == 0 || applied == 0 {
            missing.push(format!("{kind}: authored {authored}, applied {applied}"));
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}

/// The deferred class is read by its mechanism (D52), not its authors: the
/// never-seen history, its second region's author made to have seen the
/// first region's create, still overlaps them, an overlap one author makes
/// directly, and is in the class under its general name, the cause being none
/// the classifier names.
#[test]
fn a_region_overlap_its_author_saw_is_the_deferred_class() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/two_modes/110-invariant-region-extents.txt");
    let text = std::fs::read_to_string(path).expect("readable");
    let unaware =
        "(stamp 37 0 #x00000000000000010000000000000003) (causal ((#x0000000000000001 2)) ())";
    assert_eq!(
        text.matches(unaware).count(),
        1,
        "the second region's create"
    );
    let aware = text.replace(
        unaware,
        "(stamp 37 0 #x00000000000000010000000000000003) \
         (causal ((#x0000000000000001 2) (#x0000000000000002 0)) ())",
    );
    let history = modes::parse(&aware).expect("parses");
    assert!(modes::valid(&history));
    let classes: Vec<String> = modes::findings(&history)
        .into_iter()
        .map(|f| f.class)
        .collect();
    assert_eq!(classes, vec![modes::REGION_OVERLAP], "{classes:?}");
    assert!(modes::deferred(modes::REGION_OVERLAP));
    let unchanged = modes::findings(&modes::parse(&text).expect("parses"));
    assert!(unchanged
        .iter()
        .any(|f| f.class == modes::REGION_NEVER_SEEN));
}

/// The deferred class's second cause (D50) is named apart, and holds only
/// where its author's view did not hold the region live because of its own
/// delete, which the merged history refuses: the committed history is so
/// classed; with the second create's author having also seen the concurrent
/// fill, so that its view refused the delete too and held the region, or
/// with the delete gone, the overlap is in the class under its general name
/// (D52).
#[test]
fn a_region_created_where_its_authors_refused_delete_left_none_is_deferred() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/two_modes/145-invariant-region-extents.txt");
    let text = std::fs::read_to_string(path).expect("readable");
    let classes = |history: &[epiphany_ops::OperationEnvelope]| -> Vec<String> {
        assert!(modes::valid(history));
        modes::findings(history)
            .into_iter()
            .map(|f| f.class)
            .collect()
    };
    let general = |found: &[String]| found == [modes::REGION_OVERLAP];
    let history = modes::parse(&text).expect("parses");
    let found = classes(&history);
    assert!(
        found.iter().any(|c| c == modes::REGION_SEEN_DELETED),
        "{found:?}"
    );

    // The second create's author saw the fill as well.
    let blind =
        "(stamp 63 0 #x00000000000000010000000000000004) (causal ((#x0000000000000001 3)) ())";
    assert_eq!(text.matches(blind).count(), 1, "the second region's create");
    let saw_fill = text.replace(
        blind,
        "(stamp 63 0 #x00000000000000010000000000000004) \
         (causal ((#x0000000000000001 3) (#x0000000000000003 0)) ())",
    );
    let found = classes(&modes::parse(&saw_fill).expect("parses"));
    assert!(general(&found), "{found:?}");

    // No delete: the author saw the region live.
    let without: Vec<_> = history
        .iter()
        .filter(|env| {
            !matches!(
                env.payload,
                epiphany_ops::OperationPayload::Primitive(
                    epiphany_ops::OperationKind::DeleteRegion(_)
                )
            )
        })
        .cloned()
        .collect();
    assert_eq!(without.len() + 1, history.len());
    let found = classes(&modes::compact(&without));
    assert!(general(&found), "{found:?}");
}

/// The deferred class's third cause (D51) is named apart, and holds only
/// where its author's view did not hold the region live because of its own
/// undo of the region's transaction, which the merged history blocks: an
/// author undoes the transaction that created a region another author has
/// concurrently filled, so the merged history keeps the region, and creates a
/// region in its place. With the second create's author having also seen the
/// fill, so that its view blocked the undo too and held the region, or with
/// the undo gone, the overlap is in the class under its general name (D52).
#[test]
fn a_region_created_where_its_authors_blocked_undo_left_none_is_deferred() {
    use epiphany_core::{
        InstrumentId, OperationId, RegionId, ReplicaId, StaffId, StaffInstanceId, TimeAnchor,
        TimeExtent, TransactionId, WallClockTime,
    };
    use epiphany_ops::{
        valuegen, AuthorId, CausalContext, CreateInstrumentOp, CreateRegionOp,
        CreateStaffInstanceOp, CreateStaffOp, HybridLogicalClock, OperationEnvelope, OperationKind,
        OperationPayload, OperationStamp, TransactionDescriptor, UndoPolicy,
        UndoTransactionPayload,
    };
    let (one, three) = (ReplicaId(1), ReplicaId(3));
    let mut history: Vec<OperationEnvelope> = Vec::new();
    let mut op = |replica: ReplicaId,
                  counter: u64,
                  seen: &[(ReplicaId, u64)],
                  transaction: Option<TransactionId>,
                  payload: OperationPayload| {
        let id = OperationId::new(replica, counter);
        let causal_context = seen
            .iter()
            .fold(CausalContext::new(), |c, (r, n)| c.with_seen(*r, *n));
        let clock = history.len() as i64 + 1;
        history.push(OperationEnvelope {
            id,
            author: AuthorId(u128::from(replica.0)),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(clock), 0), id),
            causal_context,
            transaction,
            payload,
        });
    };
    let prim = OperationPayload::Primitive;
    let instrument = InstrumentId::new(one, 100);
    let staff = StaffId::new(one, 101);
    let tx = TransactionId::new(one, 102);
    let first = RegionId::new(one, 103);
    let second = RegionId::new(one, 104);
    let region = |id| {
        let mut region = valuegen::region(id);
        region.time_extent = TimeExtent {
            start: TimeAnchor::WallClock {
                time: WallClockTime(1),
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(1001),
            },
        };
        region
    };
    op(
        one,
        0,
        &[],
        None,
        prim(OperationKind::CreateInstrument(CreateInstrumentOp {
            instrument: valuegen::instrument(instrument),
        })),
    );
    op(
        one,
        1,
        &[(one, 0)],
        None,
        prim(OperationKind::CreateStaff(CreateStaffOp {
            staff: valuegen::staff(staff, instrument),
        })),
    );
    op(
        one,
        2,
        &[(one, 1)],
        Some(tx),
        prim(OperationKind::DeclareTransaction(TransactionDescriptor {
            id: tx,
            label: String::from("a region"),
            category: None,
        })),
    );
    op(
        one,
        3,
        &[(one, 2)],
        Some(tx),
        prim(OperationKind::CreateRegion(CreateRegionOp {
            region: region(first),
        })),
    );
    // Another author fills it, unseen by the first.
    op(
        three,
        0,
        &[(one, 3)],
        None,
        prim(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
            region: first,
            instance: valuegen::staff_instance(StaffInstanceId::new(three, 105), staff),
        })),
    );
    op(
        one,
        4,
        &[(one, 3)],
        None,
        OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: tx,
            policy: UndoPolicy::BestEffort,
        }),
    );
    op(
        one,
        5,
        &[(one, 4)],
        None,
        prim(OperationKind::CreateRegion(CreateRegionOp {
            region: region(second),
        })),
    );
    op(
        one,
        6,
        &[(one, 5)],
        None,
        prim(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
            region: second,
            instance: valuegen::staff_instance(StaffInstanceId::new(one, 106), staff),
        })),
    );
    let classes = |history: &[OperationEnvelope]| -> Vec<String> {
        assert!(modes::valid(history));
        modes::findings(history)
            .into_iter()
            .map(|f| f.class)
            .collect()
    };
    let general = |found: &[String]| found == [modes::REGION_OVERLAP];
    let found = classes(&history);
    assert!(
        found.iter().any(|c| c == modes::REGION_SEEN_UNDONE),
        "{found:?}"
    );
    assert!(modes::deferred(modes::REGION_SEEN_UNDONE));

    // The second create's author saw the fill as well.
    let saw_fill: Vec<OperationEnvelope> = history
        .iter()
        .cloned()
        .map(|mut env| {
            if env.id == OperationId::new(one, 5) {
                env.causal_context = env.causal_context.with_seen(three, 0);
            }
            env
        })
        .collect();
    let found = classes(&saw_fill);
    assert!(general(&found), "{found:?}");

    // No undo: the author saw the region live.
    let without: Vec<OperationEnvelope> = history
        .iter()
        .filter(|env| !matches!(env.payload, OperationPayload::UndoTransaction(_)))
        .cloned()
        .collect();
    assert_eq!(without.len() + 1, history.len());
    let found = classes(&modes::compact(&without));
    assert!(general(&found), "{found:?}");
}

/// The committed history `name` (`tests/two_modes/<name>`), parsed.
fn committed_history(name: &str) -> Vec<epiphany_ops::OperationEnvelope> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/two_modes")
        .join(name);
    let text = std::fs::read_to_string(&path).expect("readable");
    modes::parse(&text).expect("parses")
}

/// A marker its event's delete orphans is anchored to its region's start
/// where the verdicts read it, so it holds the region as any marker anchored
/// there does, in both modes (C4b review 1's M2, found at `0xc41ea269`): the
/// region's undo, by a concurrent author or by one who saw the delete,
/// conflicts under `Cascade` and `StrictInverse`, a `BestEffort` one keeps the
/// region, and a `DeleteRegion` is refused `ContainerNotEmpty`, the marker
/// standing at the region's start in the graph each time; and so when the
/// marker was first re-anchored to another note, whose delete then orphans it
/// (`173`); and when two modifies wrote the marker before its orphaning, so
/// the repair must rewrite its chain's last write, not its first (`180`, C4b
/// review 2's L1). Before the fix the repair moved the graph's anchor alone, and each
/// removed the region from under the marker.
#[test]
fn an_orphaned_marker_holds_its_region_in_both_modes() {
    use epiphany_core::{
        AnchorOffset, IdentityContext, RegionEdge, ReplicaId, Score, TimeAnchor, TypedObjectId,
    };
    use epiphany_ops::{
        NoOpReason, ObjectState, OperationEffect, OperationSet, PreconditionFailureReason,
    };
    for (name, last) in [
        ("168-invariant-cross-cutting-refs-resolve.txt", "conflicted"),
        ("169-invariant-cross-cutting-refs-resolve.txt", "conflicted"),
        ("170-invariant-cross-cutting-refs-resolve.txt", "conflicted"),
        ("171-invariant-cross-cutting-refs-resolve.txt", "repaired"),
        ("172-invariant-cross-cutting-refs-resolve.txt", "refused"),
        ("173-invariant-cross-cutting-refs-resolve.txt", "refused"),
        ("180-invariant-cross-cutting-refs-resolve.txt", "refused"),
    ] {
        let history = committed_history(name);
        assert!(
            modes::findings(&history).is_empty(),
            "{name}: the modes agree"
        );
        let mut set = OperationSet::new();
        set.accept_all(history.iter().cloned());
        let free = set.reduce();
        let aware = set.reduce_onto(&Score::empty(IdentityContext::new(ReplicaId(0))));
        for state in [&free, &aware.state] {
            let effect = state
                .effects
                .iter()
                .find(|(id, _)| *id == history.last().expect("a history").id)
                .map(|(_, e)| e.clone());
            match last {
                "conflicted" => assert!(
                    matches!(effect, Some(OperationEffect::Conflicted { .. })),
                    "{name}: {effect:?}"
                ),
                "repaired" => assert!(
                    matches!(effect, Some(OperationEffect::AppliedWithRepair { .. })),
                    "{name}: {effect:?}"
                ),
                _ => assert_eq!(
                    effect,
                    Some(OperationEffect::NoOp {
                        reason: NoOpReason::PreconditionFailedUnderReduction {
                            reason: PreconditionFailureReason::ContainerNotEmpty,
                        },
                    }),
                    "{name}"
                ),
            }
        }
        let repairs: Vec<_> = free
            .effects
            .iter()
            .filter_map(|(_, e)| match e {
                OperationEffect::AppliedWithRepair { repairs } => Some(repairs),
                _ => None,
            })
            .flatten()
            .map(|r| format!("{:?}", r.kind))
            .filter(|kind| kind.starts_with("Reanchored") || kind.starts_with("Orphaned"))
            .collect();
        let reanchored = repairs
            .iter()
            .filter(|k| k.starts_with("Reanchored"))
            .count();
        assert_eq!(
            (reanchored, repairs.len() - reanchored),
            (usize::from(name.starts_with("173")), 1),
            "{name}: {repairs:?}"
        );
        let markers = &aware.score.cross_cutting.markers;
        assert_eq!(markers.len(), 1, "{name}");
        let TimeAnchor::Region { id, edge, offset } = &markers[0].anchor else {
            panic!(
                "{name}: the orphan stands at its region, not {:?}",
                markers[0].anchor
            );
        };
        assert_eq!(
            (edge, offset),
            (&RegionEdge::Start, &AnchorOffset::Zero),
            "{name}"
        );
        assert!(
            matches!(
                free.objects.get(&TypedObjectId::Region(*id)),
                Some(ObjectState::Live)
            ),
            "{name}: the region stands"
        );
        assert!(
            matches!(
                free.objects.get(&TypedObjectId::Marker(markers[0].id)),
                Some(ObjectState::Live)
            ),
            "{name}: the marker stands"
        );
    }
}

/// An undo's region guard reads a live marker on an event the undo removes as
/// the repair will leave it, so a marker the undo would orphan holds the
/// region it would stand at, in both modes (C4b review 2's M1, one author): a
/// region created in one transaction with an instance, a voice and a note, a
/// tempo mark anchored to the note, then the transaction undone. Strict and
/// cascading undos conflict (`174`, `175`, and `177`, whose second note the
/// undo removes too), and a best-effort one keeps the region, taking the
/// instance, the voice and the note, the marker orphaned to the region's
/// start (`176`). Where the region stands outside the transaction the undo
/// applies and the orphan keeps it (`178`, a control). Before the fix the
/// guard read the marker at its note, and each of `174` to `177` removed the
/// region from under the orphan (`CrossCuttingRefsResolve`). A marker whose
/// note the undo removes beside another note of its voice the undo leaves is
/// read re-anchored to that note, so the voice holding the note is what
/// refuses a strict undo, not the region holding the marker (`179`).
#[test]
fn a_marker_its_regions_undo_would_orphan_holds_the_region_in_both_modes() {
    use epiphany_core::{
        AnchorOffset, IdentityContext, RegionEdge, ReplicaId, Score, TimeAnchor, TypedObjectId,
    };
    use epiphany_ops::{ObjectState, OperationEffect, OperationSet, RepairKind};
    for (name, last) in [
        ("174-invariant-cross-cutting-refs-resolve.txt", "conflicted"),
        ("175-invariant-cross-cutting-refs-resolve.txt", "conflicted"),
        ("176-invariant-cross-cutting-refs-resolve.txt", "repaired"),
        ("177-invariant-cross-cutting-refs-resolve.txt", "conflicted"),
        (
            "178-control-orphan-region-outside-transaction.txt",
            "repaired",
        ),
    ] {
        let history = committed_history(name);
        assert!(
            modes::findings(&history).is_empty(),
            "{name}: the modes agree and the graph holds"
        );
        let mut set = OperationSet::new();
        set.accept_all(history.iter().cloned());
        let free = set.reduce();
        let aware = set.reduce_onto(&Score::empty(IdentityContext::new(ReplicaId(0))));
        let regions = &aware.score.canvas.regions;
        assert_eq!(regions.len(), 1, "{name}: the region stands");
        let region = regions[0].id;
        let markers = &aware.score.cross_cutting.markers;
        assert_eq!(markers.len(), 1, "{name}: the marker stands");
        for state in [&free, &aware.state] {
            let effect = state
                .effects
                .iter()
                .find(|(id, _)| *id == history.last().expect("a history").id)
                .map(|(_, e)| e.clone());
            assert!(
                matches!(
                    state.objects.get(&TypedObjectId::Region(region)),
                    Some(ObjectState::Live)
                ),
                "{name}: the region is live"
            );
            assert!(
                matches!(
                    state.objects.get(&TypedObjectId::Marker(markers[0].id)),
                    Some(ObjectState::Live)
                ),
                "{name}: the marker is live"
            );
            match (last, effect) {
                ("conflicted", Some(OperationEffect::Conflicted { .. })) => {}
                ("repaired", Some(OperationEffect::AppliedWithRepair { repairs })) => {
                    assert!(
                        repairs.iter().any(|r| r.kind == RepairKind::Orphaned
                            && r.target == TypedObjectId::Marker(markers[0].id)),
                        "{name}: the undo orphans the marker, {repairs:?}"
                    );
                }
                (_, effect) => panic!("{name}: expected {last}, the undo was {effect:?}"),
            }
        }
        match last {
            "conflicted" => assert!(
                matches!(markers[0].anchor, TimeAnchor::Event { .. }),
                "{name}: the refused undo leaves the marker on its note"
            ),
            _ => assert_eq!(
                markers[0].anchor,
                TimeAnchor::Region {
                    id: region,
                    edge: RegionEdge::Start,
                    offset: AnchorOffset::Zero,
                },
                "{name}: the orphan stands at its region's start"
            ),
        }
    }
    let name = "179-control-marker-reanchored-in-its-voice.txt";
    let history = committed_history(name);
    assert!(
        modes::findings(&history).is_empty(),
        "{name}: the modes agree"
    );
    let mut set = OperationSet::new();
    set.accept_all(history.iter().cloned());
    let free = set.reduce();
    let aware = set.reduce_onto(&Score::empty(IdentityContext::new(ReplicaId(0))));
    let (_, _, voice) = aware.score.voices().next().expect("the voice stands");
    let kept = voice.events.last().copied().expect("its notes stand");
    for state in [&free, &aware.state] {
        let Some((_, OperationEffect::Conflicted { conflict })) = state
            .effects
            .iter()
            .find(|(id, _)| *id == history.last().expect("a history").id)
        else {
            panic!("{name}: the strict undo is refused");
        };
        let record = state
            .conflicts
            .records()
            .iter()
            .find(|record| record.id == *conflict)
            .expect("the undo's conflict");
        let mut affected = record.affected_objects.clone();
        affected.sort();
        assert_eq!(
            affected,
            vec![TypedObjectId::Event(kept), TypedObjectId::Voice(voice.id)],
            "{name}: the voice and the note it holds refuse the undo"
        );
    }
}

/// An undo's region guard reads a marker as the whole undo leaves it, its
/// restoration included, so a marker the undone transaction itself moved onto
/// the region it removes holds nothing, in both modes (C4b review 3's L1, one
/// author): a marker on a note of an older region, then one transaction
/// creating a region with an instance, a voice and a note and moving the
/// marker onto that note (`181` to `183`) or onto the new region's start
/// (`184` to `186`), then the transaction undone under each policy. The
/// undo's restoration returns the marker to the older note, so the undo
/// applies under every policy: the new region goes with its contents and the
/// marker stands on the older note. Before the fix both arms of the guard read
/// the marker where the transaction had put it, so strict and cascading undos
/// conflicted and a best-effort one kept the emptied region.
#[test]
fn a_marker_its_undo_restores_holds_no_region_in_both_modes() {
    use epiphany_core::{
        AnchorOffset, IdentityContext, ReplicaId, Score, TimeAnchor, TypedObjectId,
    };
    use epiphany_ops::{ObjectState, OperationEffect, OperationSet, RepairKind};
    for name in [
        "181-undo-transaction.txt",
        "182-undo-transaction.txt",
        "183-undo-transaction.txt",
        "184-undo-transaction.txt",
        "185-undo-transaction.txt",
        "186-undo-transaction.txt",
    ] {
        let history = committed_history(name);
        assert!(
            modes::findings(&history).is_empty(),
            "{name}: the modes agree and the graph holds"
        );
        let mut set = OperationSet::new();
        set.accept_all(history.iter().cloned());
        let free = set.reduce();
        let aware = set.reduce_onto(&Score::empty(IdentityContext::new(ReplicaId(0))));
        let regions = &aware.score.canvas.regions;
        assert_eq!(regions.len(), 1, "{name}: the older region alone stands");
        let older = regions[0].id;
        let mut minted: Vec<_> = free
            .objects
            .keys()
            .filter_map(|object| match object {
                TypedObjectId::Region(region) if *region != older => Some(*region),
                _ => None,
            })
            .collect();
        assert_eq!(minted.len(), 1, "{name}: one region the transaction made");
        let undone = minted.remove(0);
        let (_, _, voice) = aware.score.voices().next().expect("the older voice");
        assert_eq!(aware.score.voices().count(), 1, "{name}: the voice goes");
        let note = voice.events.first().copied().expect("the older note");
        let markers = &aware.score.cross_cutting.markers;
        assert_eq!(markers.len(), 1, "{name}: the marker stands");
        assert_eq!(
            markers[0].anchor,
            TimeAnchor::Event {
                id: note,
                offset: AnchorOffset::Zero,
            },
            "{name}: the restoration returns the marker to the older note"
        );
        for state in [&free, &aware.state] {
            let effect = state
                .effects
                .iter()
                .find(|(id, _)| *id == history.last().expect("a history").id)
                .map(|(_, e)| e.clone());
            let Some(OperationEffect::AppliedWithRepair { repairs }) = effect else {
                panic!("{name}: the undo applies, not {effect:?}");
            };
            assert!(
                repairs.iter().any(|r| r.kind == RepairKind::CascadeDeleted
                    && r.target == TypedObjectId::Region(undone)),
                "{name}: the undo removes the transaction's region, {repairs:?}"
            );
            assert!(
                matches!(
                    state.objects.get(&TypedObjectId::Region(undone)),
                    Some(ObjectState::Tombstoned { .. })
                ),
                "{name}: the transaction's region is gone"
            );
            assert!(
                matches!(
                    state.objects.get(&TypedObjectId::Marker(markers[0].id)),
                    Some(ObjectState::Live)
                ),
                "{name}: the marker is live"
            );
        }
    }
}
