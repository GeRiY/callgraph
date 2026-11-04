#!/usr/bin/env python3
"""Build the small, deliberately explicit R4a document golden files."""
import json
import os
import re
import sys
from pathlib import Path

ROOT = Path(os.environ['CALLGRAPH_DOC_ROOT'])  # tools/local.env

# a forrásfájlok listája (a CALLGRAPH_DOC_ROOT-hoz relatív utak, vesszővel): tools/local.env
JSON_FILES = [f for f in os.environ.get('CALLGRAPH_DOC_JSON', '').split(',') if f]
MD_FILES = [f for f in os.environ.get('CALLGRAPH_DOC_MD', '').split(',') if f]

def line_at(text, pos):
    return text.count('\n', 0, pos) + 1

def json_constants(text, value, path='', pos=0, container='document'):
    out = []
    if isinstance(value, dict):
        cursor = pos
        for key, child in value.items():
            needle = json.dumps(key, ensure_ascii=False)
            found = text.find(needle, cursor)
            if found < 0:
                found = cursor
            value_pos = text.find(':', found + len(needle)) + 1
            while value_pos < len(text) and text[value_pos].isspace():
                value_pos += 1
            child_path = f'{path}.{key}' if path else key
            typ = ('object' if isinstance(child, dict) else
                   'array' if isinstance(child, list) else
                   'string' if isinstance(child, str) else
                   'boolean' if isinstance(child, bool) else
                   'null' if child is None else 'number')
            out.append({'container': container, 'kind': 'key', 'name': child_path,
                        'line': line_at(text, found), 'value': typ,
                        'visibility': 'public', 'doc': None})
            out.extend(json_constants(text, child, child_path, value_pos, container))
            cursor = value_pos
    elif isinstance(value, list):
        for child in value:
            out.extend(json_constants(text, child, path, pos, container))
    return out

def json_imports(value):
    imports = []
    def walk(v, path=''):
        if isinstance(v, dict):
            for key, child in v.items():
                if key in ('$ref', 'extends') and isinstance(child, str):
                    imports.append((key, child))
                if path in ('dependencies', 'devDependencies', 'require', 'require-dev') and isinstance(child, (str, int, float)):
                    imports.append((key, str(child)))
                walk(child, f'{path}.{key}' if path else key)
        elif isinstance(v, list):
            for child in v:
                walk(child, path)
    walk(value)
    return dict(imports)

def json_graph(rel):
    path = ROOT / rel
    text = path.read_text(encoding='utf-8')
    value = json.loads(text)
    name = path.name
    graph = {'file': str(path), 'framework': None, 'namespace': None,
             'imports': json_imports(value),
             'containers': [{'type': 'document', 'name': name, 'line': 1,
                             'endLine': len(text.splitlines()), 'abstract': False,
                             'final': False, 'extends': [], 'implements': [],
                             'traits': [], 'doc': None}],
             'properties': [], 'constants': json_constants(text, value, container=name),
             'methods': [], 'calls': [], 'comments': [], 'controlFlow': [],
             'strings': [], 'writes': []}
    if not graph['imports']:
        graph['imports'] = []
    return {'path': str(path), 'graph': graph}

def md_graph(rel):
    path = ROOT / rel
    text = path.read_text(encoding='utf-8')
    lines = text.splitlines()
    name = path.name
    headings = []
    in_code = False
    for i, line in enumerate(lines, 1):
        if re.match(r'^```', line):
            in_code = not in_code
            continue
        if in_code:
            continue
        m = re.match(r'^(#{1,6})[ \t]+(.+?)\s*#*$', line)
        if m:
            headings.append((i, len(m.group(1)), m.group(2).strip()))
    methods = []
    for idx, (start, level, title) in enumerate(headings):
        end = len(lines)
        for next_start, next_level, _ in headings[idx + 1:]:
            if next_level <= level:
                end = next_start - 1
                break
        methods.append({'container': name, 'name': title, 'line': start,
                        'endLine': end, 'visibility': f'h{level}', 'static': False,
                        'abstract': False, 'params': [], 'returnType': None,
                        'doc': None, 'body': None})
    calls = []
    in_code = False
    for i, line in enumerate(lines, 1):
        if re.match(r'^```', line):
            in_code = not in_code
            continue
        if in_code:
            continue
        caller = '__top__'
        for heading_line, _, heading_title in headings:
            if heading_line <= i:
                caller = heading_title
            else:
                break
        for m in re.finditer(r'!\[([^]]*)\]\(([^)]+)\)|(?<!!)\[([^]]+)\]\(([^)]+)\)', line):
            image = m.group(1) is not None
            target = m.group(2) if image else m.group(4)
            if target.startswith('#'):
                category = 'anchor'
            elif re.match(r'^[a-z][a-z0-9+.-]*://', target, re.I):
                category = 'external_url'
            else:
                category = 'internal_file'
            calls.append({'container': name, 'caller': caller, 'line': i,
                          'type': 'image' if image else 'link', 'target': target,
                          'category': 'image' if image else category})
    strings = []
    in_block = None
    for i, line in enumerate(lines, 1):
        fence = re.match(r'^```\s*(.*)$', line)
        if fence and in_block is None:
            in_block = (i, fence.group(1).strip() or 'plaintext')
        elif fence and in_block is not None:
            start, lang = in_block
            strings.append({'container': name, 'caller': '__top__', 'line': start,
                            'endLine': i,
                            'value': f'code:{lang}'})
            in_block = None
    graph = {'file': str(path), 'framework': None, 'namespace': None, 'imports': [],
             'containers': [{'type': 'document', 'name': name, 'line': 1,
                             'endLine': len(lines), 'abstract': False, 'final': False,
                             'extends': [], 'implements': [], 'traits': [], 'doc': None}],
             'properties': [], 'constants': [], 'methods': methods, 'calls': calls,
             'comments': [], 'controlFlow': [], 'strings': strings, 'writes': []}
    return {'path': str(path), 'graph': graph}

def main():
    out = Path(__file__).parent
    (out / 'corpus-json.txt').write_text('\n'.join(str(ROOT / p) for p in JSON_FILES) + '\n', encoding='utf-8')
    (out / 'corpus-md.txt').write_text('\n'.join(str(ROOT / p) for p in MD_FILES) + '\n', encoding='utf-8')
    for filename, rows in [('ref-json.jsonl', (json_graph(p) for p in JSON_FILES)),
                           ('ref-md.jsonl', (md_graph(p) for p in MD_FILES))]:
        with (out / filename).open('w', encoding='utf-8') as f:
            for row in rows:
                f.write(json.dumps(row, ensure_ascii=False, separators=(',', ':')) + '\n')

if __name__ == '__main__':
    main()
