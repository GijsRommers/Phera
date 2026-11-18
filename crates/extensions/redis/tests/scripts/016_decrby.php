<?php
require __DIR__."/../support.php";
foreach (["present", "missing", "wrong"] as $kind) {
$key=$p.$kind;
if ($kind === "present") { $r->set($key, "12"); }
if ($kind === "wrong") { $r->hSet($key,"f","v"); }
check(fn()=>$r->decrBy($key,3));
check(fn()=>$r->getLastError()); $r->clearLastError();
}
finish();
