--TEST--
extract-modes
--FILE--
<?php
function probe($mode) {
 $a = 10; $present = null;
 $items = ['a' => 1, 'b' => 2, 'bad-key' => 3, 4 => 4, 'present' => 5];
 echo extract($items, $mode, 'p'), ":";
 var_dump($a, $b ?? null, $p_a ?? null, $p_b ?? null, $p_4 ?? null, $present, $p_present ?? null);
}
foreach ([EXTR_OVERWRITE, EXTR_SKIP, EXTR_PREFIX_SAME, EXTR_PREFIX_ALL, EXTR_PREFIX_INVALID, EXTR_PREFIX_IF_EXISTS, EXTR_IF_EXISTS] as $mode) probe($mode);

--EXPECT--
3:int(1)
int(2)
NULL
NULL
NULL
int(5)
NULL
1:int(10)
int(2)
NULL
NULL
NULL
NULL
NULL
3:int(10)
int(2)
int(1)
NULL
NULL
NULL
int(5)
4:int(10)
NULL
int(1)
int(2)
int(4)
NULL
int(5)
4:int(1)
int(2)
NULL
NULL
int(4)
int(5)
NULL
3:int(10)
int(2)
int(1)
NULL
NULL
NULL
int(5)
2:int(1)
NULL
NULL
NULL
NULL
int(5)
NULL
