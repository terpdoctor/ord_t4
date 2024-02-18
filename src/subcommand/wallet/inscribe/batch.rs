use super::*;

pub(super) struct Batch {
  pub(super) commit_fee_rate: FeeRate,
  pub(super) commit_only: bool,
  pub(super) commit_vsize: Option<u64>,
  pub(super) commitment: Option<OutPoint>,
  pub(super) commitment_output: Option<GetRawTransactionResultVout>,
  pub(super) destinations: Vec<Address>,
  pub(super) dump: bool,
  pub(super) dry_run: bool,
  pub(super) fee_utxos: Vec<OutPoint>,
  pub(super) inscribe_on_specific_utxos: bool,
  pub(super) inscriptions: Vec<Inscription>,
  pub(super) key: Option<String>,
  pub(super) mode: Mode,
  pub(super) next_inscriptions: Vec<Inscription>,
  pub(super) no_backup: bool,
  pub(super) no_broadcast: bool,
  pub(super) no_limit: bool,
  pub(super) no_wallet: bool,
  pub(super) parent_info: Option<ParentInfo>,
  pub(super) postage: Amount,
  pub(super) reinscribe: bool,
  pub(super) reveal_fee: Option<Amount>,
  pub(super) reveal_fee_rate: FeeRate,
  pub(super) reveal_input: Vec<OutPoint>,
  pub(super) reveal_psbt: Option<Psbt>,
  pub(super) satpoint: Option<SatPoint>,
}

impl Default for Batch {
  fn default() -> Batch {
    Batch {
      commit_fee_rate: 1.0.try_into().unwrap(),
      commit_only: false,
      commit_vsize: None,
      commitment: None,
      commitment_output: None,
      destinations: Vec::new(),
      dump: false,
      dry_run: false,
      fee_utxos: Vec::new(),
      inscribe_on_specific_utxos: false,
      inscriptions: Vec::new(),
      key: None,
      mode: Mode::SharedOutput,
      next_inscriptions: Vec::new(),
      no_backup: false,
      no_broadcast: false,
      no_limit: false,
      no_wallet: false,
      parent_info: None,
      postage: Amount::from_sat(10_000),
      reinscribe: false,
      reveal_fee: None,
      reveal_fee_rate: 1.0.try_into().unwrap(),
      reveal_input: Vec::new(),
      reveal_psbt: None,
      satpoint: None,
    }
  }
}

