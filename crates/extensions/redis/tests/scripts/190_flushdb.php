<?php
require __DIR__ . '/../support.php';
// Database 12 belongs to this test in the task-owned Redis service.
$r->select(12);
foreach ([null,true,false] as $mode) {
    $r->set($p.'key','value');
    check(fn()=>$r->flushDB($mode));check(fn()=>$r->dbSize());
    check(fn()=>$r->get($p.'key'));
}
finish();
