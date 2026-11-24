# bb api

```text
bb api [OPTIONS] <PATH>
```

Make an authenticated API request. `PATH` is relative to `/2.0`, or a full URL.

| Flag | Meaning |
|---|---|
| `-X, --method <METHOD>` | HTTP method (default GET, or POST when a body or fields are given) |
| `-f, --raw-field <KEY=VALUE>` | Add a string field (repeatable). Fields form a JSON body, or the query string for GET or with `--input` |
| `-F, --field <KEY=VALUE>` | Add a typed field (repeatable): `true`/`false`/`null`/integers become JSON values; `@FILE` or `@-` reads the value |
| `-H, --header <KEY:VALUE>` | Add a request header (repeatable) |
| `--input <FILE>` | Raw JSON body from FILE, or `-` for stdin |
| `-i, --include` | Print the response status line and headers before the body |
| `--silent` | Do not print the response body |
| `--verbose` | Print the request and response headers to stderr (the Authorization header is never shown) |
| `--paginate` | Follow `next` links and print the concatenated `values` array (GET only) |

Output is the response body, pretty-printed if it is JSON. On a non-2xx status the body is still
printed, then `HTTP <code>: <message>` goes to stderr and the exit code is 1.

```sh
bb api /user
bb api /repositories/myteam/api/pullrequests --paginate | jq '.[].title'
echo '{"content":{"raw":"hi"}}' | bb api /repositories/myteam/api/pullrequests/7/comments --input -
```

## Differences from `gh`

- *Deliberate:* `--paginate` prints one combined array of every page's `values`, not each page in
  turn, so `jq '.[]'` works as with `gh`. `--slurp` is not accepted.
- *Gap:* `--paginate` can't be combined with `-H`, `-i` or `--verbose`.
- *Gap:* no nested field syntax (`key[sub]=v`, `key[]=v`) and no `{owner}`/`{repo}`/`{branch}`
  placeholders; use `--input` for nested bodies. `--input` must be JSON.
- *Gap:* `-i`/`--verbose` show HTTP/1.1 and lowercased header names (as received).
- `--jq`, `--template`, `--cache`, `-p/--preview` and `--hostname` are out of scope.
