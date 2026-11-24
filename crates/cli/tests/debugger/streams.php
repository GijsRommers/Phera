<?php
echo "copied\n";
fwrite(STDERR, "redirected\n");
xdebug_break();
echo "finished\n";
