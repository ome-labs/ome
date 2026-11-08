//! DKG and reshare state machines driven in memory: agreement, ECDH through the result, culprit detection,
//! unattributable disputes, equivocation and timeouts.
#![allow(
    clippy::unwrap_used,
    clippy::missing_panics_doc,
    clippy::too_many_lines
)]

use std::collections::VecDeque;

use ome_crypto::{
    Base, Point, Scalar, SecretScalar,
    aztec::AddressContext,
    sealing::envelope::{self, Context, TransportPublic, TransportSecret},
    threshold::{
        dkg::KeyShare,
        ecdh::{AddressSecretShare, EcdhContext, combine},
    },
};
use ome_keeper::ceremony::{
    Outbox, Participant, Status, Target,
    dkg::{Dkg, DkgParams},
    reshare::{Reshare, ReshareParams},
};
use ome_wire::{
    IdentityPublic, KeeperIdentity, Message,
    messages::ceremony::{Justification, SealedShare},
    types::{WirePoint, scalar},
};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

const TIMEOUT: u64 = 60;

struct Node {
    identity: KeeperIdentity,
    transport: TransportSecret,
}

impl Node {
    fn new(seed: u8, rng: &mut ChaCha20Rng) -> Self {
        let (transport, _) = TransportSecret::generate(rng);
        Self {
            identity: KeeperIdentity::from_seed(&[seed; 32]),
            transport,
        }
    }

    fn participant(&self) -> Participant {
        Participant {
            identity: self.identity.public(),
            transport: self.transport.public().unwrap().0,
            addresses: Vec::new(),
        }
    }
}

/// A ceremony instance of either kind, as the router sees it.
enum Machine {
    Dkg(Dkg),
    Reshare(Reshare),
    Silent,
}

impl Machine {
    fn handle(
        &mut self,
        from: &IdentityPublic,
        message: Message,
        transport: &TransportSecret,
        rng: &mut ChaCha20Rng,
    ) -> Outbox {
        match self {
            Self::Dkg(dkg) => dkg.handle(from, message, transport, 0, rng),
            Self::Reshare(reshare) => reshare.handle(from, message, transport, 0),
            Self::Silent => Outbox::default(),
        }
    }

    fn status(&self) -> Status {
        match self {
            Self::Dkg(dkg) => dkg.status().clone(),
            Self::Reshare(reshare) => reshare.status().clone(),
            Self::Silent => Status::Running,
        }
    }
}

/// Delivers messages until quiet. `tamper(from, to, message)` may rewrite or drop (returns `false`) a message.
fn route(
    nodes: &[Node],
    machines: &mut [Machine],
    initial: Vec<(usize, Outbox)>,
    rng: &mut ChaCha20Rng,
    mut tamper: impl FnMut(usize, usize, &mut Message) -> bool,
) {
    let mut queue: VecDeque<(usize, usize, Message)> = VecDeque::new();
    let expand = |from: usize, outbox: Outbox, queue: &mut VecDeque<(usize, usize, Message)>| {
        for (target, message) in outbox.0 {
            for (to, node) in nodes.iter().enumerate() {
                let wanted = match &target {
                    Target::All => to != from,
                    Target::One(identity) => &node.identity.public() == identity,
                };
                if wanted {
                    queue.push_back((from, to, message.clone()));
                }
            }
        }
    };
    for (from, outbox) in initial {
        expand(from, outbox, &mut queue);
    }
    while let Some((from, to, mut message)) = queue.pop_front() {
        if !tamper(from, to, &mut message) {
            continue;
        }
        let sender = nodes[from].identity.public();
        let outbox = machines[to].handle(&sender, message, &nodes[to].transport, rng);
        expand(to, outbox, &mut queue);
    }
}

fn dkg(
    nodes: &[Node],
    threshold: u32,
    active: &[usize],
    rng: &mut ChaCha20Rng,
) -> (Vec<Machine>, Vec<(usize, Outbox)>) {
    let params = DkgParams {
        session: [42; 32],
        threshold,
        members: nodes.iter().map(Node::participant).collect(),
    };
    let mut machines = Vec::new();
    let mut initial = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        if active.contains(&i) {
            let (machine, outbox) =
                Dkg::start(params.clone(), &node.identity.public(), 0, TIMEOUT, rng).unwrap();
            machines.push(Machine::Dkg(machine));
            initial.push((i, outbox));
        } else {
            machines.push(Machine::Silent);
        }
    }
    (machines, initial)
}

