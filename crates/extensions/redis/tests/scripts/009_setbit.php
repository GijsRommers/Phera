<?php
require __DIR__."/../support.php";
foreach (["present", "missing", "wrong"] as $kind) {
$key=$p.$kind;
if ($kind === "present") { $r->set($key, "ab\0cd"); }
if ($kind === "wrong") { $r->hSet($key,"f","v"); }
check(fn()=>$r->setBit($key,1,true));
check(fn()=>$r->getLastError()); $r->clearLastError();
}
finish();
