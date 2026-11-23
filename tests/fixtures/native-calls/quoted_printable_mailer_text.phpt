--TEST--
Quoted-printable mail text preserves CRLF and wraps before an escaped byte
--FILE--
<?php
foreach ([
    "", "café", "a\tb", "space \r\nnext ", "one\ntwo\rthree", "\0=\xFF",
    str_repeat('a', 75), str_repeat('a', 76), str_repeat('a', 74) . "=b"
] as $text) {
    echo json_encode(quoted_printable_encode($text)), "\n";
}
--EXPECT--
""
"caf=C3=A9"
"a=09b"
"space=20\r\nnext "
"one=0Atwo=0Dthree"
"=00=3D=FF"
"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa=\r\na"
"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa=\r\n=3Db"