fn take_dkg(machine: &mut Machine) -> KeyShare {
    match machine {
        Machine::Dkg(dkg) => dkg.take_result().unwrap(),
        _ => panic!("not a dkg"),
    }
}

fn blamed(status: &Status) -> Vec<String> {
    match status {
        Status::Aborted(failure) => failure.blamed.clone(),
        other => panic!("expected an abort, got {other:?}"),
    }
}

/// `address_secret · P` from `shares` (any qualified subset).
fn ecdh_with(
    shares: &[&KeyShare],
    context: &AddressContext,
    input: &Point,
    rng: &mut ChaCha20Rng,
) -> Point {
    let ecdh_context = EcdhContext {
        vault: context.address(),
        epoch: 0,
    };
    let partials: Vec<_> = shares
        .iter()
        .map(|s| {
            AddressSecretShare::new(s.index(), s.share(), context)
                .unwrap()
                .partial(input, &ecdh_context, rng)
                .unwrap()
        })
        .collect();
    combine(&partials, partials.len()).unwrap()
}

fn hex_id(node: &Node) -> String {
    hex::encode(node.identity.public().0)
}

#[test]
fn dkg_agrees_and_any_qualified_set_computes_the_same_ecdh() {
    let mut rng = ChaCha20Rng::seed_from_u64(1);
    let nodes: Vec<Node> = (1..=5).map(|i| Node::new(i, &mut rng)).collect();
    let (mut machines, initial) = dkg(&nodes, 3, &[0, 1, 2, 3, 4], &mut rng);
    route(&nodes, &mut machines, initial, &mut rng, |_, _, _| true);
    assert!(machines.iter().all(|m| m.status() == Status::Completed));
    let shares: Vec<KeyShare> = machines.iter_mut().map(take_dkg).collect();
    assert!(
        shares
            .iter()
            .all(|s| s.group_public_key() == shares[0].group_public_key())
    );
    assert!(
        shares
            .iter()
            .all(|s| s.transcript() == shares[0].transcript())
    );

    let context = AddressContext::new(Base::from(77u64), shares[0].group_public_key()).unwrap();
    let input = Point::generator_mul_secret(&SecretScalar::random(&mut rng)).unwrap();
    let a = ecdh_with(
        &[&shares[0], &shares[1], &shares[2]],
        &context,
        &input,
        &mut rng,
    );
    let b = ecdh_with(
        &[&shares[4], &shares[2], &shares[3]],
        &context,
        &input,
        &mut rng,
    );
    assert_eq!(a, b);
}

/// Replaces dealer 3's share for member 1 with `wrong`.
fn corrupt_share(
    nodes: &[Node],
    wrong: &SecretScalar,
    rng: &mut ChaCha20Rng,
) -> impl FnMut(usize, usize, &mut Message) -> bool + use<> {
    let sender = TransportSecret::from_bytes(*nodes[2].transport.to_bytes());
    let recipient = TransportPublic(nodes[0].transport.public().unwrap().0);
    let envelope = envelope::seal_scalar(
        &Context {
            session: [42; 32],
            epoch: 0,
            kind: 1,
            sender: 3,
            recipient: 1,
        },
        &sender,
        &recipient,
        wrong,
        rng,
    )
    .unwrap();
    move |from, to, message| {
        if from == 2 && to == 0 && matches!(message, Message::DkgRound2(_)) {
            *message = Message::DkgRound2(SealedShare::new(3, 1, envelope.clone()));
        }
        true
    }
}

