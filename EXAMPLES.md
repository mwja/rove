# Examples

## Contracts: `require!` and `ensure!`

```swift
func withdraw(wallet: int, amount: int) -> int
    require! enough_money: wallet >= amount
    ensure! deducted: ret == wallet - amount
{
    return wallet - amount;
}

func inc(x: int) -> int
    require! : x >= 0
    ensure! grew: ret > old(x)
{
    x + 1
}
```

## Guards and errors

```swift
enum PurchaseError {
    NoUnits,
    InsufficientFunds
}

func purchase(cost: int, units: int, wallet: int) throws PurchaseError -> int
    guard units > 0 else ::NoUnits
    guard wallet >= cost * units else ::InsufficientFunds
{
    return wallet - cost * units;
}

func main() {
    print try! purchase(10, 3, 100);

    let left = try purchase(10, 30, 100) catch |err| {
        0
    };
    print left;
}
```

## Guards in a function body

```swift
func clamp_positive(n: int) -> int {
    guard n > 0 else {
        return 0;
    }
    n
}

func sum_skipping(skip: int) -> int {
    let total = 0;
    let i = 0;
    while i < 10 {
        i = i + 1;
        guard i != skip else {
            continue;
        }
        total = total + i;
    }
    require! positive: total > 0;
    total
}
```
