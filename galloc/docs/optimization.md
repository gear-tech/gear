# Galloc implementations comparison


## Contents
1. [Summary](#summary)
1. [Benchmark](#benchmark)
1. [Current implementation](#current)
1. [Implementation without static buffer](#without)
1. [Implementation with increased by two static buffer](#increased-twice)
1. [Implementation with multiple static buffers](#multiple)
1. [Conclusion](#conclusion)

## Summary
<a name="summary"></a>
We've tried to optimize galloc in some ways, but none of them showed significant performance improvement, so we'd **strongly suggest keeping the current implementation.**

The current implementation uses a static buffer in addition to standard <kbd>dlmalloc</kbd>-like implementation.

| Test case | NFT gas consumption | FT gtest gas consumption | FT gclient gas consumption |
| --- | --- | --- | --- |
| Current implementation | 9.01% | 1.82% | 6.59% |
| Implementation without static buffer | 10.24% | 1.81% | 6.69% |
| Implementation with increased by two static buffer | 8.30% | 1.82% | 6.60% |
| Implementation with multiple static buffers | 8.23% | 1.88% | 7.01% |


## Benchmark
<a name="benchmark"></a>
We measured the performance of dlmalloc in its current state [(commit 9135baa)](https://github.com/gear-tech/dlmalloc-rust/tree/9135baa728ef9a9a04a887998e019733c4b093af) to establish a baseline for comparison.

All optimization attempts were tested three times on different test cases, and the average of the results was used for performance comparison. Our primary metric is gas consumption, where lower gas consumption is considered better. All optimizations were measured using release mode binaries with most optimizations enabled, and the results were compared against the current performance.

Gas measurements were obtained through `debug` and `gas_available` syscalls, which were temporarily made free. Each allocation top-level operation (e.g., malloc, calloc, ...) was measured separately. The gas was measured before and after the function call, and the gas consumed was calculated as the difference between the before and after gas values.

The gas consumption was then calculated as a percentage of the total gas spent on the test case.

We intentionally did not provide any information about the testing machine since we only measure gas consumption, which remains the same across machines due to fixed weights assigned to each instruction and syscall.

The test cases were:
- `NFT init -> mint -> burn`: This test case is for measuring the performance of the NFT contract, which is one of the common cases of smart contracts. The test case consists of the following steps:
  - `init`: Initialise the NFT contract.
  - `mint`: Mint 1 NFT.
  - `burn`: Burn 1 NFT.

  This is quite a simple case, and it's not the most common case, but it's the simplest case we can think of.

  You can see the code of the test case [here](https://github.com/gear-dapps/non-fungible-token/blob/0.2.10/tests/node_tests.rs) (`burn-test`).
- `FT stress-test` with <kbd>gtest</kbd>: This test case is for measuring the performance of the FT contract, which is also one of the common cases of smart contracts. The test case consists of the following steps:
  - `init`: Initialise the FT contract.
  - `mint`: Mint 1 000 000 FT to the first user.
  - `transfer`: Transfer 6 000 FT to the first 20 accounts from the first user.
  - `balance`: Check the balance of the first account to prove it sent 6 000 FT to the first 20 accounts.
  - `balance`: Check the balance of the first 20 accounts.
  - `transfer`: Transfer 1 000 FT from the first 20 accounts to users 21-100.
  - `balance`: Check the balance of the first 20 accounts to prove it sent 1 000 FT to users 21-100.
  - `balance`: Check the balance of users 21-100 to prove they've received 1 000 FT from the first 20 accounts.
  - `mint`: Mint 1 000 000 FT to the second user. 
  - `mint`: Mint 5 000 FT to users 87..120.
  - `balance`: Check the balance of users 87..120 to prove they've received 5 000 FT after mint.
  - `transfer`: Transfer 1 000 FT from users 87..120 to the first user.
  - `total_supply`: Check the total supply of FT to prove it's sums up to correct value.
  - `balance`: Check the balance of users 918..13400
  - `mint`: Mint 5 000 FT to users 918..13400.
  - `transfer`: Transfer `i / 4` FT from users 918..13400 to the user `i * 2`.

  The next two steps were repeated 30 times:
  - `balance`: Check the balance of users 1..130
  - `transfer`: Transfer 1 FT from users 1..130 to the first user.

  This test tries to imitate what happens when the FT contract is used in the real world, with many clients and memory allocations therefore.

- `FT stress-test` with <kbd>gclient</kbd>: This test case has the same steps as the previous one, but it's executed with <kbd>gclient</kbd> instead of <kbd>gtest</kbd>. This test is most similar to the real world case because it's executed with the real node and will give the most realistic gas usage. 

  You can see the code of the test case [here](../../gstd/src/benchmarks/mod.rs).

## Current implementation
<a name="current"></a>
Here are the results of the current implementation of galloc (with static buffer):

| Test case | Allocator gas consumption |
| --- | --- |
| `NFT init -> mint -> burn` | 9.01% |
| `FT stress-test` with <kbd>gtest</kbd> | 1.82% |
| `FT stress-test` with <kbd>gclient</kbd> | 6.59% |

### `NFT init -> mint -> burn`
| | malloc        | calloc      | realloc          | free           |
| -------------------------- | ------------- | ----------- | ---------------- | -------------- |
| % of total gas spent | 3,00%         | 4,32%       | 0,91%            | 0,77%          |

### `FT stress-test` with <kbd>gtest</kbd>
|       | malloc        | calloc      | realloc          | free            |
| -------------------------- | ------------- | ----------- | ---------------- | --------------- |
| % of total gas spent | 0,42%         | 0,57%       | 0,43%            | 0,39%           |

### `FT stress-test` with <kbd>gclient</kbd>
|  | malloc             | calloc            | realloc           | free                |
| -------------------------- | ------------------ | ----------------- | ----------------- | ------------------- |
| % of total gas spent | 0,81%              | 1,10%             | 1,82%             | 2,86%               |

[Go to top](#)

## Implementation without static buffer
<a name="without"></a>
We attempted to eliminate the static buffer and instead utilize a <kbd>dlmalloc</kbd> default implementation, but it did not result in any significant performance improvement. In fact, in most cases, it actually demonstrated worse performance. Hence, it appears that the static buffer is not the bottleneck.

| Test case | Allocator gas consumption |
| --- | --- |
| `NFT init -> mint -> burn` | 10.24% |
| `FT stress-test` with <kbd>gtest</kbd> | 1.81% |
| `FT stress-test` with <kbd>gclient</kbd> | 6.69% |

### `NFT init -> mint -> burn`

|| malloc        | calloc      | realloc          | free           |
| -------------------------- | ------------- | ----------- | ---------------- | -------------- |
| % of total gas spent | 3,62%         | 4,77%       | 1,08%            | 0,77%          |

### `FT stress-test` with <kbd>gtest</kbd>

| | malloc        | calloc      | realloc          | free            |
| -------------------------- | ------------- | ----------- | ---------------- | --------------- |
| % of total gas spent | 0,40%         | 0,56%       | 0,46%            | 0,40%           |

### `FT stress-test` with <kbd>gclient</kbd>

| | malloc             | calloc            | realloc           | free                |
| -------------------------- | ------------------ | ----------------- | ----------------- | ------------------- |
| % of total gas spent | 0,73%              | 1,09%             | 2,01%             | 2,85%               |

[Go to top](#)

## Increase static buffer size by two
<a name="increased-twice"></a>
We attempted to increase the size of the static buffer by two, and while its performance showed a slight improvement over the current implementation, it was not significant enough to change the current implementation.

| Test case | Allocator gas consumption |
| --- | --- |
| `NFT init -> mint -> burn` | 8.30% |
| `FT stress-test` with <kbd>gtest</kbd> | 1.82% |
| `FT stress-test` with <kbd>gclient</kbd> | 6.60% |

### `NFT init -> mint -> burn`

|  | malloc        | calloc      | realloc          | free           |
| -------------------------- | ------------- | ----------- | ---------------- | -------------- |
| % of total gas spent | 2,82%         | 4,02%       | 0,70%            | 0,76%          |

### `FT stress-test` with <kbd>gtest</kbd>

| | malloc        | calloc      | realloc          | free            |
| -------------------------- | ------------- | ----------- | ---------------- | --------------- |
| % of total gas spent | 0,42%         | 0,57%       | 0,43%            | 0,39%           |

### `FT stress-test` with <kbd>gclient</kbd>

| | malloc             | calloc            | realloc           | free                |
| -------------------------- | ------------------ | ----------------- | ----------------- | ------------------- |
| % of total gas spent | 0,82%              | 1,10%             | 1,82%             | 2,86%               |

[Go to top](#)

## Multiple static buffers
<a name="multiple"></a>
We tried to implement not just one static buffer, but multiple static buffers with different sizes. However, the study revealed that this effort was not worthwhile, as most allocations were still being done in the largest static buffer, resulting in even worse performance compared to the current implementation.

The best-performing combination of buffers was as follows:
- 1-byte buffer with 2 cells
- 2-byte buffer with 2 cells
- 4-byte buffer with 4 cells
- 6-byte buffer with 4 cells
- 8-byte buffer with 8 cells
- 32-byte buffer with 4 cells

| Test case | Allocator gas consumption |
| --- | --- |
| `NFT init -> mint -> burn` | 8.23% |
| `FT stress-test` with <kbd>gtest</kbd> | 1.88% |
| `FT stress-test` with <kbd>gclient</kbd> | 7.01% |

### `NFT init -> mint -> burn`

| | malloc        | calloc      | realloc          | free           |
| -------------------------- | ------------- | ----------- | ---------------- | -------------- |
| % of total gas spent | 2,47%         | 4,37%       | 0,61%            | 0,78%          |

### `FT stress-test` with <kbd>gtest</kbd>

| FT stress-test gtest       | malloc        | calloc      | realloc          | free            |
| -------------------------- | ------------- | ----------- | ---------------- | --------------- |
| % of total gas spent | 0,42%         | 0,56%       | 0,49%            | 0,42%           |

### `FT stress-test` with <kbd>gclient</kbd>

|   | malloc             | calloc            | realloc           | free                |
| -------------------------- | ------------------ | ----------------- | ----------------- | ------------------- |
| % of total gas spent | 0,83%              | 1,08%             | 2,15%             | 2,95%               |

[Go to top](#)

## Conclusion
<a name="conclusion"></a>

In this research, we compared different implementations of the `galloc` memory allocator. However, none of the optimizations showed significant performance improvement compared to the current implementation. We recommend keeping the current implementation, which utilizes a static buffer alongside <kbd>dlmalloc</kbd>-like implementation. It consistently demonstrated the lowest gas consumption and proved to be efficient in all test cases. Further exploration of alternative strategies or specific optimizations may be considered in the future, but based on our findings, the current implementation remains the most suitable choice for optimal memory allocation in our project.