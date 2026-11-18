<?php
require __DIR__ . '/../support.php';
$c=new Redis();$c->connect('127.0.0.1',16399);$c->select(14);
check(fn()=>$c->__destruct());check(fn()=>$c->isConnected());
check(fn()=>$c->ping());check(fn()=>$c->close());check(fn()=>$c->isConnected());
$c->connect('127.0.0.1',16399);$c->select(14);
foreach (['multi','pipeline'] as $mode) {
    $c->$mode();$c->ping();check(fn()=>$c->__destruct());
    check(fn()=>$c->getMode());check(fn()=>$c->isConnected());
    check(fn()=>$c->exec());check(fn()=>$c->ping());
}
$c->close();
finish();
