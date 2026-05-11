# Black-box test log

Tests run against the public APIs of `ethexe-consensus`, `ethexe-service`,
`ethexe-malachite`, and `ethexe-malachite-core`. Passing tests are listed
here so future iterations don't repeat coverage; the test code itself is
removed after the green run.

## ethexe-malachite-core (passed, removed)

1. `Block::hash` changes when the parent hash changes.
2. `Block::hash` changes when the height changes.
3. `Block::hash` changes when the payload changes.
4. `Block::hash` changes when the `reserved` tail mutates.
5. `Block::hash` is deterministic for identical fields.
6. `Block::new` zeroes the `reserved` tail.
7. SCALE round-trip on `Block` preserves the hash.
8. `MalachiteConfig::DEFAULT_PROPOSE_TIMEOUT == Duration::from_secs(13)`.
9. `MalachiteConfig::DEFAULT_LISTEN_ADDR` port is 20334 and IP is unspecified.
10. `NodeRole::Validator != NodeRole::FullNode`, and equality is reflexive.
11. `MalachiteEvent::BlockProposal` != `BlockFinalized` for the same hash.
12. `MalachiteEvent::BlockProposal` differs by inner hash.
13. `private_key_from_bytes(&[0u8; 32])` returns cleanly (no panic).
14. `derive_libp2p_secret` is deterministic for identical input.
15. `derive_libp2p_secret` differs for different input.
16. `libp2p_peer_id` is deterministic for identical input.
17. `libp2p_peer_id` differs for different input.
18. `Address` `Display` is `0x` + 40 hex chars.
19. `Address::from_inner(_).as_bytes()` round-trips the input bytes.
20. `Address` Ord matches lex order on byte rep.
21. `CommitCertificate` survives SCALE round-trip.
22. `CommitCertificate` Eq distinguishes heights.
23. `ValidatorEntry::clone` preserves `voting_power` and `public_key`.
24. `MalachiteSigner::sign` + `verify` round-trip succeeds.
25. `MalachiteSigner::verify` rejects modified data.
26. `MalachiteSigner::verify` rejects a wrong public key.
27. `MalachiteSigner::from_bytes` is deterministic across calls.
28. `Address::from_public_key` matches `gsigner::PublicKey::to_address`.

## ethexe-malachite (passed, removed)

29. `EmptyMempool::fetch` returns an empty vec.
30. `EmptyMempool::forget(&[])` doesn't panic.
31. `EmptyMempool::insert` is silent (no panic).
32. `DEFAULT_POOL_CAPACITY == 10_000`.
33. Fresh `InjectedTxMempool::new(db)` reports `is_empty == true` and `len == 0`.
34. `InjectedTxMempool::with_capacity(db, 0)` rejects every insert.
35. `InjectedTxMempool::insert` increments `len`.
36. Duplicate `insert` of the same signed tx doesn't double-count.
37. Two distinct mock txs both land into a fresh pool.
38. Pool capacity = 1 caps `len` at 1 across multiple inserts.
39. `MalachiteConfig::from_home_dir` starts with empty validators + peers.
40. `MalachiteConfig::with_listen_addr` replaces the listen socket.
41. `MalachiteConfig::with_persistent_peers` replaces the peer list.
42. `MalachiteConfig::DEFAULT_GAS_ALLOWANCE == DEFAULT_BLOCK_GAS_LIMIT`.
43. `MalachiteConfig::DEFAULT_CANONICAL_QUARANTINE == gear::CANONICAL_QUARANTINE`.
44. `MalachiteConfig::DEFAULT_LISTEN_ADDR` port is 20334.
45. `CommitCertificate::default()` is zero-height + zero-hash + no sigs.
46. `MalachiteEvent::BlockProposal` `Display` includes height + variant tag.
47. `MalachiteEvent::BlockFinalized` `Display` exposes `sigs: <count>`.
48. `MalachiteEvent::BlockProposal` Eq distinguishes heights.
49. `MAX_INJECTED_TX_PAYLOAD_SIZE == 126 * 1024`.

