
# VS Code Extension

## Scaffolding

```sh
npm install -g yo generator-code
yo code
```

Answer the questions:
* Type: TypeScript
* Name: angular-html-tags
* Identifier: (default)
* Description: (default)
* Git Repo: N
* Webpack: (default)
* Package Manager: (default)

## Add Goto Definition on HTML files

### Configure meta data in `package.json`

Remove the example `onCommand` and add `onLanguage`:

```json
  "activationEvents": [
    "onLanguage:html"
  ],
```

### Add a goto definition provider in `extension.ts`

```ts
interface TagDefinition {
  selector: string,
  uri: vscode.Uri,
  line: number
}

let tagDefinitionIndex: Record<string, TagDefinition> = {};

export function activate(context: vscode.ExtensionContext) {
  const rootFolderUri = vscode.workspace.workspaceFolders?.[0]?.uri ?? vscode.Uri.parse("file:/");
	
	// mock definitions
  tagDefinitionIndex["app-example-tag"] = { selector: "app-example-tag", uri: vscode.Uri.joinPath(rootFolderUri, "README.md"), line: 1 };
	tagDefinitionIndex["app-other-tag"] = { selector: "app-other-tag", uri: vscode.Uri.joinPath(rootFolderUri, "README.md"), line: 10 };

	context.subscriptions.push(vscode.languages.registerDefinitionProvider('html', {
		provideDefinition(document: vscode.TextDocument, position: vscode.Position, token: vscode.CancellationToken): vscode.DefinitionLink[] {
		  const wordRange = document.getWordRangeAtPosition(position);
		  const word = document.getText(wordRange);
      const results: vscode.DefinitionLink[] = [];
      const tagDef = tagDefinitionIndex[word];
      if (tagDef) {
        results.push({
          targetUri: tagDef.uri,
          targetRange: new vscode.Range(
              new vscode.Position(tagDef.line, 0),
              new vscode.Position(tagDef.line + 1, 0)
          ),
          originSelectionRange: wordRange
        });
      }
      return results;
    }
	}));
}
```

### Read the tags file

```ts
const TAGS_FILE_NAME = "tags";

function loadTags(rootFolder: vscode.Uri) {
	const tagsPath = vscode.Uri.joinPath(rootFolder, TAGS_FILE_NAME);
	vscode.workspace.fs.readFile(tagsPath).then(data => {
		tagDefinitionIndex = {};
		Buffer.from(data).toString('utf8').split("\n").forEach(line => {
			const elements = line.split(/\t+/);
			if (elements.length === 3) {
				tagDefinitionIndex[elements[0]] = {
					selector: elements[0],
					uri: vscode.Uri.joinPath(rootFolder, elements[1]),
					line: parseInt(elements[2]) - 1
				};
			}
		});
	});
}
```

And load on extension activation :
```ts
const rootFolderUri = vscode.workspace.workspaceFolders?.[0]?.uri ?? vscode.Uri.parse("file:/");

loadTags(rootFolderUri);
	
context.subscriptions.push(...);
```

## Extension creates index itself

First scan the workspace for TS source file:
```ts
function scanWorkspace(rootFolder: vscode.Uri) {
	tagDefinitionIndex = {};
	vscode.workspace.findFiles(
		new vscode.RelativePattern(rootFolder, '**/*.ts'),
		new vscode.RelativePattern(rootFolder, 'node_modules/**')).then(files => {
			files.forEach(file => {
        console.log(file);
      });
		});
}
```

And call on extension activation:
```ts
const rootFolderUri = vscode.workspace.workspaceFolders?.[0]?.uri;

if (rootFolderUri) {
  scanWorkspace(rootFolderUri);
}
	
context.subscriptions.push(...);
```

And then read and analyze each file:
```ts
files.forEach(file => {
  scanFile(file).then(definitions => {
    definitions.forEach(tagDef => {
      tagDefinitionIndex[tagDef.selector] = tagDef;
    });
  });
});
```

```ts
async function scanFile(file: vscode.Uri): Promise<TagDefinition[]> {
	const data = await vscode.workspace.fs.readFile(file);
	const content = Buffer.from(data).toString('utf8');
  return analyzeTS(file, content);
}

function analyzeTS(file: vscode.Uri, content: string): TagDefinition[] {
  const results: TagDefinition[] = [];

  const re = /selector:[ ]*'([-a-zA-Z]+)'/;
  content.split("\n").forEach((line, num) => {
    let match = re.exec(line);
    if (match) {
      results.push({
        selector: match[1],
        uri: file,
        line: num
      });
    }
  });
  
  return results;
}
```

## Watch file change

```ts
if (rootFolderUri) {
  let watcher = vscode.workspace.createFileSystemWatcher(new vscode.RelativePattern(rootFolderUri, 'src/**/*.ts'));
  watcher.onDidChange(file => scanFile(file).then(definitions => {
    definitions.forEach(tagDef => {
      tagDefinitionIndex[tagDef.selector] = tagDef;
    });
  }));
  scanWorkspace(rootFolderUri);
}
```

Exercise: clean old entries for changed file

## Provide completion on tag names

```ts
context.subscriptions.push(vscode.languages.registerCompletionItemProvider('html', {
  provideCompletionItems(document: vscode.TextDocument, position: vscode.Position,
      token: vscode.CancellationToken, context: vscode.CompletionContext): vscode.ProviderResult<vscode.CompletionItem[]> {
    
    const results: vscode.CompletionItem[] = [];

    Object.keys(tagDefinitionIndex).forEach(name => {
      results.push({ label: name, kind: vscode.CompletionItemKind.Keyword });
    });

    return Promise.resolve(results);
  }
}));
```

## Limitations

* indexation is not robust enough, it can capture false positive:
```ts
let my_example = {
  selector: 'example',
  label: 'Example',
};
```

* completions don't use any context information (new tag or attribute)