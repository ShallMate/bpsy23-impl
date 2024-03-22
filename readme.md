# How to run

1. Install [Rust](https://www.rust-lang.org/tools/install)
2. Run
    ```
    cargo run -r --example okvs_perf -- -n 1048576
    # -n how many, default 65536
    # -e epsilon, default 0.03 for bpsy23
    # -w width, default 570
    ```

# Ref

(BPSY23) Near-Optimal Oblivious Key-Value Stores for Efficient PSI, PSU and Volume-Hiding Multi-Maps