--TEST--
condition_shapes_if_while_do_for_ternary
--FILE--
<?php
function which($a, $b, $n): string {
    $out = '';
    if ($a === $b) { $out .= 'ident'; } else { $out .= 'not-ident'; }
    if ($a !== null) { $out .= ',not-null'; }
    if (null === $a) { $out .= ',null'; }
    if ($a < $b) { $out .= ',less'; }
    if ($b > $a) { $out .= ',greater'; }
    if ($a <= 10) { $out .= ',le10'; }
    if (10 >= $a) { $out .= ',ge10'; }
    if ($a == $b) { $out .= ',eq'; }
    if (!$a) { $out .= ',falsy'; }
    if ($a) { $out .= ',truthy'; }
    $i = 0;
    while ($i < $n) { $i++; }
    $out .= ",w$i";
    $j = 0;
    do { $j++; } while ($j < $n);
    $out .= ",d$j";
    for ($k = 0; $k < $n; $k++) { $out .= ''; }
    $out .= ",f$k";
    $out .= $a === $b ? ',t' : ',f';
    $out .= ($a && $b) ? ',and' : ',or-not';
    $out .= ($a || $b) ? ',or' : ',nor';
    $out .= ($a xor $b) ? ',xor' : ',nxor';
    return $out;
}
echo which(3, 3, 4), "\n";
echo which(3, 4, 0), "\n";
echo which(0, 0, 2), "\n";
echo which(null, 0, 1), "\n";
echo which("3", 3, 1), "\n";
// else-if ladder
function ladder($x): string {
    if ($x === 1) return 'one';
    elseif ($x === 2) return 'two';
    elseif ($x < 0) return 'neg';
    else return 'other';
}
echo ladder(1), ladder(2), ladder(-5), ladder(9), "\n";
// multiple for conditions: only the last decides, all run
function multi($n): int {
    $calls = 0;
    for ($i = 0; $i < $n, $calls < 2; $i++) { $calls++; }
    return $calls;
}
echo multi(5), "\n";
// nested short circuit in conditions
function nested($a, $b, $c): string {
    if ($a && ($b || $c)) return 'yes';
    return 'no';
}
echo nested(true, false, true), nested(true, false, false), nested(false, true, true), "\n";
--EXPECTF--
ident,not-null,le10,ge10,eq,truthy,w4,d4,f4,t,and,or,nxor
not-ident,not-null,less,greater,le10,ge10,truthy,w0,d1,f0,f,and,or,nxor
ident,not-null,le10,ge10,eq,falsy,w2,d2,f2,t,or-not,nor,nxor
not-ident,null,le10,ge10,eq,falsy,w1,d1,f1,f,or-not,nor,nxor
not-ident,not-null,le10,ge10,eq,truthy,w1,d1,f1,f,and,or,nxor
onetwonegother
2
yesnono
