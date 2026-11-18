<?php
require __DIR__."/../support.php";
$r->set($p."a","ab\0");$r->set($p."b","cd\0");
foreach (["AND","OR","XOR"] as $op) {check(fn()=>$r->bitop($op,$p."d",$p."a",$p."b"));
check(fn()=>bin2hex($r->get($p."d")));}
check(fn()=>$r->bitop("NOT",$p."d",$p."a"));
finish();