impl Batch {
  pub(crate) fn inscribe(
    &self,
    chain: Chain,
    index: &Index,
    client: &Client,
    locked_utxos: &BTreeSet<OutPoint>,
    runic_utxos: BTreeSet<OutPoint>,
    utxos: &mut BTreeMap<OutPoint, Amount>,
    force_input: Vec<OutPoint>,
    change: Option<Address>,
  ) -> Result<Output> {
    let use_psbt_for_commit = true; // when not signing the commit, should we use psbt or hex for the unsigned commit tx?

    let wallet_inscriptions = index.get_inscriptions(utxos)?;

    if !self.fee_utxos.is_empty() {
      if self.reveal_fee_rate != FeeRate::try_from(0.0)? {
        return Err(anyhow!("use `--fee-rate 0` when using specific utxos to pay fees; the rate will be calculated from the size of the fee utxo(s)"));
      }
      if self.commit_fee_rate != FeeRate::try_from(0.0)? {
        return Err(anyhow!("don't use `--commit-fee-rate` when using specific utxos to pay fees; the rate will be calculated from the size of the fee utxo(s)"));
      }
      if !force_input.is_empty() {
        return Err(anyhow!("don't use `--commit-input` when using specific utxos to pay fees"));
      }

      for outpoint in &self.fee_utxos {
        if !utxos.contains_key(&outpoint) {
          utxos.insert(*outpoint, Amount::from_sat(client.get_raw_transaction(&outpoint.txid, None)?.output[outpoint.vout as usize].value));
        }
      }
    }

    let force_input = if self.fee_utxos.is_empty() {
      force_input
    } else {
      self.fee_utxos.clone()
    };

    let commit_tx_change = if self.no_wallet {
      None
    } else {
      Some([
      get_change_address(client, chain)?,
      match change {
        Some(change) => change,
        None => get_change_address(client, chain)?,
      },
    ])};

    let (commit_tx, reveal_tx, recovery_key_pair, total_fees, dummy_commit_psbt) = self
      .create_batch_inscription_transactions(
        wallet_inscriptions,
        index,
        chain,
        locked_utxos.clone(),
        runic_utxos,
        utxos.clone(),
        commit_tx_change,
        force_input,
        client,
      )?;

    if dummy_commit_psbt.is_some() {
      let dummy_commit_psbt = dummy_commit_psbt.unwrap();
      return Ok(self.output(None, None, None,
                            Some(dummy_commit_psbt),
                            Some("sign commit_psbt then re-run the /inscribe endpoint with `commit_vsize` in the input JSON set to the vsize of the signed tx; the tx has 0 fees so you can't accidentally broadcast it".to_string()),
                            None, None, None, 0, Vec::new(), &BTreeMap::new()));
    }

    let commit_tx = commit_tx.unwrap();
    let mut reveal_tx = reveal_tx.unwrap();
    let recovery_key_pair = recovery_key_pair.unwrap();
    let total_fees = total_fees.unwrap();

    if self.dry_run {
      return Ok(self.output(
        if self.commitment.is_some() {
          None
        } else {
          Some(commit_tx.txid())
        },
        if self.commit_only {
          None
        } else {
          Some(reveal_tx.txid())
        },
        None,
        None,
        None,
        None,
        None,
        None,
        total_fees,
        self.inscriptions.clone(),
        utxos,
      ));
    }

    let signed_commit_tx = if self.commitment.is_some() || self.no_wallet {
      Vec::new()
    } else {
      client
      .sign_raw_transaction_with_wallet(&commit_tx, None, None)?
      .hex
    };

    let mut reveal_input_info = Vec::new();

    if self.parent_info.is_some() {
      for (vout, output) in commit_tx.output.iter().enumerate() {
        reveal_input_info.push(SignRawTransactionInput {
          txid: commit_tx.txid(),
          vout: vout.try_into().unwrap(),
          script_pub_key: output.script_pubkey.clone(),
          redeem_script: None,
          amount: Some(Amount::from_sat(output.value)),
        });
      }
    }

    for input in &self.reveal_input {
      let output = index.get_transaction(input.txid)?.unwrap().output[input.vout as usize].clone();
      reveal_input_info.push(SignRawTransactionInput {
        txid: input.txid,
        vout: input.vout,
        script_pub_key: output.script_pubkey.clone(),
        redeem_script: None,
        amount: Some(Amount::from_sat(output.value)),
      });
    }

    let signed_reveal_tx = if (reveal_input_info.is_empty() && self.parent_info.is_none()) || self.no_wallet {
      consensus::encode::serialize(&reveal_tx)
    } else {
      client
        .sign_raw_transaction_with_wallet(
          &reveal_tx,
          Some(&reveal_input_info),
          None,
        )?
        .hex
    };

    if self.no_wallet {
      let commit_tx_hex = if use_psbt_for_commit {
        general_purpose::STANDARD.encode(Psbt::from_unsigned_tx(commit_tx.clone())?.serialize())
      } else {
        commit_tx.raw_hex()
      };

      let blank_reveal_psbt = if let Some(reveal_psbt) = self.reveal_psbt.clone() {
        // eprintln!("\nwe have been given a reveal psbt:\n{:#?}\ncopy its signature(s) to our reveal_tx", reveal_psbt);
        let extracted_tx = reveal_psbt.extract_tx();
        // eprintln!("\nextracted tx {:?}", extracted_tx);
/*
        for (i, input) in extracted_tx.input.iter().enumerate() {
          eprintln!("\ninput {i}: {:?}", input);
          eprintln!("  prevout outpoint: {:?}", input.previous_output);
          eprintln!("  witness: {:?}", input.witness);
        }

        eprintln!("\n---");

        eprintln!("\nour reveal tx {:?}", reveal_tx);

        for (i, input) in reveal_tx.input.iter().enumerate() {
          eprintln!("\ninput {i}: {:?}", input);
          eprintln!("  prevout outpoint: {:?}", input.previous_output);
          eprintln!("  witness: {:?}", input.witness);
        }

        eprintln!("\n---");
*/
        if extracted_tx.input.len() != reveal_tx.input.len() {
          return Err(anyhow!("supplied reveal_psbt has {} inputs but should have {}", extracted_tx.input.len(), reveal_tx.input.len()));
        }

        for (i, input) in extracted_tx.input.iter().enumerate() {
          if input.previous_output != reveal_tx.input[i].previous_output {
            return Err(anyhow!("prevout of input {i} of reveal_psbt is incorrect"));
          }

          if reveal_tx.input[i].witness.len() == 0 {
            if input.witness.len() > 0 {
              reveal_tx.input[i] = input.clone();
            } else {
              return Err(anyhow!("input {i} of reveal_psbt isn't signed"));
            }
          }
        }
/*
        eprintln!("\n---");
        eprintln!("\nmerged txs:");

        for (i, input) in reveal_tx.input.iter().enumerate() {
          eprintln!("\ninput {i}: {:?}", input);
          eprintln!("  prevout outpoint: {:?}", input.previous_output);
          eprintln!("  witness: {:?}", input.witness);
        }
*/
        None
      } else {
        // copy the reveal_tx, and blank out all the witnesses so we can convert it to a Psbt
        let mut blank_reveal_tx = reveal_tx.clone();
        let mut any_unsigned = false;
        for input in &mut blank_reveal_tx.input {
          if input.witness.len() == 0 {
            any_unsigned = true;
          }
          input.witness = Witness::new();
        }
        
        if any_unsigned {
          let commit_txid = commit_tx.txid();
          let mut blank_reveal_psbt = Psbt::from_unsigned_tx(blank_reveal_tx.clone())?;
          let mut found_commit_output = false;
          for (i, input) in blank_reveal_tx.input.iter().enumerate() {
            if commit_txid == input.previous_output.txid {
              if found_commit_output {
                return Err(anyhow!("reveal has multiple inputs from the commit tx"));
              }
              found_commit_output = true;
              blank_reveal_psbt.inputs[i].witness_utxo = Some(commit_tx.output[input.previous_output.vout as usize].clone())
            }
          }
          if !found_commit_output {
            return Err(anyhow!("reveal has no inputs from the commit tx"));
          }
          Some(general_purpose::STANDARD.encode(blank_reveal_psbt.serialize()))
        } else {
          None
        }
      };

      return Ok(self.output(None, None, None,
                            Some(commit_tx_hex),
                            Some(if self.parent_info.is_none() {
                              "sign commit_psbt, then broadcast the signed result and reveal_hex"
                            } else {
                              "sign commit_psbt and reveal_hex, then broadcast them both. or sign the reveal_psbt, add it to the input json, and run the /inscribe endpoint again"
                            }.to_string()),
                            Some(consensus::encode::serialize(&reveal_tx).raw_hex()),
                            blank_reveal_psbt,
                            None, 0, Vec::new(), &BTreeMap::new()));
    }

    if !self.no_backup && self.key.is_none() {
      Self::backup_recovery_key(client, recovery_key_pair, chain.network())?;
    }

    let (commit, reveal) = if self.no_broadcast {
      (if self.commitment.is_some() { None }
      	  else { Some(client.decode_raw_transaction(&signed_commit_tx, None)?.txid) },
       if self.commit_only { None }
       	  else { Some(client.decode_raw_transaction(&signed_reveal_tx, None)?.txid) })
    } else {
    let commit = if self.commitment.is_some() {
      None
    } else {
      Some(client.send_raw_transaction(&signed_commit_tx)?)
    };

    let reveal = if self.commit_only {
      None
    } else {
    match client.send_raw_transaction(&signed_reveal_tx) {
    Ok(txid) => Some(txid),
    Err(err) => {
      return Err(anyhow!(
        format!("Failed to send reveal transaction: {err}{}", if commit.is_some() { format!("\nCommit tx {:?} will be recovered once mined", commit) } else { "".to_string() })
      ))
    }
    }
    };

    (commit, reveal)
    };

    Ok(self.output(
      commit,
      reveal,
      if self.dump && self.commitment.is_none() { Some(signed_commit_tx.raw_hex()) } else { None },
      None, None,
      if self.dump && !self.commit_only { Some(signed_reveal_tx.raw_hex()) } else { None },
      None,
      if self.dump { Some(Self::get_recovery_key(&client, recovery_key_pair, chain.network())?.to_string()) } else { None },
      total_fees,
      self.inscriptions.clone(),
      utxos,
    ))
  }

