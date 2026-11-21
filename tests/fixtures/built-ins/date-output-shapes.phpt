--TEST--
Date fields and buffered string output preserve exact bytes
--FILE--
<?php
date_default_timezone_set('UTC');
$display = 'D l d j S N w z W o F M m n t L y Y X x a A B g h G H i s u v e I O P p T Z c r U';
foreach ([
    new DateTimeImmutable('2024-02-29 01:23:45.123456', new DateTimeZone('UTC')),
    new DateTimeImmutable('2024-08-31 14:33:12', new DateTimeZone('+05:30')),
    new DateTimeImmutable('2023-12-31 22:00:00', new DateTimeZone('-08:00')),
] as $date) {
    echo $date->format($display), "\n";
    echo bin2hex($date->format("Y\\m%\x80\0\\")), "\n";
}
foreach (['d', 'W', 'B', 'Y', 'Z', 'i'] as $format) {
    var_dump(idate($format, 1714137123));
}
class OutputText {
    function __toString(): string { return 'object'; }
}
function out_text($value) { echo $value; }
ob_start();
echo 'A';
$out = 'B';
$alias = &$out;
out_text($alias);
out_text(new OutputText());
ini_set('precision', '5');
out_text(1.234567);
echo "\0\xff";
var_dump(bin2hex(ob_get_clean()));
ob_start(fn($text) => strtoupper($text), 2);
echo 'a', 'b', 'cd';
ob_end_flush();
echo "\n";
--EXPECT--
Thu Thursday 29 29 th 4 4 59 09 2024 February Feb 02 2 29 1 24 2024 +2024 2024 am AM 099 1 01 1 01 23 45 123456 123 UTC 0 +0000 +00:00 Z UTC 0 2024-02-29T01:23:45+00:00 Thu, 29 Feb 2024 01:23:45 +0000 1709169825
323032346d25800000
Sat Saturday 31 31 st 6 6 243 35 2024 August Aug 08 8 31 1 24 2024 +2024 2024 pm PM 418 2 02 14 14 33 12 000000 000 +05:30 0 +0530 +05:30 +05:30 GMT+0530 19800 2024-08-31T14:33:12+05:30 Sat, 31 Aug 2024 14:33:12 +0530 1725094992
323032346d25800000
Sun Sunday 31 31 st 7 0 364 52 2023 December Dec 12 12 31 0 23 2023 +2023 2023 pm PM 291 10 10 22 22 00 00 000000 000 -08:00 0 -0800 -08:00 -08:00 GMT-0800 -28800 2023-12-31T22:00:00-08:00 Sun, 31 Dec 2023 22:00:00 -0800 1704088800
323032336d25800000
int(26)
int(17)
int(591)
int(2024)
int(0)
int(12)
string(32) "41426f626a656374312e3233343600ff"
ABCD
