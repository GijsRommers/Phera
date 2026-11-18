<?php
require __DIR__."/../support.php";
foreach (["present","missing","wrong"] as $kind) {
$key=$p.$kind;if ($kind==="present") {$r->zAdd($key,1.5,"a");}
if ($kind==="wrong") {$r->set($key,"wrong");}
check(fn()=>$r->zRange($key,0,-1,true));check(fn()=>$r->getLastError());$r->clearLastError();}
finish();
