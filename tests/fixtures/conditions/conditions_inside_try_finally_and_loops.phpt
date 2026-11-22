--TEST--
conditions_inside_try_finally_and_loops
--FILE--
<?php
function t($x): string {
    $out = '';
    try {
        if ($x < 0) { $out .= 'neg'; return $out . '/ret'; }
        $out .= 'pos';
        foreach ([1,2] as $v) { if ($v === 2) { $out .= '/brk'; break; } $out .= "/$v"; }
    } finally {
        $out .= '/fin';
    }
    return $out;
}
echo t(-1), "\n", t(5), "\n";
function loopfin($n): string {
    $out = '';
    for ($i = 0; $i < $n; $i++) {
        try { if ($i === 1) continue; if ($i === 2) return $out . 'ret'; $out .= "i$i;"; }
        finally { $out .= "f$i;"; }
    }
    return $out;
}
echo loopfin(4), "\n";
function dowhilefin($n): string {
    $out = '';
    $i = 0;
    do {
        try { $i++; if ($i < $n) continue; $out .= "done$i"; }
        finally { $out .= "f$i;"; }
    } while ($i < $n);
    return $out;
}
echo dowhilefin(3), "\n";
--EXPECTF--
neg/ret
pos/1/brk/fin
i0;f0;f1;ret
f1;f2;done3f3;
