--TEST--
literal_operand_forms
--FILE--
<?php
function lit($a): string {
    $out = '';
    if ($a !== null) $out .= 'nn';
    if ($a === null) $out .= 'n';
    if ($a < 10) $out .= 'l10';
    if ($a <= -3) $out .= 'le-3';
    if (10 > $a) $out .= '10g';
    if (7 == $a) $out .= 'e7';
    if ($a === true) $out .= 'tru';
    if ($a === false) $out .= 'fls';
    if ($a !== false) $out .= 'nfls';
    if ($a == "3") $out .= 'eq3';
    if ($a === "3") $out .= 'id3';
    if ($a < "10") $out .= 'slt10';
    if ($a > 1.5) $out .= 'g15';
    return $out;
}
foreach ([null, 0, 3, "3", 10, 1.5, true, false, "abc"] as $v) {
    echo var_export($v, true), ' => ', lit($v), "\n";
}
--EXPECTF--
NULL => nl10le-310gnflsslt10
0 => nnl1010gnflsslt10
3 => nnl1010gnflseq3slt10g15
'3' => nnl1010gnflseq3id3slt10g15
10 => nnnflsg15
1.5 => nnl1010gnflsslt10
true => nnle-3e7trunflseq3
false => nnl10le-310gflsslt10
'abc' => nnnflsg15
