<?php
require __DIR__."/../support.php";
foreach (["present","missing","wrong"] as $kind) {
$key=$p.$kind;if ($kind==="present") {$r->xAdd($key,"1-0",["f"=>"v"]);}
if ($kind==="wrong") {$r->set($key,"wrong");}
check(fn()=>$r->xTrim($key,"1"));check(fn()=>$r->getLastError());$r->clearLastError();}
finish();
