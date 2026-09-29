//! Parser profile `doginals-trac-1.0.2-compat-v1` (P-03).
//!
//! The compat parser decided every inscription in existing databases, so its
//! outcomes are pinned here byte for byte, including the PUSHDATA2/PUSHDATA4
//! length quirk (findings P-F03): the length is read from the opcode byte and
//! the byte(s) after it instead of the following two/four little-endian
//! bytes. Each quirk vector also pins what Dogecoin Core's push decoding
//! yields, which is what `ord dogemap-pushdata-audit` compares. Expected
//! values are written out from the script bytes, not computed by the code
//! under test. The chain cases run the production indexer through the `ord`
//! binary.

mod dogemap_support;

use {
  bitcoin::{OutPoint, PackedLockTime, Script, Sequence, Transaction, TxIn, Witness},
  dogemap_support::{envelope, envelope_head, envelope_tail, Chain},
  ord::dogemap_feed::parser::{self, ParseOutcome},
  serde_json::Value,
  std::process::Stdio,
  test_bitcoincore_rpc::TransactionTemplate,
};

/// Dogecoin Core's minimal push encoding.
fn push(data: &[u8]) -> Vec<u8> {
  let len = data.len();
  let mut out = match len {
    0..=75 => vec![len as u8],
    76..=255 => vec![0x4c, len as u8],
    256..=65535 => vec![0x4d, (len & 0xff) as u8, (len >> 8) as u8],
    _ => {
      let mut prefix = vec![0x4e];
      prefix.extend_from_slice(&(len as u32).to_le_bytes());
      prefix
    }
  };
  out.extend_from_slice(data);
  out
}

/// `ord`, one piece, the content type, countdown 0, then `body_push` bytes
/// exactly as given.
fn envelope_with_body_push(content_type: &[u8], body_push: &[u8]) -> Script {
  let mut bytes = push(b"ord");
  bytes.push(0x51);
  bytes.extend(push(content_type));
  bytes.push(0x00);
  bytes.extend_from_slice(body_push);
  Script::from(bytes)
}

fn complete(content_type: &[u8], body: &[u8]) -> ParseOutcome {
  ParseOutcome::Complete {
    content_type: Some(content_type.to_vec()),
    body: Some(body.to_vec()),
    delegate: None,
  }
}

fn both(script: &Script) -> (ParseOutcome, ParseOutcome) {
  let scripts = std::slice::from_ref(script);
  (
    parser::compat_script_sigs(scripts),
    parser::strict_script_sigs(scripts),
  )
}

fn transaction(script_sigs: &[Script]) -> Transaction {
  Transaction {
    version: 1,
    lock_time: PackedLockTime(0),
    input: script_sigs
      .iter()
      .map(|script_sig| TxIn {
        previous_output: OutPoint::null(),
        script_sig: script_sig.clone(),
        sequence: Sequence::MAX,
        witness: Witness::new(),
      })
      .collect(),
    output: Vec::new(),
  }
}

#[test]
fn direct_pushes_decode_the_same_under_both_decoders() {
  for len in [1usize, 15, 64, 75] {
    let body = vec![b'd'; len];
    let script = envelope_with_body_push(b"text/plain", &push(&body));
    assert_eq!(script.as_bytes()[script.len() - len - 1], len as u8);
    assert_eq!(
      both(&script),
      (
        complete(b"text/plain", &body),
        complete(b"text/plain", &body)
      )
    );
    assert_eq!(
      parser::compat_pushes(&script),
      parser::strict_pushes(&script)
    );
  }
}

#[test]
fn pushdata1_boundaries_decode_the_same_under_both_decoders() {
  for len in [76usize, 200, 255] {
    let body = vec![0x42; len];
    let body_push = push(&body);
    assert_eq!(&body_push[..2], &[0x4c, len as u8]);
    let script = envelope_with_body_push(b"image/png", &body_push);
    assert_eq!(
      both(&script),
      (complete(b"image/png", &body), complete(b"image/png", &body))
    );
    assert_eq!(
      parser::compat_pushes(&script),
      parser::strict_pushes(&script)
    );
  }
}

