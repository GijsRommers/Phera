<?php
require __DIR__."/../support.php";
foreach (["present","missing","wrong"] as $kind) {
$key=$p.$kind;if ($kind==="present") {$r->hSet($key,"a","12");}
if ($kind==="wrong") {$r->set($key,"wrong");}
check(fn()=>$r->hSet($key,"b","v\0","c","z"));check(fn()=>$r->getLastError());$r->clearLastError();}
finish();
