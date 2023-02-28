
# Convert extension to LSP Server

## What is LSP

[Language Server Protocol](https://microsoft.github.io/language-server-protocol/)

Example of JSon-RPC communication:
```sh
echo -e 'Content-Length: 81\r\n\r\n{"jsonrpc":"2.0", "id":1, "method": "initialize", "params":{ "capabilities": {}}}' ./target/debug/angular-lsp
```

## The extension client

The extension only responsibility is now to find and start the LSP server.

## The server

The Rust server is mainly inspired (or let's say copied) from :

https://github.com/IWANABETHATGUY/tower-lsp-boilerplate/blob/main/src/main.rs

but works exactly the same way as the previous extension.