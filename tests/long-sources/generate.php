<?php
// Generates very long and very deeply nested PHP sources.
//
// Machine-generated PHP (compiled containers, templates, data tables) holds
// expressions and lists far larger than anything written by hand. Each shape
// below builds one such source at a given size; every source prints a short
// result, so the output of Phera and of real PHP can be compared exactly.
//
//     php -n tests/long-sources/generate.php --list           # one shape per line
//     php -n tests/long-sources/generate.php add 100000 > a.php
//
// `crates/cli/tests/long_sources.rs` runs every shape through both engines at
// small sizes; `tests/long-sources/table.sh` runs all sizes and prints the
// table of what PHP and Phera do.

function chain(string $first, string $glue, string $term, int $n): string
{
    return $first . str_repeat($glue . $term, $n);
}

function numbered(string $pattern, int $n, string $glue): string
{
    $parts = [];
    for ($i = 0; $i < $n; $i++) {
        $parts[] = str_replace('#', (string) $i, $pattern);
    }
    return implode($glue, $parts);
}

$shapes = [
    // Left-associative chains, one per operator family.
    'add' => fn ($n) => '$x = ' . chain('1', ' + ', '1', $n) . '; echo $x;',
    'add-vars' => fn ($n) => '$a = 1; $x = ' . chain('$a', ' + ', '$a', $n) . '; echo $x;',
    'subtract' => fn ($n) => '$a = 1; $x = ' . chain('$a', ' - ', '$a', $n) . '; echo $x;',
    'mixed-arithmetic' => fn ($n) => '$a = 2; $x = ' . chain('$a', ' + ', '$a * $a - $a', $n) . '; echo $x;',
    'concat' => fn ($n) => '$x = ' . chain("'a'", ' . ', "'a'", $n) . '; echo strlen($x);',
    'concat-vars' => fn ($n) => '$a = "a"; $x = ' . chain('$a', ' . ', '$a', $n) . '; echo strlen($x);',
    'logical-and' => fn ($n) => '$t = true; var_dump(' . chain('$t', ' && ', '$t', $n) . ');',
    'logical-or' => fn ($n) => '$f = false; var_dump(' . chain('$f', ' || ', '$f', $n) . ');',
    'keyword-or' => fn ($n) => '$f = false; $x = (' . chain('$f', ' or ', '$f', $n) . '); var_dump($x);',
    'bitwise-or' => fn ($n) => '$a = 1; echo ' . chain('$a', ' | ', '$a', $n) . ';',
    'bitwise-xor' => fn ($n) => '$a = 1; echo ' . chain('$a', ' ^ ', '$a', $n) . ';',
    'comparisons' => fn ($n) => '$a = 1; var_dump(' . chain('$a == 1', ' && ', '$a < 2', $n) . ');',
    'pipe' => fn ($n) => 'echo ' . chain('1', ' |> ', 'abs(...)', $n) . ';',
    'short-ternary' => fn ($n) => '$z = 0; echo ' . chain('$z', ' ?: ', '$z', $n) . ' ?: 7;',
    'instanceof' => fn ($n) => '$o = new stdClass; var_dump(' . chain('$o instanceof stdClass', ' && ', '$o instanceof stdClass', $n) . ');',

    // Right-associative chains.
    'coalesce' => fn ($n) => '$u = null; echo ' . chain('$u', ' ?? ', '$u', $n) . ' ?? 5;',
    'power' => fn ($n) => 'echo ' . chain('1', ' ** ', '1', $n) . ';',
    'assign-chain' => fn ($n) => '$a = ' . str_repeat('$a = ', $n) . '3; echo $a;',
    'concat-assign-chain' => fn ($n) => '$a = ""; $a .= ' . str_repeat('$b = ', $n) . '"x"; echo $a, $b;',
    'ternary-middle' => fn ($n) => '$t = 1; echo ' . str_repeat('$t ? ', $n) . '1' . str_repeat(' : 0', $n) . ';',
    'ternary-parenthesised' => fn ($n) => '$f = 0; echo ' . str_repeat('$f ? 0 : (', $n) . '9' . str_repeat(')', $n) . ';',
    'right-nested-add' => fn ($n) => '$a = 1; echo ' . str_repeat('$a + (', $n) . '1' . str_repeat(')', $n) . ';',

    // Prefix runs.
    'not-run' => fn ($n) => '$t = true; var_dump(' . str_repeat('!', $n) . '$t);',
    'minus-run' => fn ($n) => 'echo ' . str_repeat('- ', $n) . '1;',
    'bitwise-not-run' => fn ($n) => 'echo ' . str_repeat('~', $n) . '1;',
    'cast-run' => fn ($n) => '$a = "7"; var_dump(' . str_repeat('(int)', $n) . '$a);',
    'silence-run' => fn ($n) => '$a = 1; echo ' . str_repeat('@', $n) . '$a;',
    'parentheses' => fn ($n) => 'echo ' . str_repeat('(', $n) . '1' . str_repeat(')', $n) . ';',
    'clone-run' => fn ($n) => '$o = new stdClass; $o->v = 4; echo (' . str_repeat('clone ', $n) . '$o)->v;',

    // Arrays.
    'array-nested' => fn ($n) => '$x = ' . str_repeat('[', $n) . '1' . str_repeat(']', $n) . '; echo count($x);',
    'array-wide' => fn ($n) => '$x = [' . numbered('#', $n, ', ') . ']; echo count($x), " ", array_sum($x);',
    'array-wide-keyed' => fn ($n) => '$x = [' . numbered("'k#' => #", $n, ', ') . ']; echo count($x), " ", $x["k" . (count($x) - 1)];',
    'array-wide-spread' => fn ($n) => '$a = [1, 2]; $x = [' . chain('...$a', ', ', '...$a', $n - 1) . ']; echo count($x);',
    'array-wide-nested' => fn ($n) => '$x = [' . numbered("[#, ['#']]", $n, ', ') . ']; echo count($x), " ", $x[count($x) - 1][1][0];',
    'list-wide' => fn ($n) => '[' . numbered('$v#', $n, ', ') . '] = range(1, ' . $n . '); echo $v0 + ${"v" . (' . $n . ' - 1)};',
    'list-nested-wide' => fn ($n) => '[' . numbered('[$a#, $b#]', $n, ', ') . '] = array_fill(0, ' . $n . ', [1, 2]); echo $a0 + $b0;',
    'list-nested-deep' => fn ($n) => str_repeat('[', $n) . '$v' . str_repeat(']', $n) . ' = ' . str_repeat('[', $n) . '6' . str_repeat(']', $n) . '; echo $v;',

    // Calls and member chains.
    'arguments-wide' => fn ($n) => 'function f(...$a) { return count($a); } echo f(' . chain('1', ', ', '1', $n - 1) . ');',
    'calls-nested' => fn ($n) => 'echo ' . str_repeat('abs(', $n) . '-1' . str_repeat(')', $n) . ';',
    'method-chain' => fn ($n) => 'class C { public $n = 0; function m() { $this->n++; return $this; } } $o = new C; echo $o' . str_repeat('->m()', $n) . '->n;',
    'nullsafe-chain' => fn ($n) => 'class C { public $n = 0; function m() { $this->n++; return $this; } } $o = new C; echo $o' . str_repeat('?->m()', $n) . '->n;',
    'property-chain' => fn ($n) => '$o = new stdClass; $o->p = $o; $o->v = 8; echo $o' . str_repeat('->p', $n) . '->v;',
    'static-chain' => fn ($n) => 'class C { static function m() { return "C"; } } echo C' . str_repeat('::m()', $n) . ';',
    'subscript-chain' => fn ($n) => '$s = "x"; echo $s' . str_repeat('[0]', $n) . ';',
    'call-chain' => fn ($n) => '$f = function () use (&$f) { return $f; }; var_dump($f' . str_repeat('()', $n) . ' instanceof Closure);',
    'new-nested' => fn ($n) => 'class N { function __construct(public $c = null) {} } $x = ' . str_repeat('new N(', $n) . str_repeat(')', $n) . '; echo get_class($x);',
    'new-chain' => fn ($n) => 'class N { function m() { return $this; } } echo get_class(' . str_repeat('(', $n) . 'new N' . str_repeat(')->m()', $n) . ');',

    // Strings.
    'interpolation-wide' => fn ($n) => '$a = "ab"; $x = "' . str_repeat('$a-{$a}', $n) . '"; echo strlen($x);',
    'heredoc-wide' => fn ($n) => "\$a = 'ab'; \$x = <<<EOT\n" . str_repeat('$a {$a}', $n) . "\nEOT;\necho strlen(\$x);",

    // Control flow.
    'match-arms' => fn ($n) => '$v = ' . ($n - 1) . '; echo match ($v) { ' . numbered('# => #', $n, ', ') . ' };',
    'switch-cases' => fn ($n) => '$v = ' . ($n - 1) . '; switch ($v) { ' . numbered('case #: echo #; break;', $n, ' ') . ' }',
    'elseif-ladder' => fn ($n) => '$v = ' . ($n - 1) . '; if ($v === -1) {} ' . numbered('elseif ($v === #) { echo #; }', $n, ' '),
    'labels-wide' => fn ($n) => 'goto l' . ($n - 1) . '; ' . numbered('l#: echo "#";', $n - 1, ' ') . ' l' . ($n - 1) . ': echo "last";',
    'gotos-wide' => fn ($n) => '$i = 0; ' . str_repeat('if ($i++ > 0) goto done; ', $n) . 'done: echo $i;',
    'elseif-goto-ladder' => fn ($n) => '$v = ' . ($n - 1) . '; if ($v === -1) { goto done; } ' . numbered('elseif ($v === #) { echo #; goto done; }', $n, ' ') . ' echo "missed"; done: echo " done";',
    'else-if-ladder' => fn ($n) => '$v = ' . ($n - 1) . '; if ($v === -1) {} ' . numbered('else if ($v === #) { echo #; }', $n, ' '),
    'blocks-nested' => fn ($n) => str_repeat('{', $n) . 'echo 1;' . str_repeat('}', $n),
    'if-nested' => fn ($n) => '$t = 1; ' . str_repeat('if ($t) { ', $n) . 'echo 2;' . str_repeat(' }', $n),
    'while-nested' => fn ($n) => '$t = 1; ' . str_repeat('while ($t) { ', $n) . 'echo 3; $t = 0;' . str_repeat(' }', $n),
    'foreach-nested' => fn ($n) => '$a = [1]; ' . str_repeat('foreach ($a as $v) { ', $n) . 'echo $v;' . str_repeat(' }', $n),
    'for-nested' => fn ($n) => str_repeat('for ($i = 0; $i < 1; $i++) { ', $n) . 'echo 4;' . str_repeat(' }', $n),
    'statements' => fn ($n) => '$a = 0; ' . str_repeat('$a++; ', $n) . 'echo $a;',
    'echo-list' => fn ($n) => 'echo ' . chain("''", ', ', "''", $n - 1) . ', "done";',
    'isset-list' => fn ($n) => '$a = 1; var_dump(isset(' . chain('$a', ', ', '$a', $n - 1) . '));',
    'unset-list' => fn ($n) => '$a = 1; unset(' . chain('$a', ', ', '$a', $n - 1) . '); var_dump(isset($a));',
    'global-list' => fn ($n) => 'function g() { global ' . numbered('$g#', $n, ', ') . '; return 1; } echo g();',

    // Functions.
    'closures-nested' => fn ($n) => '$f = ' . str_repeat('function () { return ', $n) . '5' . str_repeat('; }', $n) . '; echo is_callable($f) ? "y" : "n";',
    'arrow-nested' => fn ($n) => '$f = ' . str_repeat('fn () => ', $n) . '5; echo is_callable($f) ? "y" : "n";',
    'static-arrow-nested' => fn ($n) => '$f = ' . str_repeat('static fn () => ', $n) . '5; echo is_callable($f) ? "y" : "n";',
    'closure-use-list' => fn ($n) => numbered('$u# = #;', $n, ' ') . ' $f = function () use (' . numbered('$u#', $n, ', ') . ') { return $u0 + $u' . ($n - 1) . '; }; echo $f();',
    'parameters-wide' => fn ($n) => 'function f(' . numbered('$p# = #', $n, ', ') . ') { return $p' . ($n - 1) . '; } echo f();',
    'attributes-wide' => fn ($n) => '#[' . chain('A', ', ', 'A', $n - 1) . '] function f() { return "ok"; } echo f();',

    // Attributes and constant expressions, read back through reflection.
    'class-attributes-wide' => fn ($n) => str_repeat('#[A] ', $n) . 'class C {} echo count((new ReflectionClass("C"))->getAttributes());',
    'attribute-arguments-wide' => fn ($n) => '#[A(' . chain('1', ', ', '1', $n - 1) . ')] class C {} echo count((new ReflectionClass("C"))->getAttributes()[0]->getArguments());',
    'attribute-argument-nested' => fn ($n) => '#[A(' . str_repeat('[', $n) . '1' . str_repeat(']', $n) . ')] class C {} echo count((new ReflectionClass("C"))->getAttributes()[0]->getArguments());',
    'attribute-argument-chain' => fn ($n) => '#[A(' . chain('1', ' + ', '1', $n) . ')] class C {} echo (new ReflectionClass("C"))->getAttributes()[0]->getArguments()[0];',
    'class-constant-chain' => fn ($n) => 'class C { const K = ' . chain('1', ' + ', '1', $n) . '; } echo C::K;',
    'property-default-chain' => fn ($n) => 'class C { public $p = ' . chain('1', ' + ', '1', $n) . '; } echo (new C)->p;',
    'parameter-default-chain' => fn ($n) => 'function f($p = ' . chain('1', ' + ', '1', $n) . ') { return $p; } echo f();',

    // Generators.
    'yield-in-chain' => fn ($n) => 'function g() { $x = (yield 1)' . str_repeat(' + 1', $n) . '; return $x; } $g = g(); $g->current(); $g->send(1); echo $g->getReturn();',
    'yield-statements' => fn ($n) => 'function g() { ' . numbered('yield #;', $n, ' ') . ' } echo iterator_count(g());',
    'yield-from-ladder' => fn ($n) => 'function g($v) { if ($v === -1) { yield from []; } ' . numbered('elseif ($v === #) { yield from [#]; }', $n, ' ') . ' } echo implode(",", iterator_to_array(g(' . ($n - 1) . '), false));',

    // Classes.
    'class-methods' => fn ($n) => 'class C { ' . numbered('function m#() { return #; }', $n, ' ') . ' } echo (new C)->m' . ($n - 1) . '();',
    'class-constants' => fn ($n) => 'class C { ' . numbered('const K# = #;', $n, ' ') . ' } echo C::K' . ($n - 1) . ';',
    'class-properties' => fn ($n) => 'class C { ' . numbered('public $p# = #;', $n, ' ') . ' } echo (new C)->p' . ($n - 1) . ';',
    'functions' => fn ($n) => numbered('function f#() { return #; }', $n, ' ') . ' echo f' . ($n - 1) . '();',
];

if (($argv[1] ?? '') === '--list') {
    echo implode("\n", array_keys($shapes)), "\n";
    exit(0);
}

$name = $argv[1] ?? '';
$size = (int) ($argv[2] ?? 0);
if (!isset($shapes[$name]) || $size < 2) {
    fwrite(STDERR, "usage: generate.php --list | generate.php <shape> <size of at least 2>\n");
    exit(2);
}
echo '<?php ', $shapes[$name]($size), "\n";
