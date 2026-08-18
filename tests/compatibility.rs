use {
  bitcoin::{
    blockdata::{opcodes, script::Builder},
    PackedLockTime, Transaction, TxOut,
  },
  ord::{Dune, DuneId, Dunestone, Edict},
  std::str::FromStr,
};

fn transaction(script_pubkey: bitcoin::Script) -> Transaction {
  Transaction {
    version: 1,
    lock_time: PackedLockTime::ZERO,
    input: Vec::new(),
    output: vec![
      TxOut {
        value: 1,
        script_pubkey: bitcoin::Script::new(),
      },
      TxOut {
        value: 0,
        script_pubkey,
      },
    ],
  }
}

#[test]
fn dune_identifiers_round_trip_through_the_public_api() {
  let dune = Dune(0);
  assert_eq!(Dune::from_str(&dune.to_string()).unwrap(), dune);

  let id = DuneId {
    height: 5_084_000,
    index: 7,
  };
  assert_eq!(DuneId::from_str(&id.to_string()).unwrap(), id);
}

#[test]
fn dunestone_edicts_round_trip_with_the_pinned_dogecoin_script_api() {
  let expected = Dunestone {
    edicts: vec![Edict {
      id: u128::from(DuneId {
        height: 5_084_000,
        index: 0,
      }),
      amount: 42,
      output: 0,
    }],
    pointer: Some(0),
    ..Default::default()
  };

  let actual = Dunestone::from_transaction(&transaction(expected.encipher()));
  assert_eq!(actual, Some(expected));
}

#[test]
fn non_dune_op_return_is_not_interpreted_as_a_dunestone() {
  let script = Builder::new()
    .push_opcode(opcodes::all::OP_RETURN)
    .push_slice(b"not-a-dune")
    .into_script();

  assert_eq!(Dunestone::from_transaction(&transaction(script)), None);
}

#[test]
fn configured_redb_cache_applies_before_existing_index_open() {
  let source = include_str!("../src/index.rs");
  let configure = source
    .find("database_builder.set_cache_size(db_cache_size)")
    .expect("redb cache must be configured");
  let open = source
    .find("database_builder.open(&path)")
    .expect("existing redb index must use the configured builder");

  assert!(configure < open);
}