#[test]
fn pushdata2_length_quirk_is_pinned() {
  // 4d 00 01: Core reads LE16 0x0100 = 256. Compat reads
  // (0x00 << 8) + 0x4d = 77, takes 77 bytes, then meets 0xaa, which is not a
  // push opcode, and rejects the whole script.
  let mut script = vec![0x4d, 0x00, 0x01];
  script.extend(vec![0xaa; 256]);
  let script = Script::from(script);
  assert_eq!(parser::compat_pushes(&script), None);
  assert_eq!(parser::strict_pushes(&script), Some(vec![vec![0xaa; 256]]));

  // The 77-byte misread made visible: the bytes after it are OP_1s, which
  // compat accepts as 179 pushes of [1].
  let mut bytes = vec![0x4d, 0x00, 0x01];
  bytes.extend(vec![0xaa; 77]);
  bytes.extend(vec![0x51; 179]);
  let script = Script::from(bytes.clone());
  let mut compat = vec![vec![0xaa; 77]];
  compat.extend(vec![vec![1u8]; 179]);
  assert_eq!(parser::compat_pushes(&script), Some(compat));
  assert_eq!(
    parser::strict_pushes(&script),
    Some(vec![bytes[3..].to_vec()])
  );

  // In an envelope: not an inscription under the compat profile, a complete
  // 256-byte body under Core decoding.
  let body = vec![0x61; 256];
  let script = envelope_with_body_push(b"text/plain", &push(&body));
  assert_eq!(
    &script.as_bytes()[script.len() - 259..script.len() - 256],
    &[0x4d, 0x00, 0x01]
  );
  assert_eq!(
    both(&script),
    (ParseOutcome::None, complete(b"text/plain", &body))
  );

  // A non-minimal PUSHDATA2 of a candidate body: 4d 0f 00 reads as 15 under
  // Core and as (0x0f << 8) + 0x4d = 3917 under compat.
  let mut body_push = vec![0x4d, 0x0f, 0x00];
  body_push.extend_from_slice(b"4771259.dogemap");
  let script = envelope_with_body_push(b"text/plain", &body_push);
  assert_eq!(
    both(&script),
    (
      ParseOutcome::None,
      complete(b"text/plain", b"4771259.dogemap")
    )
  );
}

#[test]
fn pushdata4_length_quirk_is_pinned() {
  // 4e 00 01 00 00: Core reads LE32 256. Compat reads
  // (0x00 << 24) + (0x01 << 16) + (0x00 << 8) + 0x4e = 65614 and runs out.
  let mut script = vec![0x4e, 0x00, 0x01, 0x00, 0x00];
  script.extend(vec![0xaa; 256]);
  let script = Script::from(script);
  assert_eq!(parser::compat_pushes(&script), None);
  assert_eq!(parser::strict_pushes(&script), Some(vec![vec![0xaa; 256]]));

  // 4e 00 00 00 00 declares 0 bytes under Core; compat reads 0x4e = 78.
  let mut bytes = vec![0x4e, 0x00, 0x00, 0x00, 0x00];
  bytes.extend(vec![0x01; 78]);
  let script = Script::from(bytes);
  assert_eq!(parser::compat_pushes(&script), Some(vec![vec![0x01; 78]]));
  let mut strict = vec![Vec::new()];
  strict.extend(vec![vec![0x01u8]; 39]);
  assert_eq!(parser::strict_pushes(&script), Some(strict));

  let body = vec![0x62; 256];
  let mut body_push = vec![0x4e];
  body_push.extend_from_slice(&256u32.to_le_bytes());
  body_push.extend_from_slice(&body);
  let script = envelope_with_body_push(b"text/plain", &body_push);
  assert_eq!(
    both(&script),
    (ParseOutcome::None, complete(b"text/plain", &body))
  );
}

