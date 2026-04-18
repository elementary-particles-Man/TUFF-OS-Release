# KAIRO-ACL EBNF Specification (Draft v1)

`KAIRO-ACL.txt` の厳密パーサ仕様。

## 1. Evaluation Model

- 優先順位: `KILL > BYPASS > ALLOW > DEFAULT`
- `DEFAULT` は暗黙で `DENY`
- `KILL` 一致時は `drop + force_off + extended_log`
- `BYPASS` 一致時は FD を fast-path 除外

## 2. Lexical Rules

- 文字コード: UTF-8（実質 ASCII 運用推奨）
- 行単位パース
- 空行は無視
- コメント: `#` 以降は行末まで無視
- キーワードは大文字小文字を区別しない（内部で大文字化して比較）
- FQDN は小文字正規化

## 3. EBNF

```ebnf
file            = { line } ;
line            = ws , [ statement ] , [ ws ] , [ comment ] , newline ;
comment         = "#" , { any_char_except_newline } ;

statement       = action , ws , target_type , ws , target_value , { ws , condition } ;

action          = "ALLOW" | "KILL" | "BYPASS" ;

target_type      = "HOST" | "CIDR" | "PORT" | "PROTO" | "METHOD" ;

target_value     = host
                 | cidr
                 | port
                 | proto
                 | method_list ;

condition        = cond_host
                 | cond_dst
                 | cond_proto
                 | cond_size ;

cond_host        = "HOST" , ws , ( host | "UNKNOWN" | "*" ) ;
cond_dst         = "DST" , ws , ( "LAN" | "!LAN" ) ;
cond_proto       = "PROTO" , ws , proto ;
cond_size        = "SIZE" , ws , comparator , ws , size_literal ;

comparator       = ">" | ">=" | "<" | "<=" | "=" ;

method_list      = method , { "," , method } ;
method           = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" ;

proto            = "HTTP" | "HTTPS" | "DNS" | "NTP" | "SMTP" | "SMB" | token ;

host             = "*" | fqdn | ipv4 ;
fqdn             = label , { "." , label } ;
label            = alnum , { alnum | "-" } ;

cidr             = ipv4 , "/" , digits ;
ipv4             = dec_octet , "." , dec_octet , "." , dec_octet , "." , dec_octet ;
dec_octet        = digits ;   (* semantic check: 0..255 *)

port             = digits ;   (* semantic check: 1..65535 *)

size_literal     = digits , size_unit ;
size_unit        = "B" | "KB" | "MB" ;

token            = alpha , { alpha | digit | "_" | "-" } ;

ws               = { " " | "\t" } ;
newline          = "\n" | "\r\n" ;

alnum            = alpha | digit ;
alpha            = "A".."Z" | "a".."z" ;
digit            = "0".."9" ;
digits           = digit , { digit } ;
```

## 4. Semantic Constraints

- `BYPASS` に `METHOD` は使用不可
- `CIDR` は IPv4 のみ（v1）
- `HOST *` は `ALLOW` では禁止、`KILL`/`BYPASS` のみ許可
- `METHOD` を target にした場合は `HOST` または `DST` 条件のいずれか必須
- `SIZE` 条件は `METHOD` が `PUT` または `POST` を含む場合のみ有効
- 同一正規化キーに `ALLOW` と `KILL` が競合する場合、警告を出して `KILL` を採用

## 5. Normalization

- FQDN: 小文字化、末尾 `.` を除去
- `PROTO`: 大文字化
- `METHOD`: 大文字化
- 空白: 連続空白を単一空白扱い
- `SIZE`: bytes に正規化して内部保持

## 6. Parse Failure Policy

- 構文エラーが 1 件でもあれば反映失敗
- 失敗時は「現在有効な ACL セット」を維持（fail-safe）
