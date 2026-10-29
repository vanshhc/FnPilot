# Dependency and asset provenance

Generate `docs/dependency-inventory.json` using `python3 scripts/license-inventory.py docs/dependency-inventory.json` after locked installs. The inventory records Rust/Node package versions, declared licenses, source repositories, lock hashes/integrity and the pinned native runtime. Local registry/home paths are omitted.

The machine-readable inventory is an audit input, not redistribution approval or a complete bundled notice file. Before release, collect the actual license/NOTICE files of every shipped dependency, review mixed-license choices and native-library redistribution, and include them in the exact artifact inventory. Existing Swift/llama.cpp notices remain in `THIRD_PARTY_NOTICES.txt`. OS frameworks and WebView2 have separate platform distribution terms.

The preview icon is an original geometric F created for this project. No third-party photo, font, screenshot or voice is added. Text fixtures remain the original synthetic corpus. Model weights and audio clips are not distributed; each requires independent upstream revision, license, checksum and provenance approval. Generated/synthetic speech is not automatically redistributable.