#[test]
fn truncated_lengths_and_payloads_fail_without_panicking() {
  for bytes in [
    vec![0x4c],
    vec![0x4d],
    vec![0x4d, 0x00],
    vec![0x4e, 0x00, 0x00, 0x00],
    vec![0x05, 0xaa, 0xaa],
    vec![0x4c, 0x05, 0xaa],
    vec![0x4d, 0x05, 0x00, 0xaa],
    vec![0x4e, 0x05, 0x00, 0x00, 0x00, 0xaa],
    // A 4 GiB declared length must fail on bounds, not allocate.
    vec![0x4e, 0xff, 0xff, 0xff, 0xff, 0xaa],
  ] {
    let script = Script::from(bytes.clone());
    assert_eq!(parser::compat_pushes(&script), None, "{bytes:02x?}");
    assert_eq!(parser::strict_pushes(&script), None, "{bytes:02x?}");
    assert_eq!(both(&script), (ParseOutcome::None, ParseOutcome::None));

    // The same truncation after a valid envelope header.
    let mut tail = push(b"ord");
    tail.extend([0x51]);
    tail.extend(push(b"text/plain"));
    tail.extend([0x00]);
    tail.extend(&bytes);
    let script = Script::from(tail);
    assert_eq!(both(&script), (ParseOutcome::None, ParseOutcome::None));
  }
  // No scriptSigs at all is not an inscription (it used to index out of
  // bounds).
  assert_eq!(parser::compat_script_sigs(&[]), ParseOutcome::None);
  assert_eq!(parser::compat_transactions(&[]), ParseOutcome::None);
}

#[test]
fn a_trailing_non_push_opcode_rejects_the_script() {
  let mut bytes = envelope(b"text/plain", &[b"4771259.dogemap"]).to_bytes();
  let valid = Script::from(bytes.clone());
  assert_eq!(
    both(&valid),
    (
      complete(b"text/plain", b"4771259.dogemap"),
      complete(b"text/plain", b"4771259.dogemap")
    )
  );
  bytes.push(0xac); // OP_CHECKSIG
  assert_eq!(
    both(&Script::from(bytes)),
    (ParseOutcome::None, ParseOutcome::None)
  );
  for opcode in [0x4f, 0x50, 0x61, 0x76] {
    // OP_1NEGATE, OP_RESERVED, OP_NOP, OP_DUP
    let mut bytes = vec![opcode];
    bytes.extend(envelope(b"text/plain", &[b"1.dogemap"]).to_bytes());
    assert_eq!(
      both(&Script::from(bytes)),
      (ParseOutcome::None, ParseOutcome::None)
    );
  }
}

#[test]
fn multipart_counts_down_two_one_zero_across_two_transactions() {
  let head = envelope_head(b"text/plain", 3, &[b"4771", b"259."]);
  let tail = envelope_tail(0, &[b"dogemap"]);
  let first = transaction(&[head]);
  let second = transaction(std::slice::from_ref(&tail));

  assert_eq!(
    parser::compat_transactions(&[first.clone(), second.clone()]),
    complete(b"text/plain", b"4771259.dogemap")
  );
  assert_eq!(
    parser::compat_transactions(std::slice::from_ref(&first)),
    ParseOutcome::Partial
  );
  assert_eq!(
    parser::compat_transactions(std::slice::from_ref(&second)),
    ParseOutcome::None
  );
  assert_eq!(
    parser::compat_transactions(&[second, first.clone()]),
    ParseOutcome::None
  );

  // The first part alone contributes the first 8 body bytes; the feed skips
  // assembling a multipart body only when this prefix already exceeds 64.
  assert_eq!(parser::first_part(&first), (ParseOutcome::Partial, 8));
  let single = transaction(&[envelope(b"text/plain", &[b"1.dogemap"])]);
  assert_eq!(
    parser::first_part(&single),
    (complete(b"text/plain", b"1.dogemap"), 9)
  );
  let big_head = envelope_head(b"text/plain", 2, &[&[b'x'; 65]]);
  assert_eq!(
    parser::first_part(&transaction(&[big_head])),
    (ParseOutcome::Partial, 65)
  );

  // A wrong countdown in the continuation ends the chain.
  let wrong = transaction(&[envelope_tail(1, &[b"dogemap"])]);
  assert_eq!(
    parser::compat_transactions(&[first, wrong]),
    ParseOutcome::None
  );
}

#[test]
fn only_the_first_input_is_read() {
  let body = envelope(b"text/plain", &[b"1.dogemap"]);
  assert_eq!(
    parser::compat_transactions(&[transaction(&[Script::new(), body.clone()])]),
    ParseOutcome::None
  );
  assert_eq!(
    parser::compat_transactions(&[transaction(&[
      body,
      envelope(b"text/plain", &[b"2.dogemap"])
    ])]),
    complete(b"text/plain", b"1.dogemap")
  );
  assert_eq!(
    parser::compat_transactions(&[transaction(&[])]),
    ParseOutcome::None
  );
}