#[test]
fn a_bad_share_that_the_dealer_stands_by_blames_the_dealer() {
    let mut rng = ChaCha20Rng::seed_from_u64(2);
    let nodes: Vec<Node> = (1..=3).map(|i| Node::new(i, &mut rng)).collect();
    let (mut machines, initial) = dkg(&nodes, 2, &[0, 1, 2], &mut rng);
    let wrong = SecretScalar::random(&mut rng);
    let wrong_wire = scalar(wrong.expose());
    let mut corrupt = corrupt_share(&nodes, &wrong, &mut rng);
    route(
        &nodes,
        &mut machines,
        initial,
        &mut rng,
        |from, to, message| {
            // The dealer "justifies" with the share it actually sent.
            if let Message::Justification(j) = message
                && from == 2
            {
                *message = Message::Justification(Justification {
                    share: wrong_wire,
                    ..*j
                });
            }
            corrupt(from, to, message)
        },
    );
    // Every honest member blames the dealer; the dealer itself just stops.
    for machine in &machines[..2] {
        assert_eq!(blamed(&machine.status()), vec![hex_id(&nodes[2])]);
    }
    assert_eq!(blamed(&machines[2].status()), Vec::<String>::new());
}

#[test]
fn a_dispute_settled_by_a_valid_justification_blames_nobody() {
    let mut rng = ChaCha20Rng::seed_from_u64(3);
    let nodes: Vec<Node> = (1..=3).map(|i| Node::new(i, &mut rng)).collect();
    let (mut machines, initial) = dkg(&nodes, 2, &[0, 1, 2], &mut rng);
    let wrong = SecretScalar::random(&mut rng);
    let corrupt = corrupt_share(&nodes, &wrong, &mut rng);
    route(&nodes, &mut machines, initial, &mut rng, corrupt);
    for machine in &machines {
        assert_eq!(blamed(&machine.status()), Vec::<String>::new());
    }
}

#[test]
fn an_equivocating_dealer_never_yields_a_key() {
    let mut rng = ChaCha20Rng::seed_from_u64(4);
    let nodes: Vec<Node> = (1..=3).map(|i| Node::new(i, &mut rng)).collect();
    let (mut machines, initial) = dkg(&nodes, 2, &[0, 1, 2], &mut rng);
    // Member 2 also runs a second, different round 1 and shows it to member 1 only.
    let params = DkgParams {
        session: [42; 32],
        threshold: 2,
        members: nodes.iter().map(Node::participant).collect(),
    };
    let (_, other) = Dkg::start(params, &nodes[1].identity.public(), 0, TIMEOUT, &mut rng).unwrap();
    let forged = other.0[0].1.clone();
    route(
        &nodes,
        &mut machines,
        initial,
        &mut rng,
        |from, to, message| {
            if from == 1 && to == 0 && matches!(message, Message::DkgRound1(_)) {
                *message = forged.clone();
            }
            true
        },
    );
    assert!(machines.iter().all(|m| m.status() != Status::Completed));
}

#[test]
fn a_silent_member_times_out_and_is_blamed() {
    let mut rng = ChaCha20Rng::seed_from_u64(5);
    let nodes: Vec<Node> = (1..=3).map(|i| Node::new(i, &mut rng)).collect();
    let (mut machines, initial) = dkg(&nodes, 2, &[0, 1], &mut rng);
    route(&nodes, &mut machines, initial, &mut rng, |_, _, _| true);
    let Machine::Dkg(first) = &mut machines[0] else {
        panic!()
    };
    assert_eq!(*first.status(), Status::Running);
    assert_eq!(first.tick(TIMEOUT).0, vec![]);
    let abort = first.tick(TIMEOUT + 1);
    assert_eq!(abort.0.len(), 1);
    assert_eq!(blamed(first.status()), vec![hex_id(&nodes[2])]);
}

struct Committee {
    nodes: Vec<Node>,
    shares: Vec<KeyShare>,
}

fn committee(rng: &mut ChaCha20Rng) -> Committee {
    let nodes: Vec<Node> = (1..=3).map(|i| Node::new(i, rng)).collect();
    let (mut machines, initial) = dkg(&nodes, 2, &[0, 1, 2], rng);
    route(&nodes, &mut machines, initial, rng, |_, _, _| true);
    let shares = machines.iter_mut().map(take_dkg).collect();
    Committee { nodes, shares }
}

