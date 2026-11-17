"""Regenerate native signatures from the checked-in phpredis reflection inventory."""
import json
from pathlib import Path
root = Path(__file__).resolve().parent.parent
inventory = json.loads((root / 'tools/phpredis-6.3.0.json').read_text())
methods = list(inventory['methods'].items())
types = {'int': 'Int', 'bool': 'Bool', 'string': 'String', 'float': 'Float',
         'array': 'Array', 'mixed': 'Mixed', 'callable': 'Callable',
         'array|string': 'ArrayOrString', 'string|int': 'IntOrString',
         'int|float': 'Number', 'string|int|float': 'StringOrNumber',
         'array|int|float': 'ArrayOrNumber', 'array|float': 'ArrayOrFloat',
         'array|bool': 'ArrayOrBool'}
groups = []
for start in range(0, len(methods), 10):
    group = f'group_{start // 10:02}'
    groups.append(group)
    text = '//! Reflected PHP signatures; e.g. `$redis->get(key: "name")`.\n'
    text += 'use phera_extension_api::{MethodEntry, Param, ParamType};\n'
    text += 'use crate::dispatch::dispatch;\n'
    for i, (name, params) in enumerate(methods[start:start+10], start):
        text += f'pub const M{i}: MethodEntry = MethodEntry::public_async("{name}", &[\n'
        for p in params:
            ty = p['type'].replace('?', '').replace('|null', '')
            text += f'    Param::new("{p["name"]}", ParamType::{types.get(ty, "Mixed")})'
            if '?' in p['type'] or 'null' in p['type']: text += '.nullable()'
            if p['optional']: text += '.optional()'
            if p['ref']: text += '.by_ref()'
            if p['variadic']: text += '.variadic()'
            text += ',\n'
        text += f'], |c, o, a| dispatch("{name.lower()}", c, o, a))\n'
        text += f'.with_preparation(|c,p,a| crate::arguments::prepare("Redis::{name}",c,p,a));\n' 
    (root/f'src/registration/{group}.rs').write_text(text)
text = '//! `Redis` and `RedisException` only; cluster, sentinel and array are separate APIs.\n'
text += 'use phera_extension_api::{ClassEntry, ConstantValue, Registry, MethodEntry};\n'
text += '\n'.join(f'mod {g};' for g in groups) + '\n'
text += 'pub fn register(registry: &mut Registry) {\n'
text += 'registry.class(ClassEntry { parent: Some("RuntimeException"),\n'
text += '..ClassEntry::class("RedisException") });\n'
text += 'registry.class(ClassEntry { methods: METHODS, constants: CONSTANTS,\n'
text += '..ClassEntry::class("Redis") });\n}\n'
registration_header = text.replace('ConstantValue, ', '').replace(', MethodEntry', '')
text = '//! Complete reflected method table for `Redis`.\nuse phera_extension_api::MethodEntry;\nuse super::*;\npub const METHODS: &[MethodEntry] = &[\n'
# two entries per line to keep the top module small
for i in range(0, len(methods), 2):
    text += '    ' + ' '.join(f'group_{j//10:02}::M{j},' for j in range(i,min(i+2,len(methods))))+'\n'
text += '];\n'
(root/'src/registration/methods.rs').write_text(text)
text = '//! Constants reflected from phpredis 6.3.0.\nuse phera_extension_api::ConstantValue;\npub const CONSTANTS: &[(&str, ConstantValue)] = &[\n'
for k,v in inventory['constants'].items():
    text += f'("{k}", ConstantValue::'+(f'String({json.dumps(v)})' if isinstance(v,str) else f'Int({v})')+'),\n'
text += '];\n'
(root/'src/registration/constants.rs').write_text(text)
(root/'src/registration.rs').write_text(registration_header + '\nmod methods;\nmod constants;\nuse methods::METHODS;\nuse constants::CONSTANTS;\n')
