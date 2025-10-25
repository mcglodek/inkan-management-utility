# 1) create project folder and files above
cargo build --release

# 2) run against your existing batch JSON
./target/release/inkan_offline_tx_batch \
  --batch ./txCreationInputFiles/sample_batch.json \
  --out ./txOutputFiles/batch_output.json \
  --gas-limit 30000000 \
  --max-fee-per-gas 30000000000 \
  --max-priority-fee-per-gas 2000000000

