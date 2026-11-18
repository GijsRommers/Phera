--TEST--
Output handlers receive start, clean, flush and final modes
--FILE--
<?php
$modes = [];
ob_start();
ob_start(static function ($text, $mode) use (&$modes) {
    $modes[] = [$text, $mode];
    return strtoupper($text);
}, 4);
echo 'ab', 'cd';
echo 'ef';
ob_clean();
echo 'gh';
ob_flush();
echo 'i';
$raw = ob_get_flush();
$body = ob_get_clean();
echo json_encode([$raw, $body, $modes]), "\n";
--EXPECT--
["i","ABCDGHI",[["abcd",1],["ef",2],["gh",4],["i",8]]]