  fn output(
    &self,
    commit: Option<Txid>,
    reveal: Option<Txid>,
    commit_hex: Option<String>,
    commit_psbt: Option<String>,
    message: Option<String>,
    reveal_hex: Option<String>,
    reveal_psbt: Option<String>,
    recovery_descriptor: Option<String>,
    total_fees: u64,
    inscriptions: Vec<Inscription>,
    utxos: &BTreeMap<OutPoint, Amount>,
  ) -> super::Output {
    if commit_psbt.is_some() {
      return super::Output {
        commit: None,
        commit_hex: None,
        commit_psbt,
        inscriptions: Vec::new(),
        message,
        parent: None,
        recovery_descriptor: None,
        reveal: None,
        reveal_hex,
        reveal_psbt,
        total_fees: 0,
      };
    }

    let mut inscriptions_output = Vec::new();
    let mut offset = 0;
    for index in 0..inscriptions.len() {
      let index = u32::try_from(index).unwrap();

      let vout = match self.mode {
        Mode::SharedOutput | Mode::SameSat => {
          if self.parent_info.is_some() {
            1
          } else {
            0
          }
        }
        Mode::SeparateOutputs => {
          if self.parent_info.is_some() {
            index + 1
          } else {
            index
          }
        }
      };

      if !self.commit_only {
      inscriptions_output.push(InscriptionInfo {
        id: InscriptionId {
          txid: reveal.unwrap(),
          index,
        },
        location: SatPoint {
          outpoint: OutPoint { txid: reveal.unwrap(), vout },
          offset,
        },
      });
      }

      if self.mode == Mode::SharedOutput {
        offset += if self.inscribe_on_specific_utxos {
          utxos[&self.inscriptions[index as usize].utxo.unwrap()]
        } else {
          self.postage
        }.to_sat()
      }
    }

    super::Output {
      commit,
      commit_hex,
      commit_psbt: None,
      message: None,
      reveal,
      reveal_hex,
      reveal_psbt: None,
      recovery_descriptor,
      total_fees,
      parent: self.parent_info.clone().map(|info| info.id),
      inscriptions: inscriptions_output,
    }
  }

