# Galloc implementations comparison

1. [Summary](#summary)
1. [Benchmark](#benchmark)
1. [Current implementation](#current)
1. [Implementation without static buffer](#without)
1. [Implementation with increased by two static buffer](#increased-twice)
1. [Implementation with multiple static buffers](#multiple)

## Content

### Summary
<a name="summary"></a>
We've tried to optimize galloc in some ways, but none of them showed significant performance improvement, so we'd **strongly suggest keeping the current implementation.**

The current implementation uses a static buffer in addition to standard <kbd>dlmalloc</kbd>-like implementation.

| Test case | NFT gas consumption | FT gtest gas consumption | FT gclient gas consumption |
| --- | --- | --- | --- |
| Current implementation | 9.01% | 1.82% | 6.59% |
| Implementation without static buffer | 10.24% | 1.81% | 6.69% |
| Implementation with increased by two static buffer | 8.30% | 1.82% | 6.60% |
| Implementation with multiple static buffers | 8.23% | 1.88% | 7.01% |


### Benchmark
<a name="benchmark"></a>
We've measured the current [(commit 9135baa)](https://github.com/gear-tech/dlmalloc-rust/tree/9135baa728ef9a9a04a887998e019733c4b093af) performance of dlmalloc to have a baseline for comparison.

All optimization attempts were tested 3 times on the different test cases, and the average of the results was used to compare the performance. Our main metric is gas consumption: the less gas is consumed, the better. All optimizations were measured in release mode binaries with most optimizations enabled and compared to current performance.

The gas measurements were made via `debug` and `gas_available` syscalls, which were temporarily made free. Each allocation top-level operation (i.e., malloc, calloc, ...) was measured separately; the gas was measured before and after the function call and calculated as the subtraction result of gas before and gas after.

Then the gas consumption was calculated as a percentage of the total gas spent on the test case.

We've intentionally not put any info about the testing machine because we measure only gas consumption, which will be the same for every machine since we have fixed weights for every instruction and syscall.

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

### Current implementation
<a name="current"></a>
Here are the results of the current implementation of galloc (with static buffer):

| Test case | Allocator gas consumption |
| --- | --- |
| `NFT init -> mint -> burn` | 9.01% |
| `FT stress-test` with <kbd>gtest</kbd> | 1.82% |
| `FT stress-test` with <kbd>gclient</kbd> | 6.59% |

#### `NFT init -> mint -> burn`
| | malloc        | calloc      | realloc          | free           |
| -------------------------- | ------------- | ----------- | ---------------- | -------------- |
| % of total gas spent | 3,00%         | 4,32%       | 0,91%            | 0,77%          |

#### `FT stress-test` with <kbd>gtest</kbd>
|       | malloc        | calloc      | realloc          | free            |
| -------------------------- | ------------- | ----------- | ---------------- | --------------- |
| % of total gas spent | 0,42%         | 0,57%       | 0,43%            | 0,39%           |

#### `FT stress-test` with <kbd>gclient</kbd>
|  | malloc             | calloc            | realloc           | free                |
| -------------------------- | ------------------ | ----------------- | ----------------- | ------------------- |
| % of total gas spent | 0,81%              | 1,10%             | 1,82%             | 2,86%               |

[Go to top](#)

### Implementation without static buffer
<a name="without"></a>
We've tried to remove the static buffer and use only <kbd>dlmalloc</kbd>-like implementation, but it showed no significant performance improvement. In most cases, it even showed worse performance. So it seems that the static buffer is not the bottleneck.

### Test data

| Test case | Allocator gas consumption |
| --- | --- |
| `NFT init -> mint -> burn` | 10.24% |
| `FT stress-test` with <kbd>gtest</kbd> | 1.81% |
| `FT stress-test` with <kbd>gclient</kbd> | 6.69% |

#### `NFT init -> mint -> burn`

|| malloc        | calloc      | realloc          | free           |
| -------------------------- | ------------- | ----------- | ---------------- | -------------- |
| % of total gas spent | 3,62%         | 4,77%       | 1,08%            | 0,77%          |

#### `FT stress-test` with <kbd>gtest</kbd>

| | malloc        | calloc      | realloc          | free            |
| -------------------------- | ------------- | ----------- | ---------------- | --------------- |
| % of total gas spent | 0,40%         | 0,56%       | 0,46%            | 0,40%           |

#### `FT stress-test` with <kbd>gclient</kbd>

| | malloc             | calloc            | realloc           | free                |
| -------------------------- | ------------------ | ----------------- | ----------------- | ------------------- |
| % of total gas spent | 0,73%              | 1,09%             | 2,01%             | 2,85%               |

[Go to top](#)

## Increase static buffer size by two
<a name="increased-twice"></a>
We've tried to increase the static buffer size by two, and its performance was slightly better than the current implementation. But it's not significant enough to change the current implementation.

### Tests data

| Test case | Allocator gas consumption |
| --- | --- |
| `NFT init -> mint -> burn` | 8.30% |
| `FT stress-test` with <kbd>gtest</kbd> | 1.82% |
| `FT stress-test` with <kbd>gclient</kbd> | 6.60% |

#### `NFT init -> mint -> burn`

|  | malloc        | calloc      | realloc          | free           |
| -------------------------- | ------------- | ----------- | ---------------- | -------------- |
| % of total gas spent | 2,82%         | 4,02%       | 0,70%            | 0,76%          |

#### `FT stress-test` with <kbd>gtest</kbd>

| | malloc        | calloc      | realloc          | free            |
| -------------------------- | ------------- | ----------- | ---------------- | --------------- |
| % of total gas spent | 0,42%         | 0,57%       | 0,43%            | 0,39%           |

#### `FT stress-test` with <kbd>gclient</kbd>

| | malloc             | calloc            | realloc           | free                |
| -------------------------- | ------------------ | ----------------- | ----------------- | ------------------- |
| % of total gas spent | 0,82%              | 1,10%             | 1,82%             | 2,86%               |

[Go to top](#)

## Multiple static buffers
<a name="multiple"></a>
We've tried to implement not only one static buffer but multiple static buffers with different sizes. But the study showed that it's not worth the effort, and most allocations were done in the largest static buffer, which performance was even worse than the current implementation.

The best-performed buffer combination was:
- 1-byte buffer with 2 cells
- 2-byte buffer with 2 cells
- 4-byte buffer with 4 cells
- 6-byte buffer with 4 cells
- 8-byte buffer with 8 cells
- 32-byte buffer with 4 cells

### Tests data

| Test case | Allocator gas consumption |
| --- | --- |
| `NFT init -> mint -> burn` | 8.23% |
| `FT stress-test` with <kbd>gtest</kbd> | 1.88% |
| `FT stress-test` with <kbd>gclient</kbd> | 7.01% |

#### `NFT init -> mint -> burn`

| | malloc        | calloc      | realloc          | free           |
| -------------------------- | ------------- | ----------- | ---------------- | -------------- |
| % of total gas spent | 2,47%         | 4,37%       | 0,61%            | 0,78%          |

#### `FT stress-test` with <kbd>gtest</kbd>

| FT stress-test gtest       | malloc        | calloc      | realloc          | free            |
| -------------------------- | ------------- | ----------- | ---------------- | --------------- |
| % of total gas spent | 0,42%         | 0,56%       | 0,49%            | 0,42%           |

#### `FT stress-test` with <kbd>gclient</kbd>

|   | malloc             | calloc            | realloc           | free                |
| -------------------------- | ------------------ | ----------------- | ----------------- | ------------------- |
| % of total gas spent | 0,83%              | 1,08%             | 2,15%             | 2,95%               |

[Go to top](#)