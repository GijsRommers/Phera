--TEST--
PHP namespaces share the file-scope label table
--FILE--
<?php
namespace First {
    goto here;
    echo "miss\n";
}
namespace Second {
    here: echo __NAMESPACE__, "\n";
}
--EXPECT--
Second