  pub(crate) fn create_batch_inscription_transactions(
    &self,
    wallet_inscriptions: BTreeMap<SatPoint, InscriptionId>,
    index: &Index,
    chain: Chain,
    locked_utxos: BTreeSet<OutPoint>,
    runic_utxos: BTreeSet<OutPoint>,
    mut utxos: BTreeMap<OutPoint, Amount>,
    change: Option<[Address; 2]>,
    force_input: Vec<OutPoint>,
    client: &Client,
  ) -> Result<(Option<Transaction>, Option<Transaction>, Option<TweakedKeyPair>, Option<u64>, Option<String>)> {
    if let Some(parent_info) = &self.parent_info {
      assert!(self
        .inscriptions
        .iter()
        .all(|inscription| inscription.parent().unwrap() == parent_info.id))
    }

    if !self.fee_utxos.is_empty() && !self.inscribe_on_specific_utxos {
      return Err(anyhow!("listing utxos to use as fees only works when inscribing on specified utxos"));
    }

    if !self.next_inscriptions.is_empty() && self.commitment.is_none() {
      return Err(anyhow!("--next-batch and --next-file don't work without --commitment"));
    }

    if !self.fee_utxos.is_empty() && self.reveal_fee.is_some() {
      return Err(anyhow!("--reveal-fee doesn't work when specifying fee_utxos"));
    }

    match self.mode {
      Mode::SameSat => assert_eq!(
        self.destinations.len(),
        1,
        "invariant: same-sat has only one destination"
      ),
      Mode::SeparateOutputs => assert_eq!(
        self.destinations.len(),
        self.inscriptions.len(),
        "invariant: destination addresses and number of inscriptions doesn't match"
      ),
      Mode::SharedOutput => assert_eq!(
        self.destinations.len(),
        1,
        "invariant: destination addresses and number of inscriptions doesn't match"
      ),
    }

    let satpoints = if self.inscribe_on_specific_utxos {
      self.inscriptions.iter().map(|inscription| SatPoint {outpoint: inscription.utxo.unwrap(), offset: 0}).collect::<Vec<SatPoint>>()
    } else {
    let satpoint = if self.commitment.is_some() {
      SatPoint::from_str("0000000000000000000000000000000000000000000000000000000000000000:0:0")?
    } else if let Some(satpoint) = self.satpoint {
      satpoint
    } else {
      let inscribed_utxos = wallet_inscriptions
        .keys()
        .map(|satpoint| satpoint.outpoint)
        .collect::<BTreeSet<OutPoint>>();

      utxos
        .iter()
        .find(|(outpoint, amount)| {
          amount.to_sat() > 0
            && !inscribed_utxos.contains(outpoint)
            && !locked_utxos.contains(outpoint)
            && !runic_utxos.contains(outpoint)
            && !self.fee_utxos.contains(outpoint)
        })
        .map(|(outpoint, _amount)| SatPoint {
          outpoint: *outpoint,
          offset: 0,
        })
        .ok_or_else(|| anyhow!("wallet contains no cardinal utxos"))?
    };
    vec![satpoint]
    };

    let mut reinscription = false;

    for satpoint in satpoints.clone() {
    for (inscribed_satpoint, inscription_id) in &wallet_inscriptions {
      if *inscribed_satpoint == satpoint {
        reinscription = true;
        if self.reinscribe {
          continue;
        } else {
          return Err(anyhow!("sat at {} already inscribed", satpoint));
        }
      }

      if inscribed_satpoint.outpoint == satpoint.outpoint {
        return Err(anyhow!(
          "utxo {} already inscribed with inscription {inscription_id} on sat {inscribed_satpoint}",
          satpoint.outpoint,
        ));
      }
    }
    }

    if self.reinscribe && !reinscription {
      return Err(anyhow!(
        "reinscribe flag set but this would not be a reinscription"
      ));
    }

    let secp256k1 = Secp256k1::new();
    let key_pair = if self.key.is_some() {
      secp256k1::KeyPair::from_secret_key(&secp256k1, &PrivateKey::from_wif(&self.key.clone().unwrap())?.inner)
    } else {
      let key_pair = UntweakedKeyPair::new(&secp256k1, &mut rand::thread_rng());
      if self.commit_only {
        eprintln!("use --key {} to reveal this commitment", PrivateKey::new(key_pair.secret_key(), chain.network()).to_wif());
      }
      key_pair
    };
    let (public_key, _parity) = XOnlyPublicKey::from_keypair(&key_pair);

    let reveal_script = Inscription::append_batch_reveal_script(
      &self.inscriptions,
      ScriptBuf::builder()
        .push_slice(public_key.serialize())
        .push_opcode(opcodes::all::OP_CHECKSIG),
    );

    let taproot_spend_info = TaprootBuilder::new()
      .add_leaf(0, reveal_script.clone())
      .expect("adding leaf should work")
      .finalize(&secp256k1, public_key)
      .expect("finalizing taproot builder should work");

    let control_block = taproot_spend_info
      .control_block(&(reveal_script.clone(), LeafVersion::TapScript))
      .expect("should compute control block");

    let commit_tx_address = Address::p2tr_tweaked(taproot_spend_info.output_key(), chain.network());

    let reveal_change_address = if !self.next_inscriptions.is_empty() {
      let next_reveal_script = Inscription::append_batch_reveal_script(
        &self.next_inscriptions,
        ScriptBuf::builder()
          .push_slice(public_key.serialize())
          .push_opcode(opcodes::all::OP_CHECKSIG),
      );

      let next_taproot_spend_info = TaprootBuilder::new()
        .add_leaf(0, next_reveal_script.clone())
        .expect("adding leaf should work")
        .finalize(&secp256k1, public_key)
        .expect("finalizing taproot builder should work");

      Some(Address::p2tr_tweaked(next_taproot_spend_info.output_key(), chain.network()))
    } else if change.is_some() {
      Some(change.clone().unwrap()[0].clone())
    } else {
      None
    };

    let total_postage = if self.inscribe_on_specific_utxos {
      self.inscriptions.iter().map(|entry| utxos[&entry.utxo.unwrap()]).sum::<Amount>()
    } else {
      match self.mode {
      Mode::SameSat => self.postage,
      Mode::SharedOutput | Mode::SeparateOutputs => {
        self.postage * u64::try_from(self.inscriptions.len()).unwrap()
      }
      }
    };

    let mut reveal_inputs = self.reveal_input.clone();
    reveal_inputs.insert(0, OutPoint::null());
    let mut count = 0;
    let mut reveal_outputs = self
      .destinations
      .iter()
      .map(|destination| {
        count += 1;
        TxOut {
          script_pubkey: destination.script_pubkey(),
          value: match self.mode {
            Mode::SeparateOutputs => if self.inscribe_on_specific_utxos {
              utxos[&self.inscriptions[count - 1].utxo.unwrap()].to_sat()
            } else {
              self.postage.to_sat()
            },
            Mode::SharedOutput | Mode::SameSat => total_postage.to_sat(),
          }
        }
      })
      .collect::<Vec<TxOut>>();

    if let Some(ParentInfo {
      location,
      id: _,
      destination,
      tx_out,
    }) = self.parent_info.clone()
    {
      reveal_inputs.insert(0, location.outpoint);
      reveal_outputs.insert(
        0,
        TxOut {
          script_pubkey: destination.script_pubkey(),
          value: tx_out.value,
        },
      );
    }

    let commit_input = if self.parent_info.is_some() { 1 } else { 0 };

    if self.commitment.is_some() {
      reveal_outputs.push(TxOut {
        script_pubkey: reveal_change_address.unwrap().script_pubkey(),
        value: 0,
      });
    }

    let (_, mut reveal_fee, reveal_vsize) = Self::build_reveal_transaction(
      &control_block,
      self.reveal_fee_rate,
      reveal_inputs.clone(),
      commit_input,
      reveal_outputs.clone(),
      &reveal_script,
    );

    let commit_vsize = if self.fee_utxos.is_empty() {
      0
    } else {
      let dummy_commit_tx = TransactionBuilder::new(
        satpoints.clone(),
        wallet_inscriptions.clone(),
        utxos.clone(),
        locked_utxos.clone(),
        runic_utxos.clone(),
        commit_tx_address.clone(),
        change.clone(),
        self.commit_fee_rate,
        Target::NoChange(Amount::from_sat(0)),
        force_input.clone(),
        self.no_wallet,
      ).build_transaction()?;

      if self.no_wallet {
        if let Some(commit_vsize) = self.commit_vsize {
          commit_vsize
        } else {
          // todo - can we figure out how big this will be after signing without signing it?
          let dummy_commit_psbt = general_purpose::STANDARD.encode(Psbt::from_unsigned_tx(dummy_commit_tx)?.serialize());
          return Ok((None, None, None, None, Some(dummy_commit_psbt)));
        }
      } else {
        let dummy_commit_signed = client.sign_raw_transaction_with_wallet(&dummy_commit_tx, None, None)?;
        if !dummy_commit_signed.complete {
          for error in dummy_commit_signed.errors.unwrap() {
            eprintln!("{:#?}", error);
          }
          bail!("failed to sign dummy commit tx");
        }
        client.decode_raw_transaction(&dummy_commit_signed.hex, None)?.vsize as u64
      }
    };

    if !self.fee_utxos.is_empty() {
      let fee_utxos_value = self.fee_utxos.iter().map(|outpoint| utxos[&outpoint]).sum::<Amount>();
      let total_vsize = commit_vsize + reveal_vsize;
      // eprintln!("total_vsize {} = commit_vsize {} + reveal_vsize {}", total_vsize, commit_vsize, reveal_vsize);
      reveal_fee = (fee_utxos_value * reveal_vsize + Amount::from_sat(total_vsize - 1)) / total_vsize;
      // eprintln!("reveal_fee = (fee_utxos {} * reveal_vsize {} + total_vsize {} - 1) / total_vsize {} = reveal_fee {}", fee_utxos_value.to_sat(), reveal_vsize, total_vsize, total_vsize, reveal_fee.to_sat());
    } else if let Some(r) = self.reveal_fee {
      if r < reveal_fee {
        return Err(anyhow!("requested reveal_fee is too small; should be at least {} sats", reveal_fee.to_sat()));
      }

      reveal_fee = r;
    }

    let unsigned_commit_tx = if self.commitment.is_some() {
      Transaction {
        version: 0,
        lock_time: LockTime::ZERO,
        input: vec![],
        output: vec![],
      }
    } else {
      TransactionBuilder::new(
      satpoints,
      wallet_inscriptions,
      utxos.clone(),
      locked_utxos.clone(),
      runic_utxos,
      commit_tx_address.clone(),
      change,
      self.commit_fee_rate,
      if self.commit_only {
        Target::NoChange(reveal_fee + total_postage)
      } else if !self.fee_utxos.is_empty() {
        Target::ChangeIsFee(reveal_fee + total_postage)
      } else {
        Target::Value(reveal_fee + total_postage)
      },
      force_input,
      self.no_wallet,
      )
        .build_transaction()?
    };

    let mut reveal_input_value = Amount::from_sat(0);
    let mut reveal_input_prevouts = Vec::new();
    for i in &self.reveal_input {
      let output = index.get_transaction(i.txid)?.unwrap().output[i.vout as usize].clone();
      reveal_input_value += Amount::from_sat(output.value);
      reveal_input_prevouts.push(output.clone());
      utxos.insert(*i, Amount::from_sat(output.value));
    }

    let vout = if self.commitment.is_some() {
      reveal_inputs[commit_input] = self.commitment.unwrap();

      if let Some(last) = reveal_outputs.last_mut() {
        (*last).value = (reveal_input_value + self.commitment_output.clone().unwrap().value - total_postage - reveal_fee).to_sat();
      }

      0
    } else {
      let (vout, _commit_output) = unsigned_commit_tx
        .output
        .iter()
        .enumerate()
        .find(|(_vout, output)| output.script_pubkey == commit_tx_address.script_pubkey())
        .expect("should find sat commit/inscription output");

      reveal_inputs[commit_input] = OutPoint {
        txid: unsigned_commit_tx.txid(),
        vout: vout.try_into().unwrap(),
      };

      vout
    };

    let (mut reveal_tx, _fee, _vsize) = Self::build_reveal_transaction(
      &control_block,
      self.reveal_fee_rate,
      reveal_inputs,
      commit_input,
      reveal_outputs.clone(),
      &reveal_script,
    );

    if reveal_tx.output[commit_input].value
      < reveal_tx.output[commit_input]
        .script_pubkey
        .dust_value()
        .to_sat()
    {
      bail!("commit transaction output would be dust");
    }

    let mut prevouts = vec![
      if self.commitment.is_some() {
        TxOut {
          value: self.commitment_output.clone().unwrap().value.to_sat(),
          script_pubkey: self.commitment_output.clone().unwrap().script_pub_key.script()?
        }
      } else {
        unsigned_commit_tx.output[vout].clone()
      }
    ];

    if let Some(parent_info) = self.parent_info.clone() {
      prevouts.insert(0, parent_info.clone().tx_out);
      if self.no_wallet {
        utxos.insert(parent_info.location.outpoint, Amount::from_sat(parent_info.tx_out.value));
      }
    }

    prevouts.extend(reveal_input_prevouts);

    let mut sighash_cache = SighashCache::new(&mut reveal_tx);

    let sighash = sighash_cache
      .taproot_script_spend_signature_hash(
        commit_input,
        &Prevouts::All(&prevouts),
        TapLeafHash::from_script(&reveal_script, LeafVersion::TapScript),
        TapSighashType::Default,
      )
      .expect("signature hash should compute");

    let sig = secp256k1.sign_schnorr(
      &secp256k1::Message::from_slice(sighash.as_ref())
        .expect("should be cryptographically secure hash"),
      &key_pair,
    );

    let witness = sighash_cache
      .witness_mut(commit_input)
      .expect("getting mutable witness reference should work");

    witness.push(
      Signature {
        sig,
        hash_ty: TapSighashType::Default,
      }
      .to_vec(),
    );

    witness.push(reveal_script);
    witness.push(&control_block.serialize());

    let recovery_key_pair = key_pair.tap_tweak(&secp256k1, taproot_spend_info.merkle_root());

    let (x_only_pub_key, _parity) = recovery_key_pair.to_inner().x_only_public_key();
    assert_eq!(
      Address::p2tr_tweaked(
        TweakedPublicKey::dangerous_assume_tweaked(x_only_pub_key),
        chain.network(),
      ),
      commit_tx_address
    );

    let reveal_weight = reveal_tx.weight();

    if !self.no_limit && reveal_weight > bitcoin::Weight::from_wu(MAX_STANDARD_TX_WEIGHT.into()) {
      bail!(
        "reveal transaction weight greater than {MAX_STANDARD_TX_WEIGHT} (MAX_STANDARD_TX_WEIGHT): {reveal_weight}"
      );
    }

    utxos.insert(
      reveal_tx.input[commit_input].previous_output,
      if self.commitment.is_some() {
        self.commitment_output.clone().unwrap().value
      } else {
      Amount::from_sat(
        unsigned_commit_tx.output[reveal_tx.input[commit_input].previous_output.vout as usize]
          .value,
      )
      },
    );

    let total_fees =
      if self.commitment.is_some() {
        0
      } else {
        Self::calculate_fee(&unsigned_commit_tx, &utxos)
      } + if self.commit_only {
        0
      } else {
        Self::calculate_fee(&reveal_tx, &utxos)
      };

    Ok((Some(unsigned_commit_tx), Some(reveal_tx), Some(recovery_key_pair), Some(total_fees), None))
  }

