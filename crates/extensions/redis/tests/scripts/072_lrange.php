<?php
require __DIR__."/../support.php";
foreach (["present","missing","wrong"] as $kind) {
$key=$p.$kind;if ($kind==="present") {$r->rPush($key,"a","b\0");}
if ($kind==="wrong") {$r->set($key,"wrong");}
check(fn()=>$r->lRange($key,0,-1));check(fn()=>$r->getLastError());$r->clearLastError();}
finish();
