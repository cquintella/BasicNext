// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

const assert = require("assert");
const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "../../..");
const library = JSON.parse(
  fs.readFileSync(path.join(root, "docs/library/basicnext.tmLanguage.json"), "utf8"),
);
const bundled = JSON.parse(
  fs.readFileSync(path.join(root, "plugins/vscode/syntaxes/basicnext.tmLanguage.json"), "utf8"),
);
assert.deepStrictEqual(library, bundled, "TextMate copies must be byte-equivalent JSON");

function walk(node, visit) {
  if (!node || typeof node !== "object") {
    return;
  }
  visit(node);
  for (const value of Object.values(node)) {
    if (Array.isArray(value)) {
      value.forEach((item) => walk(item, visit));
    } else {
      walk(value, visit);
    }
  }
}

function wordTerminals(grammar) {
  const words = new Set();
  walk(grammar, (node) => {
    if (typeof node.match !== "string") {
      return;
    }
    assert.doesNotMatch(node.match, /-INF/, "-INF must not be a lexical terminal");
    const grouped = node.match.match(/\\b\(([^)]+)\)\\b/);
    if (!grouped) {
      return;
    }
    for (const part of grouped[1].split("|")) {
      if (/^[A-Z][A-Z0-9]*$/.test(part)) {
        words.add(part);
      }
    }
  });
  return words;
}

function matchNamed(grammar, name) {
  let pattern;
  walk(grammar, (node) => {
    if (node.name === name && typeof node.match === "string") {
      pattern = node.match;
    }
  });
  assert.ok(pattern, `missing TextMate pattern ${name}`);
  return new RegExp(`^${pattern}$`);
}

const words = wordTerminals(library);
for (const required of ["PARALLEL", "SYSTEM", "NAN", "INF", "IF"]) {
  assert.ok(words.has(required), `${required} must be a TextMate word terminal`);
}
assert.ok(!words.has("-INF"));

const decimal = matchNamed(library, "constant.numeric.decimal.bn");
assert.ok(decimal.test("42"));
assert.ok(decimal.test("3.14"));
assert.ok(!decimal.test("1e3"));
assert.ok(!decimal.test("1."));
assert.ok(!decimal.test(".5"));

const binary = matchNamed(library, "constant.numeric.binary.bn");
assert.ok(binary.test("0b10"));
assert.ok(!binary.test("0b2"));

const hex = matchNamed(library, "constant.numeric.hex.bn");
assert.ok(hex.test("0x0F"));
assert.ok(!hex.test("0xGG"));

const escape = matchNamed(library, "constant.character.escape.bn");
assert.ok(escape.test("\\\""));
assert.ok(escape.test("\\\\"));
assert.ok(!escape.test("\\x"));

const control = matchNamed(library, "keyword.control.bn");
assert.ok(control.test("IF"));
assert.ok(!control.test("if"));

const lineComment = matchNamed(library, "comment.line.double-slash.bn");
assert.ok(lineComment.test("// comment"));

assert.ok(!control.test("1e3"));
console.log("Basic Next TextMate grammar checks passed");