  fn get_recovery_key(
    client: &Client,
    recovery_key_pair: TweakedKeyPair,
    network: Network,
  ) -> Result<String> {
    let recovery_private_key =
      PrivateKey::new(recovery_key_pair.to_inner().secret_key(), network).to_wif();
    Ok(format!(
      "rawtr({})#{}",
      recovery_private_key,
      client
        .get_descriptor_info(&format!("rawtr({})", recovery_private_key))?
        .checksum
    ))
  }

  fn backup_recovery_key(
    client: &Client,
    recovery_key_pair: TweakedKeyPair,
    network: Network,
  ) -> Result {
    let recovery_private_key = PrivateKey::new(recovery_key_pair.to_inner().secret_key(), network);

    let info = client.get_descriptor_info(&format!("rawtr({})", recovery_private_key.to_wif()))?;

    let response = client.import_descriptors(ImportDescriptors {
      descriptor: format!("rawtr({})#{}", recovery_private_key.to_wif(), info.checksum),
      timestamp: Timestamp::Now,
      active: Some(false),
      range: None,
      next_index: None,
      internal: Some(false),
      label: Some("commit tx recovery key".to_string()),
    })?;

    for result in response {
      if !result.success {
        return Err(anyhow!("commit tx recovery key import failed"));
      }
    }

    Ok(())
  }

