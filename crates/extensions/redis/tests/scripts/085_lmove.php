<?php
require __DIR__."/../support.php";
foreach (["present","missing","wrong"] as $kind) {
$key=$p.$kind;if ($kind==="present") {$r->rPush($key,"a","b\0");}
if ($kind==="wrong") {$r->set($key,"wrong");}
check(fn()=>$r->lMove($key,$p."dst","LEFT","RIGHT"));check(fn()=>$r->getLastError());$r->clearLastError();}
finish();