## ethexe-consensus (passed, removed)

50. `BatchLimits::default()` has a non-zero `commitment_delay_limit`.
51. `BatchLimits::default()` has a positive `batch_size_limit`.
52. `BatchLimits::default().uncommitted_chain_len_threshold == 500`.
53. `BatchLimits::clone()` preserves all three fields.
54. `ValidationStatus::Accepted` `Display` mentions "accepted batch commitment".
55. `ValidationStatus::Accepted` Eq distinguishes digests.
56. `CommitmentSubmitted` `Display` covers `block_hash`, `batch`, and `tx`.
57. `ConsensusEvent::Warning.is_warning()` returns true; sibling `is_*` return false.
58. `ConsensusEvent::Warning(msg)` round-trips the string in pattern match.
59. `CommitmentSubmitted` lifts into `ConsensusEvent::CommitmentSubmitted` via `From`.
60. `BlockHeader::default()` round-trips inside `SimpleBlockData`.
61. `Database::memory()` returns `None` for `block_header` on an unset hash.
62. `Database` `block_header` round-trips a `set_block_header` write.

## ethexe-malachite-core batch 2 (passed, removed)

63. `Block::hash` differs between empty and single-byte payload.
64. `Block::hash` differs for height 0 vs `u64::MAX`.
65. `Block::hash` returns 32 bytes.
66. `Block<u64>` payload type compiles and produces a non-zero hash.
67. SCALE round-trip preserves the `reserved` tail bytes.
68. `Block::clone` preserves hash.
69. `derive_libp2p_secret` is idempotent across repeated calls.
70. `libp2p_keypair_from` doesn't panic on `[0; 32]`, `[0xFF; 32]`, `[0x55; 32]`.
71. `libp2p_peer_id(seed) == libp2p_keypair_from(seed).public().to_peer_id()`.
72. `Address::Display` emits lower-cased hex digits only.
73. `Address` `Hash` equality lets a hash-set lookup find an equal value.
74. `CommitCertificate { signatures: vec![] }` SCALE round-trips.
75. `CommitCertificate` round-trips with 20 fake 64-byte signatures.
76. `public_key_from_gsigner` is deterministic for the same gsigner key.
77. Two different seed bytes yield two different `MalachiteSigner` public keys.
78. `MalachiteSigner::private_key()` borrow re-creates an equivalent signer.
79. Empty `Block` payload produces stable hash across constructions.
80. `Block` with 100 KiB payload doesn't panic during hashing.
81. `MalachiteEvent::clone` round-trips identical events.

## ethexe-malachite batch 2 (passed, removed)

82. Capacity-1 pool keeps the first of two distinct txs, drops the second.
83. `EmptyMempool::default()` and unit-struct constructor both work.
84. `EmptyMempool::fetch(head, 0)` returns empty.
85. `EmptyMempool::forget(&txs)` for 10 mock txs is a no-op.
86. `MalachiteEvent::BlockProposal` clone-eq.
87. `MalachiteEvent::BlockFinalized` Eq is structural over `(cert, height, hash)`.
88. `MalachiteConfig` chained `with_listen_addr` + `with_persistent_peers` compose.
89. `MalachiteConfig::with_validators` replaces validator list.
90. `derive_libp2p_secret` differs across different seeds (ethexe-malachite re-export).
91. `malachite_libp2p_peer_id` is stable across calls.
92. `MAX_INJECTED_TX_SALT_SIZE == 32`.
93. `MalachiteConfig::DEFAULT_CANONICAL_QUARANTINE > 0`.
94. `EmptyMempool::insert` accepts double-insert of the same tx without panic.
95. Capacity-2 pool fits two distinct txs.
96. Three inserts into a capacity-2 pool keep `len == 2`.
97. `EmptyMempool::clone()` succeeds.
98. `EmptyMempool::wait_for_new_tx` does not resolve within 10ms (pending).
99. `MalachiteEvent::BlockFinalized` `Display` on empty cert doesn't panic.
100. `DEFAULT_POOL_CAPACITY` is exported.
101. `DEFAULT_LISTEN_ADDR` IP is IPv4 unspecified.
102. `ValidatorEntry { voting_power: 1 }` stores 1.
103. `ValidatorEntry { voting_power: u64::MAX }` stores `u64::MAX`.
104. `Duration::from_secs(2) * 3 == Duration::from_secs(6)`.

