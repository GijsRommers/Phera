<?php
error_reporting(E_ALL & ~E_DEPRECATED);

function item(string $name, $value): void {
    echo $name, '=', json_encode($value, JSON_THROW_ON_ERROR), "\n";
}

$options = getopt('ac', ['long:'], $rest);
item('getopt', [$options, $rest]);
item('rot13', str_rot13("Phera 42!"));
item('increment', [str_increment('Z9'), str_decrement('AA')]);
item('quoted printable', quoted_printable_decode("hello=20world=\r\n=21"));
item('uuencode', convert_uuencode("The green world\0"));
item('uudecode', bin2hex(convert_uudecode(convert_uuencode("a\0b"))));
item('utf8 decode', bin2hex(utf8_decode("A\xc3\xa9\xf0\x9f\x98\x80")));

setlocale(LC_ALL, 'C');
item('langinfo', [nl_langinfo(DAY_1), nl_langinfo(ABMON_4), nl_langinfo(RADIXCHAR)]);
item('codeset', in_array(nl_langinfo(CODESET), ['ASCII', 'US-ASCII', 'ANSI_X3.4-1968'], true));
item('hebrev', [bin2hex(hebrev("\xe0\xe1 abc \xe2\xe3")), hebrev('aa bb cc', 6)]);
item('strftime', gmstrftime('%Y-%m-%d %H:%M:%S', 1700000000));
item('timeofday', [array_keys(gettimeofday()), is_float(gettimeofday(true))]);

class ForwardBase {
    public static function who(string $a, string $b): array {
        return [static::class, $a, $b];
    }
    public static function run(): void {
        item('forward', forward_static_call([self::class, 'who'], 'a', 'b'));
        item('forward array', forward_static_call_array([self::class, 'who'], ['b' => 'B', 'a' => 'A']));
    }
}
class ForwardChild extends ForwardBase {}
ForwardChild::run();

item('last modification', getlastmod() === filemtime(__FILE__));
item('device', linkinfo(__FILE__) === lstat(__FILE__)['dev']);
item('project ids', ftok(__FILE__, 'a') !== ftok(__FILE__, 'b'));
item('disk space', is_float(diskfreespace(__DIR__)));
item('highlight alias', show_source(__FILE__, true) === highlight_file(__FILE__, true));
item('realpath cache', [is_array(realpath_cache_get()), realpath_cache_size() >= 0]);

$temporary = tempnam(sys_get_temp_dir(), 'phera-standard-');
try {
    file_put_contents($temporary, '<meta content="hello &amp; bye" name="First.Tag"><meta name=second content=two></head><meta name=ignored content=value>');
    item('meta tags', get_meta_tags($temporary));
    $title = "\x1c\x02\x05\x00\x03red";
    item('iptc tags', iptcparse($title));
    file_put_contents($temporary, hex2bin('ffd8ffe000044142ffda0002ffd9'));
    $embedded = iptcembed($title, $temporary);
    item('iptc jpeg', [str_starts_with($embedded, "\xff\xd8"), str_contains($embedded, $title), str_ends_with($embedded, "\xff\xd9")]);
} finally {
    unlink($temporary);
}

ob_start();
debug_zval_dump(['hello' => 'world']);
$dump = ob_get_clean();
item('dump shape', preg_replace('/ (?:refcount\(\d+\)|interned)/', '', $dump));
ob_start();
phpcredits(CREDITS_GROUP);
item('credits heading', str_contains(ob_get_clean(), 'PHP Group'));
