# Simple CLI for LockSwap

search first 10 items of avaliable sUDT cells:

```bash
$ cargo run -- search_sudt
```

search first 10 items of avaliable swap orders:

```bash
$ cargo run -- search_lockswap
```

make sUDT sell order:

```bash
$ cargo run -- make_order --sudt <sudt_offset_in_search_list> --ckb <CKB order>
```

take sUDT sell order:

```bash
$ cargo run -- take_order --lockswap <lockswap_offset_in_search_list>
```
