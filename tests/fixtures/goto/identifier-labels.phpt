--TEST--
Constant spellings and contextual names are legal labels
--FILE--
<?php
goto true;
parent: goto null;
enum: echo "enum\n"; return;
null: goto enum;
true: goto parent;
--EXPECT--
enum
