<?php
// R3-3: `allowed_classes` is not honoured inside SPL containers' own
// unserialize payloads.
//
// unserialize() checks allowed_classes for the `C:11:"ArrayObject"` object
// itself, but the payload that ArrayObject::unserialize() then parses
// (crates/extensions/spl/src/legacy/read.rs, the `O:` branch) calls
// create_object() for ANY class name, with no allowed_classes check. So
//
//     unserialize($data, ['allowed_classes' => ['ArrayObject']])
//
// instantiates `Evil` (and sets its properties) from the nested `O:` inside
// the ArrayObject payload, while real PHP 8.5.12 turns the nested object into
// __PHP_Incomplete_Class. An application that unserializes attacker-controlled
// data with an allow list gets objects of non-allowed classes anyway -- the
// exact object-injection primitive allowed_classes exists to prevent.
//
// Run:  phera tests/security/repro_r3_allowed_classes_spl.php
// Exit status 1 = vulnerable (Evil was really instantiated).
// Reference: /opt/homebrew/bin/php -n tests/security/repro_r3_allowed_classes_spl.php
// prints __PHP_Incomplete_Class and exits 0.

class Evil
{
    public $x = 1;
}

$inner = 'x:i:0;O:4:"Evil":1:{s:1:"x";i:9;};m:a:0:{}';
$payload = 'C:11:"ArrayObject":' . strlen($inner) . ':{' . $inner . '}';

$result = unserialize($payload, ['allowed_classes' => ['ArrayObject']]);

// var_dump() is the reliable view of ArrayObject's storage: a real Evil shows
// as object(Evil)#, PHP's blocked attempt as object(__PHP_Incomplete_Class)#.
ob_start();
var_dump($result);
$dump = ob_get_clean();
$evilIsReal = str_contains($dump, 'object(Evil)#');
printf("outer=%s dump=%s\n", get_class($result), trim(preg_replace('/\s+/', ' ', $dump)));

if ($evilIsReal) {
    echo "VULNERABLE: Evil was instantiated although allowed_classes=['ArrayObject']\n";
    exit(1);
}
echo "ok: the nested object was not instantiated as Evil (PHP parity)\n";
exit(0);