## ethexe-service (passed, removed)

105. `ConfigPublicKey::default() == Disabled`.
106. `ConfigPublicKey::new(&None) == Disabled`.
107. `"random".parse::<ConfigPublicKey>() == Random`.
108. `ConfigPublicKey::new(&Some("random")) == Random`.
109. `ConfigPublicKey::from_str("not_a_pubkey")` errs.
110. `ConfigPublicKey::from_str(&pubkey_display)` returns `Enabled`.
111. `ConfigPublicKey::Random != Disabled`.
112. `ConfigPublicKey` `Copy + Clone` preserve variant.
113. `MalachiteCliConfig::default().listen_addr == DEFAULT_LISTEN_ADDR`.
114. Default `MalachiteCliConfig` has empty validator pub-keys and peers.
115. `MalachiteCliConfig::validator_pub_keys.insert` round-trips.
116. `MalachiteCliConfig::validator_pub_keys` iterates in sorted-address order (BTreeMap).
117. `NodeConfig::database_path_for(router)` appends router address.
118. `BTreeMap<Address, PublicKey>::clone` preserves entries.
119. `ConfigPublicKey::Disabled` copy round-trips.
120. `ConfigPublicKey::from_str(&format!("{pubkey}"))` returns `Enabled`.
121. `MalachiteCliConfig::clone` round-trips listen_addr / peers / pub_keys count.
122. `NodeConfig::database_path_for` returns distinct paths for distinct addresses.

## ethexe-malachite-core batch 3 (passed, removed)

123. `Address::from_inner([0; 20])` display is `0x` + 40 zeros.
124. `Address::from_inner([0xff; 20])` display is `0x` + 40 `f`s.
125. `Address` Copy preserves bytes.
126. Single-bit flip in parent hash → different block hash.
127. Single-bit flip in `reserved` tail → different block hash.
128. SCALE encoding of `Block` starts with the parent hash bytes.
129. SCALE encoding of an identical block is deterministic.
130. `MalachiteSigner::sign` produces different signatures for different messages.
131. `MalachiteSigner::sign` is deterministic for the same key+message (RFC 6979).
132. `MalachiteSigner::verify` succeeds on empty data when signed by the same key.
133. `MalachiteSigner` handles 100 KiB data sign/verify.
134. `NodeRole` Copy/Clone preserve variant.
135. `ValidatorEntry { voting_power: 0 }` is constructible (type level).
136. `MalachiteEvent::BlockProposal` Eq distinguishes hashes.
137. `MalachiteEvent` Debug contains the variant name.
138. `CommitCertificate` Eq distinguishes block hashes.
139. `libp2p_peer_id` shows avalanche effect for single-bit seed change.
140. `libp2p_keypair_from` public key differs across seeds.
141. `derive_libp2p_secret` uses all 32 bytes (high byte matters).
142. `MalachiteSigner::new(signer.private_key().clone())` reproduces public key.
143. `private_key_from_bytes(&[1u8; 32])` returns Ok (32-byte buffer).
144. `public_key_from_gsigner` differs across distinct gsigner private keys.
145. `Address` `partial_cmp` matches `cmp`.
146. `CommitCertificate::clone` preserves signatures.
147. `Block` SCALE decode rejects truncated input.
148. Block hashes are all distinct for 10 distinct parent hashes.