  fn build_reveal_transaction(
    control_block: &ControlBlock,
    fee_rate: FeeRate,
    inputs: Vec<OutPoint>,
    commit_input_index: usize,
    outputs: Vec<TxOut>,
    script: &Script,
  ) -> (Transaction, Amount, u64) {
    let reveal_tx = Transaction {
      input: inputs
        .iter()
        .map(|outpoint| TxIn {
          previous_output: *outpoint,
          script_sig: script::Builder::new().into_script(),
          witness: Witness::new(),
          sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
        })
        .collect(),
      output: outputs,
      lock_time: LockTime::ZERO,
      version: 2,
    };

    let (fee, vsize) = {
      let mut reveal_tx = reveal_tx.clone();

      for (current_index, txin) in reveal_tx.input.iter_mut().enumerate() {
        // add dummy inscription witness for reveal input/commit output
        if current_index == commit_input_index {
          txin.witness.push(
            Signature::from_slice(&[0; SCHNORR_SIGNATURE_SIZE])
              .unwrap()
              .to_vec(),
          );
          txin.witness.push(script);
          txin.witness.push(&control_block.serialize());
        } else {
          txin.witness = Witness::from_slice(&[&[0; SCHNORR_SIGNATURE_SIZE]]);
        }
      }

      let vsize = reveal_tx.vsize();
      (fee_rate.fee(vsize), vsize as u64)
    };

    (reveal_tx, fee, vsize)
  }

