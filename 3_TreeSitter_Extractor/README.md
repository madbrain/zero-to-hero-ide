
# Tree-sitter Extractor

## Online tests

[Playground](https://tree-sitter.github.io/tree-sitter/playground)

## Setup

/!\ Using WASM version of `tree-sitter` because it's simpler to make it work with VSCode (based on electron) even if less fast.

* install dependency

```
npm install web-tree-sitter
```

* Patch web-tree-sitter package !
* Copy parsers binaries

* Initialize parser and query boilerplate

```ts
import * as Parser from 'web-tree-sitter';
import * as path from 'path';

let typescript: Parser.Language;
let html: Parser.Language;
let typescriptParser: Parser;
let htmlParser: Parser;
let findComponentQuery: Parser.Query;
let findInoutQuery: Parser.Query;

export async function initialize(context: vscode.ExtensionContext) {
  const parsersDir = path.join(context.extensionPath, 'parsers');
  async function initParser() {
    await Parser.init();
  }
  async function initTypescript() {
    typescript = await Parser.Language.load(path.relative(process.cwd(), path.join(parsersDir, 'tree-sitter-typescript.wasm')));
    typescriptParser = new Parser();
    typescriptParser.setLanguage(typescript);

    //findComponentQuery = typescript.query(findComponentQueryString);
    //findInoutQuery = typescript.query(inoutQueryString);
  }
  async function initHtml() {
    html = await Parser.Language.load(path.relative(process.cwd(), path.join(parsersDir, 'tree-sitter-html.wasm')));
    
    htmlParser = new Parser();
    htmlParser.setLanguage(html);
  }
  await initParser();
  await initTypescript();
  await initHtml();
} 
```

* Change analysis with real parsing

```ts
function analyzeTS(file: vscode.Uri, content: string): TagDefinition[] {
  const results: TagDefinition[] = [];

  const tree = typescriptParser.parse(content);
  console.log(tree.rootNode.toString());
  
  return results;
}
```

* And then query for components

```ts
const findComponentQueryString = `
(export_statement
  decorator: (decorator
    (call_expression
      function: (identifier) @dec-name
      arguments: (arguments
        (object (pair
          key: (property_identifier) @prop-name
          value: (string (string_fragment) @prop-value))))
    )
  )
  declaration: (class_declaration name: (type_identifier) @class-name) @declaration
  (#eq? @dec-name Component)
  (#eq? @prop-name selector)
)`;

const inoutQueryString = `
(
  (decorator (call_expression function: (identifier) @dec-name))
  .
  [
    (public_field_definition name: (property_identifier) @prop-name)
    (method_definition name: (property_identifier) @prop-name)
  ]
  (#match? @dec-name "Input|Output")
)`;

function toMap(captures: Parser.QueryCapture[]): { [key: string]: Parser.QueryCapture } {
  return captures.reduce((d, x) => { d[x.name] = x; return d }, <{ [key: string]: Parser.QueryCapture }>{});
}

function analyzeTS(file: vscode.Uri, content: string): TagDefinition[] {
  const results: TagDefinition[] = [];

  const tree = typescriptParser.parse(content);

  findComponentQuery.matches(tree.rootNode).forEach(match => {
    const inputs: string[] = [];
    const outputs: string[] = [];
    const classCaptures = toMap(match.captures);
    const selector = classCaptures["prop-value"].node.text;
    const classDeclaration = classCaptures["class-name"].node;
    findInoutQuery.matches(classCaptures["declaration"].node).forEach(m => {
    const inoutCaptures = toMap(m.captures);
    (inoutCaptures["dec-name"].node.text === 'Input' ? inputs : outputs).push(inoutCaptures["prop-name"].node.text)
    });
    console.log(selector, classDeclaration.text, classDeclaration.startPosition.row, inputs, outputs);
    results.push({ tag: selector, uri: file, line: classDeclaration.startPosition.row, inputs, outputs})
  });
  
  return results;
}
```

* Optimize completion by finding context

```ts
interface Span {
  start: number;
  end: number;
}

interface Context {
  tagNamePosition?: Span,
  contextTagNamePosition?: Span,
  attrNamePosition?: Span,
}

function analyzeHtml(content: string, offset: number): Context | null {
  try {
    const tree = htmlParser.parse(content);
    let startTag = findNode(tree.rootNode, offset, [ 'start_tag', 'self_closing_tag' ]);
    if (!startTag) {
      return null;
    }
    const attrName = findNode(startTag, offset, [ 'attribute_name' ]);
    const tagName = findNode(startTag, offset, [ 'tag_name' ]);
    const contextTagName = attrName || !tagName ? startTag.firstNamedChild : null;
    return {
      tagNamePosition: getPosition(tagName),
      contextTagNamePosition: getPosition(contextTagName),
      attrNamePosition: getPosition(attrName),
    };
  } catch (e) {
    console.log("ERROR", e);
    return null;
  }
}

function getPosition(node: Parser.SyntaxNode | null) {
  return node ? { start: node.startIndex, end: node.endIndex } : undefined;
}

function findNode(node: Parser.SyntaxNode, offset: number, types: string[]): Parser.SyntaxNode | null {
  const cursor = node.walk();
  while (true) {
    if (cursor.startIndex <= offset && offset <= cursor.endIndex) {
      if (types.includes(cursor.currentNode().type)) {
        return cursor.currentNode();
      }
      if (! cursor.gotoFirstChild()) {
        return null;
      }
    } else {
      if (! cursor.gotoNextSibling()) {
        return null;
      }
    }
  }
}
```

* Complete on context

```ts
const results: vscode.CompletionItem[] = [];
const analysis = analyzeHtml(document.getText(), document.offsetAt(position));
if (analysis) {
    console.log('COMPLETE', analysis.tagNamePosition, analysis.attrNamePosition, analysis.contextTagNamePosition);
    if (analysis.tagNamePosition) {
        Object.keys(tagDefinitionIndex)
            .forEach(name => {
                results.push({ label: name, kind: vscode.CompletionItemKind.Keyword });
            });
    } else if (analysis.contextTagNamePosition) {
        const tagName = document.getText(new vscode.Range(
            document.positionAt(analysis.contextTagNamePosition.start),
            document.positionAt(analysis.contextTagNamePosition.end)));
        const tagDef = tagDefinitionIndex[tagName];
        if (tagDef) {
            tagDef.inputs.map(name => ({name, isInput: true}))
                .concat(tagDef.outputs.map(name => ({name, isInput: false})))
                .forEach(symbol => {
                    const snippet = symbol.isInput ?
                        new vscode.SnippetString('[').appendText(symbol.name).appendText(']="').appendTabstop().appendText('"')
                        : new vscode.SnippetString('(').appendText(symbol.name).appendText(')="').appendTabstop().appendText('"');
                    results.push({
                        label: symbol.name,
                        kind: symbol.isInput ? vscode.CompletionItemKind.Field : vscode.CompletionItemKind.Event,
                        insertText: snippet,
                    });
                });
        }
    }
}
return Promise.resolve(results);
```