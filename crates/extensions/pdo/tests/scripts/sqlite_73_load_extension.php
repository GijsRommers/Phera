<?php
$p=PDO::connect('sqlite::memory:');
foreach(['',"a\0b",'phera-no-such-extension'] as $name) {
    try {$p->loadExtension($name);}
    catch(Throwable $e) {
        var_dump(get_class($e),$e->getCode(),$e->getMessage());
        if($e instanceof PDOException) var_dump($e->errorInfo);
    }
}
