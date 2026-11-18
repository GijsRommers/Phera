<?php
require __DIR__."/../support.php";
foreach (["present","missing","wrong"] as $kind) {
$key=$p.$kind;if ($kind==="present") {$r->sAdd($key,"a");}
if ($kind==="wrong") {$r->set($key,"wrong");}
check(fn()=>$r->sIsMember($key,"a"));check(fn()=>$r->getLastError());$r->clearLastError();}
finish();
