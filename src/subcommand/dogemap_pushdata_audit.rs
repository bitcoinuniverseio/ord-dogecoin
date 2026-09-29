//! `ord dogemap-pushdata-audit`: measure what the compat parser's PUSHDATA2 /
//! PUSHDATA4 length quirk (findings P-F03) changes on a real chain.
//!
//! Every input-0 scriptSig in the height range is decoded twice, with the
//! compat decoder that indexes and with Dogecoin Core's push decoding, and
//! every script whose decodings differ is reported with both single-script
//! parse outcomes. Blocks are read from the node over RPC; the index is not
//! opened or modified. The report is evidence for a future parser profile
//! decision, never an input to indexing.

use {
  super::*,
  crate::{dogemap_feed as wire, inscription::ParsedInscription},
  bitcoincore_rpc::Auth,
};

#[derive(Debug, Parser)]
pub(crate) struct DogemapPushdataAudit {
  #[arg(long, help = "Scan blocks from <FROM_HEIGHT>.")]
  from_height: u32,
  #[arg(long, help = "Scan blocks up to and including <TO_HEIGHT>.")]
  to_height: u32,
  #[arg(long, help = "Print one JSON report instead of text.")]
  json: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Finding {
  height: u32,
  tx_index: u32,
  txid: String,
  /// `parse_outcome_differs`: none/partial/complete differ; `body_differs`:
  /// both complete with different body or content type;
  /// `pushdata_decode_only`: the pushes differ but a single-script parse
  /// agrees (a continuation script or a non-inscription spend), which can
  /// still change a multipart chain that includes this script.
  kind: &'static str,
  compat_outcome: &'static str,
  strict_outcome: &'static str,
  compat_decodes: bool,
  strict_decodes: bool,
  strict_body_byte_length: Option<usize>,
  strict_body_passes_prefilter: bool,
  compat_body_passes_prefilter: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
  schema_version: &'static str,
  network: String,
  parser_profile: &'static str,
  from_height: u32,
  to_height: u32,
  blocks_scanned: u64,
  transactions_scanned: u64,
  decode_differences: u64,
  parse_differences: u64,
  findings: Vec<Finding>,
}

fn outcome(parsed: &ParsedInscription) -> &'static str {
  match parsed {
    ParsedInscription::None => "none",
    ParsedInscription::Partial => "partial",
    ParsedInscription::Complete(_) => "complete",
  }
}

fn passes_prefilter(parsed: &ParsedInscription) -> bool {
  match parsed {
    ParsedInscription::Complete(inscription) => {
      wire::candidate_prefilter(inscription.body().unwrap_or_default())
    }
    _ => false,
  }
}

impl DogemapPushdataAudit {
  pub(crate) fn run(self, options: Options) -> SubcommandResult {
    ensure!(
      self.from_height <= self.to_height,
      "--from-height must not exceed --to-height"
    );

    // Same credential rule as the index: the cookie file when present,
    // otherwise user and password from the RPC URL.
    let rpc_url = options.rpc_url();
    let cookie_file = options.cookie_file()?;
    let auth = if cookie_file.exists() {
      Auth::CookieFile(cookie_file)
    } else {
      let url = url::Url::parse(&rpc_url)?;
      Auth::UserPass(
        url.username().to_string(),
        url.password().map(str::to_owned).unwrap_or_default(),
      )
    };
    let client = Client::new(&rpc_url, auth).context("failed to connect to RPC URL")?;

    let network = options.chain().to_string();
    let expected_chain = match options.chain() {
      Chain::Mainnet => "main",
      Chain::Testnet => "test",
      Chain::Regtest => "regtest",
      Chain::Signet => "signet",
    };
    let chain = client.get_blockchain_info()?.chain;
    ensure!(
      chain == expected_chain,
      "Dogecoin Core reports chain {chain}, but ord runs on {network}"
    );

    let mut report = Report {
      schema_version: "dogemap-pushdata-audit-v1",
      network,
      parser_profile: wire::PARSER_PROFILE,
      from_height: self.from_height,
      to_height: self.to_height,
      blocks_scanned: 0,
      transactions_scanned: 0,
      decode_differences: 0,
      parse_differences: 0,
      findings: Vec::new(),
    };

    for height in self.from_height..=self.to_height {
      if SHUTTING_DOWN.load(atomic::Ordering::Relaxed) {
        bail!("interrupted at height {height}");
      }
      let hash = client.get_block_hash(height.into())?;
      let block = client.get_block(&hash)?;
      report.blocks_scanned += 1;

      for (tx_index, tx) in block.txdata.iter().enumerate() {
        report.transactions_scanned += 1;
        let Some(input) = tx.input.first() else {
          continue;
        };
        let script = &input.script_sig;
        let compat_pushes = Inscription::decode_pushes_compat(script);
        let strict_pushes = Inscription::decode_pushes_strict(script);
        if compat_pushes == strict_pushes {
          continue;
        }
        report.decode_differences += 1;

        let scripts = std::slice::from_ref(script);
        let compat = Inscription::parse_script_sigs_compat(scripts);
        let strict = Inscription::parse_script_sigs_strict(scripts);
        let kind = match (&compat, &strict) {
          (ParsedInscription::Complete(a), ParsedInscription::Complete(b)) if a != b => {
            "body_differs"
          }
          (a, b) if outcome(a) != outcome(b) => "parse_outcome_differs",
          _ => "pushdata_decode_only",
        };
        if kind != "pushdata_decode_only" {
          report.parse_differences += 1;
        }

        let finding = Finding {
          height,
          tx_index: u32::try_from(tx_index)?,
          txid: tx.txid().to_string(),
          kind,
          compat_outcome: outcome(&compat),
          strict_outcome: outcome(&strict),
          compat_decodes: compat_pushes.is_some(),
          strict_decodes: strict_pushes.is_some(),
          strict_body_byte_length: match &strict {
            ParsedInscription::Complete(inscription) => inscription.content_length(),
            _ => None,
          },
          strict_body_passes_prefilter: passes_prefilter(&strict),
          compat_body_passes_prefilter: passes_prefilter(&compat),
        };

        if !self.json {
          println!(
            "{} {} {} {} compat={} strict={} strictPrefilter={}",
            finding.height,
            finding.tx_index,
            finding.txid,
            finding.kind,
            finding.compat_outcome,
            finding.strict_outcome,
            finding.strict_body_passes_prefilter,
          );
        }
        report.findings.push(finding);
      }
    }

    if self.json {
      println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
      println!(
        "scanned {} blocks ({}..={}) and {} transactions on {}: {} scriptSigs decode differently, {} of them parse differently",
        report.blocks_scanned,
        report.from_height,
        report.to_height,
        report.transactions_scanned,
        report.network,
        report.decode_differences,
        report.parse_differences,
      );
    }

    Ok(Box::new(Empty {}))
  }
}
