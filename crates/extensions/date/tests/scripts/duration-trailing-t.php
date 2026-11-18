<?php
try {
    $span = new DateInterval("P1YT");
    echo $span->format('%R%Y-%M-%D %H:%I:%S.%F %a %r %y %m %d %h %i %s %f %% %q %'), "\n";
    echo json_encode([
        $span->y, $span->m, $span->d, $span->h,
        $span->i, $span->s, $span->f, $span->days,
    ]), "\n";
} catch (Throwable $error) {
    echo get_class($error), " ", $error->getMessage(), "\n";
}
