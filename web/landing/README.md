# Hinemos Landing

Yew frontend for the Hinemos HTTP MVP. Trunk is a local development and build tool only; do not run `trunk serve` in production.

## Development

Run the Rust backend from the repository root:

```sh
cargo run -p hinemos-cli -- serve http --bind 127.0.0.1:8080
```

Run the Yew app from this directory for local development only, proxying `/api` requests to the backend:

```sh
trunk serve --address 127.0.0.1 --port 13000 --proxy-backend=http://127.0.0.1:8080/api/
```

## Integrated build

Build the production frontend from this directory. The production build calls the API through `https://api.hinemos.ai` by default:

```sh
trunk build --release
```

Override the API host at build time when needed:

```sh
HINEMOS_API_BASE_URL=https://api.hinemos.ai trunk build --release
```

Then run the backend from the repository root. By default it serves `web/landing/dist` and keeps API endpoints under `/api`:

```sh
cargo run -p hinemos-cli -- serve http --bind 127.0.0.1:8080
```