/// The compat profile keys a partial reveal by its txid alone, so spending
/// any output of it, here the sibling output 1, continues the chain; the
/// inscription keeps the first reveal's id and lands on the continuation's
/// output. This is the historical behavior and is kept.
#[test]
fn a_sibling_output_continues_a_partial_reveal() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(2);
  let first = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(1, 0, 0)],
    outputs: 2,
    script_sig: envelope_head(b"text/plain", 2, &[b"4771259."]),
    ..Default::default()
  });
  rpc.mine_blocks(1);
  let second = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(3, 1, 1)],
    script_sig: envelope_tail(0, &[b"dogemap"]),
    ..Default::default()
  });
  let block = rpc.mine_blocks(1).remove(0);

  let server = chain.serve();
  server.wait_for_checkpoint(4);
  let (page, events) = server.whole_block(4, &block.header.block_hash().to_string(), 10);
  assert_eq!(page["creationCount"], "1");
  assert_eq!(events[0]["inscriptionId"], format!("{first}i0"));
  assert_eq!(events[0]["partCount"], "2");
  assert_eq!(events[0]["origin"]["height"], "3");
  assert_eq!(events[0]["completion"]["txid"], second.to_string());
  assert_eq!(events[0]["location"]["outpoint"], format!("{second}:0"));
  assert_eq!(
    events[0]["rawBody"]["bytes"],
    base64::encode(b"4771259.dogemap")
  );
}

/// A reveal whose body uses a (non-minimal) PUSHDATA2 is not an inscription
/// under the compat profile, end to end, and the audit subcommand reports it
/// as a candidate Core decoding would have accepted.
#[test]
fn the_pushdata2_quirk_holds_in_the_indexer_and_the_audit_reports_it() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(1);
  let mut body_push = vec![0x4d, 0x0f, 0x00];
  body_push.extend_from_slice(b"4771259.dogemap");
  let quirk = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(1, 0, 0)],
    script_sig: envelope_with_body_push(b"text/plain", &body_push),
    ..Default::default()
  });
  let block = rpc.mine_blocks(1).remove(0);

  let server = chain.serve();
  server.wait_for_checkpoint(2);
  let (page, events) = server.whole_block(2, &block.header.block_hash().to_string(), 10);
  assert_eq!(page["creationCount"], "0");
  assert!(events.is_empty());
  drop(server);

  let output = chain
    .ord()
    .args([
      "dogemap-pushdata-audit",
      "--from-height",
      "0",
      "--to-height",
      "2",
      "--json",
    ])
    .stdout(Stdio::piped())
    .output()
    .unwrap();
  assert!(output.status.success());
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(report["schemaVersion"], "dogemap-pushdata-audit-v1");
  assert_eq!(report["network"], "regtest");
  assert_eq!(report["parserProfile"], "doginals-trac-1.0.2-compat-v1");
  assert_eq!(report["blocksScanned"], 3);
  assert_eq!(report["decodeDifferences"], 1);
  assert_eq!(report["parseDifferences"], 1);
  let finding = &report["findings"][0];
  assert_eq!(finding["height"], 2);
  assert_eq!(finding["txIndex"], 1);
  assert_eq!(finding["txid"], quirk.to_string());
  assert_eq!(finding["kind"], "parse_outcome_differs");
  assert_eq!(finding["compatOutcome"], "none");
  assert_eq!(finding["strictOutcome"], "complete");
  assert_eq!(finding["compatDecodes"], false);
  assert_eq!(finding["strictDecodes"], true);
  assert_eq!(finding["strictBodyByteLength"], 15);
  assert_eq!(finding["strictBodyPassesPrefilter"], true);
  assert_eq!(finding["compatBodyPassesPrefilter"], false);

  let output = chain
    .ord()
    .args([
      "dogemap-pushdata-audit",
      "--from-height",
      "3",
      "--to-height",
      "2",
    ])
    .output()
    .unwrap();
  assert!(!output.status.success());
}

#[test]
fn signet_is_rejected_at_startup() {
  let output = std::process::Command::new(executable_path::executable_path("ord"))
    .args(["--signet", "epochs"])
    .stdout(Stdio::null())
    .stderr(Stdio::piped())
    .output()
    .unwrap();
  assert!(!output.status.success());
  assert!(String::from_utf8_lossy(&output.stderr).contains("signet does not exist for Dogecoin"));
}
