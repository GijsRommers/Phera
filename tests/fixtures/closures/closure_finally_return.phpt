--TEST--
closure finally return
--FILE--
<?php
$f=function(){try{return 7;}finally{echo "done
";}};var_dump($f());
--EXPECT--
done
int(7)