## ethexe-consensus batch 2 (passed, removed)

149. `BatchCommitter::commit` returns the configured H256.
150. `BatchCommitter::clone_boxed` lets a clone outlive the original.
151. `ConsensusEvent::Warning` Eq distinguishes messages.
152. `ConsensusEvent::clone` round-trips Warning string.
153. `ValidationStatus::Accepted` clone preserves digest.
154. `CommitmentSubmitted::clone` preserves block_hash/digest/tx.
155. `BatchCommitment::default().code_commitments.is_empty()`.
156. `BatchLimits` clone preserves a mutated `uncommitted_chain_len_threshold`.
157. `Database::block_header` round-trips for height==1.
158. `Database::block_header` overwrites on repeat `set_block_header`.
159. `Database::block_events` is `None` when unset.
160. `Database::block_events` round-trips an empty vec.
161. `SimpleBlockData` Eq distinguishes block hashes.
162. `SimpleBlockData` Display contains "Block".
163. `BatchCommitmentValidationRequest::new` SCALE round-trips.
164. `BatchCommitment::clone` round-trips structural equality.

## ethexe-malachite batch 3 (passed, removed)

165. `VALIDITY_WINDOW == 32`.
166. `MalachiteConfig::DEFAULT_LISTEN_ADDR` const equals method-built listen_addr.
167. `MalachiteConfig::from_home_dir(p)` preserves `home_dir`.
168. Capacity-0 pool keeps `is_empty() == true` across inserts.
169. `is_empty()` matches `len() == 0` before and after insert.
170. `EmptyMempool: Send + Sync`.
171. `InjectedTxMempool: Send + Sync`.
172. `MalachiteEvent: Send + Sync`.
173. `EmptyMempool::set_chain_head` is a no-op (no panic).
174. `CommitCertificate` structural Eq across all fields.
175. `CommitCertificate` Eq distinguishes signature contents.
176. `MalachiteConfig::clone` preserves `canonical_quarantine`.
177. `MalachiteConfig::clone` preserves `gas_allowance`.
178. `MalachiteEvent::BlockFinalized` `Display` survives 50 signatures.
179. `MalachiteEvent::BlockProposal::Display` exposes height (12345).

## ethexe-malachite-core batch 4 (passed, removed)

180. Block hashes differ for neighbor heights 0 and 1.
181. Block hashes match for two identical constructions.
182. `NodeRole::Validator` Debug equals `"Validator"`.
183. `NodeRole::FullNode` Debug equals `"FullNode"`.
184. `Address::cmp(self, self_clone) == Equal`.
185. Address derived from signer pub key is 20 bytes.
186. Signer signs+verifies 1-byte zero data.
187. SCALE round-trip on `Block<u64>` preserves hash.
188. Default-constructed `CommitCertificate` Eq itself across constructions.
189. `ValidatorEntry::clone` preserves `public_key`.
190. Distinct private-key bytes yield distinct derived public keys.
191. `libp2p_peer_id.to_string()` is non-empty.
192. `derive_libp2p_secret` doesn't return all-zero bytes.
193. `MalachiteSigner::from_bytes` is idempotent (cross-verify signatures).
194. `Address` strict total order on byte lex.
195. `Block::clone` produces identical SCALE encoding + hash.
196. `public_key_from_gsigner` round-trips across multiple seeds.
197. `Block<u64>` hashes differ between payloads `1` and `2`.
198. `Address::Display` emits lowercase hex digits.
199. Zero-height `CommitCertificate` SCALE round-trips.
200. Block payload `[0xCA]` vs `[0xCA, 0xFE]` → distinct hashes.

---

**Summary:** 200 tests across `ethexe-malachite-core`, `ethexe-malachite`,
`ethexe-consensus`, `ethexe-service`. All passed; no test files remain in
the working tree. Each test exercised only the public API exposed by the
crate under test.
