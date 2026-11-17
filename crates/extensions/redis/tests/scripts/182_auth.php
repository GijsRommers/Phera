<?php
require __DIR__ . '/../support.php';
$username = $p . 'user';
$r->rawCommand('ACL','SETUSER',$username,'on','>secret','~'.$p.'*','+@all');
try {
    $c=new Redis();$c->connect('127.0.0.1',16399);$c->select(14);
    check(fn()=>$c->auth([$username,'secret']));check(fn()=>$c->getAuth());
    check(fn()=>$c->set($p.'a','value'));check(fn()=>$c->get($p.'a'));
    check(fn()=>$c->auth([$username,'incorrect']));check(fn()=>$c->getAuth());
    $c->close();
    $c=new Redis(['host'=>'127.0.0.1','port'=>16399,'database'=>14,'auth'=>[$username,'secret']]);
    check(fn()=>$c->get($p.'a'));check(fn()=>$c->getAuth());
    $c->close();
} finally { $r->rawCommand('ACL','DELUSER',$username); }
finish();
