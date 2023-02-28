
# Zero to Hero IDE

The objective is to be able to navigate from html files to Angular component and eventually get code completion
integrated into our preferred IDE.

As an example application use [Angular RealWorld App](https://github.com/gothinkster/angular-realworld-example-app).

## Tags file made with naive grep

```sh
grep -roPn --include="*.ts" "selector:[ ]*'\K[-a-zA-Z]*(?='.*)" \
        | awk -F: '{ print $3 "\t" $1 "\t" $2 }' \
        | sort > tags
```

Vi doesn't not use `-` character in keyword, to had it:
```vim
set isk+=-
```

Navigate to tag with `Ctrl-]` and back with `Ctrl-T`.

## VSCode extension

- parse tags file at root of workspace
- use `Go To Definition` provider
- then implement indexation and don't read tags file anymore

Limitations:
- no dynamic update of index file

## Dynamically index TS files 

Limitations:
- naive regexp not robust enough
- only selector, no other information extracted

## Use of tree-sitter for information extraction

- tree-sitter-typescript and Query to index component information

## Convert to LSP

- convert to Language Server (good idea for a rust project?)
- could be integrated back into *Vim