/// Reshares from dealers {1, 2} of `old` to {1, 2, new} (member 3 leaves, a new member joins).
fn reshare(
    old: &Committee,
    newcomer: Node,
    rng: &mut ChaCha20Rng,
    tamper: impl FnMut(usize, usize, &mut Message) -> bool,
) -> (Vec<Node>, Vec<Machine>) {
    let mut nodes: Vec<Node> = old
        .nodes
        .iter()
        .map(|n| Node {
            identity: KeeperIdentity::from_seed(&n.identity.seed()),
            transport: TransportSecret::from_bytes(*n.transport.to_bytes()),
        })
        .collect();
    nodes.push(newcomer);
    let params = ReshareParams {
        session: [43; 32],
        vault: [9; 32],
        old_epoch: 0,
        old_threshold: 2,
        old_members: old.nodes.iter().map(Node::participant).collect(),
        old_verification_shares: old.shares[0]
            .verification_shares()
            .unwrap()
            .iter()
            .map(WirePoint::from)
            .collect(),
        dealers: vec![1, 2],
        new_threshold: 2,
        new_members: vec![
            nodes[0].participant(),
            nodes[1].participant(),
            nodes[3].participant(),
        ],
    };
    let mut machines = Vec::new();
    let mut initial = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        let current = old.shares.get(i).filter(|_| i < 2);
        match Reshare::start(
            params.clone(),
            &node.identity.public(),
            current,
            0,
            TIMEOUT,
            rng,
        ) {
            Ok((machine, mut outbox)) => {
                outbox
                    .0
                    .extend(machine.seal_sub_shares(&node.transport, rng).unwrap().0);
                machines.push(Machine::Reshare(machine));
                initial.push((i, outbox));
            }
            Err(_) => machines.push(Machine::Silent),
        }
    }
    route(&nodes, &mut machines, initial, rng, tamper);
    (nodes, machines)
}

#[test]
fn reshare_keeps_the_secret_and_retires_old_shares() {
    let mut rng = ChaCha20Rng::seed_from_u64(6);
    let old = committee(&mut rng);
    let newcomer = Node::new(4, &mut rng);
    let (_, mut machines) = reshare(&old, newcomer, &mut rng, |_, _, _| true);
    // Member 3 is neither dealer nor new member: it takes no part.
    assert!(matches!(machines[2], Machine::Silent));
    let mut new_shares = Vec::new();
    for i in [0, 1, 3] {
        let machine = &mut machines[i];
        assert_eq!(machine.status(), Status::Completed);
        let Machine::Reshare(r) = machine else {
            panic!()
        };
        new_shares.push(r.take_result().unwrap());
    }
    assert!(
        new_shares
            .iter()
            .all(|s| s.group_public_key() == old.shares[0].group_public_key())
    );

    let context = AddressContext::new(Base::from(5u64), old.shares[0].group_public_key()).unwrap();
    let input = Point::generator_mul_secret(&SecretScalar::random(&mut rng)).unwrap();
    let before = ecdh_with(
        &[&old.shares[0], &old.shares[2]],
        &context,
        &input,
        &mut rng,
    );
    let after = ecdh_with(
        &[&new_shares[1], &new_shares[2]],
        &context,
        &input,
        &mut rng,
    );
    assert_eq!(before, after);
    // The removed member's old share does not combine with new shares.
    let mixed = ecdh_with(
        &[&old.shares[2], &new_shares[1]],
        &context,
        &input,
        &mut rng,
    );
    assert_ne!(mixed, before);
}

#[test]
fn a_dealer_dealing_another_value_is_blamed() {
    let mut rng = ChaCha20Rng::seed_from_u64(7);
    let old = committee(&mut rng);
    let dealer_two = hex_id(&old.nodes[1]);
    let newcomer = Node::new(4, &mut rng);
    let fake = WirePoint::from(
        &Point::generator_mul_secret(&SecretScalar::new(Scalar::from(99u64))).unwrap(),
    );
    let (_, machines) = reshare(&old, newcomer, &mut rng, |from, _, message| {
        if from == 1
            && let Message::ReshareDeal(deal) = message
        {
            deal.commitments[0] = fake;
        }
        true
    });
    for machine in [&machines[0], &machines[3]] {
        assert_eq!(blamed(&machine.status()), vec![dealer_two.clone()]);
    }
}