  fn calculate_fee(tx: &Transaction, utxos: &BTreeMap<OutPoint, Amount>) -> u64 {
    tx.input
      .iter()
      .map(|txin| utxos.get(&txin.previous_output).unwrap().to_sat())
      .sum::<u64>()
      .checked_sub(tx.output.iter().map(|txout| txout.value).sum::<u64>())
      .unwrap()
  }
}

#[derive(PartialEq, Debug, Copy, Clone, Serialize, Deserialize, Default)]
pub(crate) enum Mode {
  #[serde(rename = "same-sat")]
  SameSat,
  #[default]
  #[serde(rename = "separate-outputs")]
  SeparateOutputs,
  #[serde(rename = "shared-output")]
  SharedOutput,
}

#[derive(Deserialize, Default, PartialEq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct BatchEntry {
  pub(crate) destination: Option<Address<NetworkUnchecked>>,
  pub(crate) file: PathBuf,
  pub(crate) metadata: Option<serde_yaml::Value>,
  pub(crate) metadata_json: Option<serde_json::Value>,
  pub(crate) metaprotocol: Option<String>,
  pub(crate) utxo: Option<OutPoint>,
}

impl BatchEntry {
  pub(crate) fn metadata(&self) -> Result<Option<Vec<u8>>> {
    Ok(match &self.metadata {
      None => match &self.metadata_json {
        Some(metadata) => {
          let mut cbor = Vec::new();
          ciborium::into_writer(&metadata, &mut cbor)?;
          Some(cbor)
        }
        None => None,
      }
      Some(metadata) => {
        let mut cbor = Vec::new();
        ciborium::into_writer(&metadata, &mut cbor)?;
        Some(cbor)
      }
    })
  }
}

