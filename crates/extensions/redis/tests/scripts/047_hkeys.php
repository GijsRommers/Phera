<?php
require __DIR__."/../support.php";
foreach (["present","missing","wrong"] as $kind) {
$key=$p.$kind;if ($kind==="present") {$r->hSet($key,"a","12");}
if ($kind==="wrong") {$r->set($key,"wrong");}
check(fn()=>$r->hKeys($key));check(fn()=>$r->getLastError());$r->clearLastError();}
finish();
