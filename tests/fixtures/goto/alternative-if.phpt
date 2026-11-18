--TEST--
Labels work across branches of an alternative-syntax if
--FILE--
<?php
goto otherwise;
if (true):
    first: echo "first\n";
    goto done;
else:
    otherwise: goto first;
endif;
done:
echo "done\n";
--EXPECT--
first
done