#[derive(Deserialize, PartialEq, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct Batchfile {
  pub(crate) fees: Option<Vec<OutPoint>>,
  pub(crate) inscriptions: Vec<BatchEntry>,
  pub(crate) mode: Mode,
  pub(crate) parent: Option<InscriptionId>,
  pub(crate) parent_satpoint: Option<SatPoint>,
  pub(crate) postage: Option<u64>,
  pub(crate) sat: Option<Sat>,
}

impl Batchfile {
  pub(crate) fn load(path: &Path) -> Result<Batchfile> {
    let batchfile: Batchfile = serde_yaml::from_reader(File::open(path)?)?;

    if batchfile.inscriptions.is_empty() {
      bail!("batchfile must contain at least one inscription");
    }

    Ok(batchfile)
  }

  pub(crate) fn inscriptions(
    &self,
    client: &Client,
    chain: Chain,
    parent_value: Option<u64>,
    metadata: Option<Vec<u8>>,
    postage: Amount,
    compress: bool,
    utxos: &mut BTreeMap<OutPoint, Amount>,
  ) -> Result<(Vec<Inscription>, Vec<Address>, bool, Vec<OutPoint>)> {
    assert!(!self.inscriptions.is_empty());

    if self
      .inscriptions
      .iter()
      .any(|entry| entry.destination.is_some())
      && self.mode == Mode::SharedOutput
    {
      return Err(anyhow!(
        "individual inscription destinations cannot be set in shared-output mode"
      ));
    }

    let inscribe_on_specific_utxos = if self.inscriptions.iter().any(|entry| entry.utxo.is_some()) {
      if self.inscriptions.iter().all(|entry| entry.utxo.is_some()) {
        true
      } else {
        return Err(anyhow!("if utxo is set for any inscription it must be set for all inscriptions"))
      }
    } else {
      false
    };

    if inscribe_on_specific_utxos {
      if self.postage.is_some() {
        return Err(anyhow!("postage size cannot be set when specifying the utxo to inscribe on for each inscription"))
      }

      if self.mode == Mode::SameSat {
        return Err(anyhow!("Inscription utxos can't be specified in `same-sat` mode"));
      }

      for outpoint in self.inscriptions.iter().map(|entry| entry.utxo.unwrap()) {
        if !utxos.contains_key(&outpoint) {
          utxos.insert(outpoint, Amount::from_sat(client.get_raw_transaction(&outpoint.txid, None)?.output[outpoint.vout as usize].value));
        }
      }
    }

    if metadata.is_some() {
      assert!(self
        .inscriptions
        .iter()
        .all(|entry| entry.metadata.is_none()));
    }

    let mut pointer = parent_value.unwrap_or_default();

    let mut inscriptions = Vec::new();
    for (i, entry) in self.inscriptions.iter().enumerate() {
      inscriptions.push(Inscription::from_file(
        chain,
        &entry.file,
        self.parent,
        if i == 0 { None } else { Some(pointer) },
        entry.metaprotocol.clone(),
        match &metadata {
          Some(metadata) => Some(metadata.clone()),
          None => entry.metadata()?,
        },
        compress,
        entry.utxo,
      )?);

      if inscribe_on_specific_utxos {
        pointer += utxos[&entry.utxo.unwrap()].to_sat();
      } else {
        pointer += postage.to_sat();
      }
    }

    let destinations = match self.mode {
      Mode::SharedOutput | Mode::SameSat => vec![get_change_address(client, chain)?],
      Mode::SeparateOutputs => self
        .inscriptions
        .iter()
        .map(|entry| {
          entry.destination.as_ref().map_or_else(
            || get_change_address(client, chain),
            |address| {
              address
                .clone()
                .require_network(chain.network())
                .map_err(|e| e.into())
            },
          )
        })
        .collect::<Result<Vec<_>, _>>()?,
    };

    let fees = match self.fees.clone() {
      Some(fees) => fees,
      None => Vec::new(),
    };

    Ok((inscriptions, destinations, inscribe_on_specific_utxos, fees))
  }
}
