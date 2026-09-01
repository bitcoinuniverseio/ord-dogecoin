use {
  bitcoin::{
    blockdata::{opcodes, script::Builder},
    PackedLockTime, Transaction, TxOut,
  },
  ord::{Dune, DuneId, Dunestone, Edict},
  redb::{Database, MultimapTableDefinition, TableDefinition},
  std::{fs, process::Command, str::FromStr},
  tempfile::TempDir,
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

#[test]
fn repair_address_index_removes_spent_rows_and_backfills_live_rows() {
  const STATISTIC_TO_COUNT: TableDefinition<u64, u64> = TableDefinition::new("STATISTIC_TO_COUNT");
  const OUTPOINT_TO_VALUE: TableDefinition<&[u8; 36], u64> =
    TableDefinition::new("OUTPOINT_TO_VALUE");
  const OUTPOINT_TO_ADDRESS: TableDefinition<&[u8; 36], &[u8]> =
    TableDefinition::new("OUTPOINT_TO_ADDRESS");
  const ADDRESS_TO_OUTPOINT: MultimapTableDefinition<&[u8], &[u8; 36]> =
    MultimapTableDefinition::new("ADDRESS_TO_OUTPOINT");
  let tempdir = TempDir::new().unwrap();
  let index_path = tempdir.path().join("address-repair.redb");
  let cookie_path = tempdir.path().join("cookie");
  fs::write(&cookie_path, "user:password").unwrap();

  let live = [1_u8; 36];
  let stale = [2_u8; 36];
  let address = b"nZg3mF9K6H7vR8qS2tU4wX5yA1bC3dE4fG";

  {
    let database = Database::create(&index_path).unwrap();
    let wtx = database.begin_write().unwrap();
    {
      let mut statistics = wtx.open_table(STATISTIC_TO_COUNT).unwrap();
      statistics.insert(&1, &0).unwrap();
      statistics.insert(&2, &0).unwrap();
      statistics.insert(&3, &0).unwrap();
      statistics.insert(&9, &6).unwrap();
      statistics.insert(&10, &0).unwrap();
    }
    wtx
      .open_table(OUTPOINT_TO_VALUE)
      .unwrap()
      .insert(&live, &1)
      .unwrap();
    {
      let mut addresses = wtx.open_multimap_table(ADDRESS_TO_OUTPOINT).unwrap();
      addresses.insert(address.as_slice(), &live).unwrap();
      addresses.insert(address.as_slice(), &stale).unwrap();
    }
    wtx.commit().unwrap();
  }

  let output = Command::new(executable_path::executable_path("ord"))
    .arg("--chain=regtest")
    .arg("--index")
    .arg(&index_path)
    .arg("--rpc-url=http://127.0.0.1:1")
    .arg("--cookie-file")
    .arg(&cookie_path)
    .arg("repair-address-index")
    .args(["--address-batch-size", "1"])
    .output()
    .unwrap();

  assert!(
    output.status.success(),
    "repair failed: {}",
    String::from_utf8_lossy(&output.stderr)
  );

  let database = Database::open(&index_path).unwrap();
  let rtx = database.begin_read().unwrap();
  assert_eq!(
    rtx
      .open_table(OUTPOINT_TO_ADDRESS)
      .unwrap()
      .get(&live)
      .unwrap()
      .unwrap()
      .value(),
    address
  );
  let address_table = rtx.open_multimap_table(ADDRESS_TO_OUTPOINT).unwrap();
  let remaining = address_table
    .get(address.as_slice())
    .unwrap()
    .map(|item| *item.unwrap().value())
    .collect::<Vec<_>>();
  assert_eq!(remaining, vec![live]);
}
