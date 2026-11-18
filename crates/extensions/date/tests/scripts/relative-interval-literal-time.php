<?php
try {
    $interval = DateInterval::createFromDateString("12:00");
    echo $interval->format('%R%Y-%M-%D %H:%I:%S.%F %a %r %y %m %d %h %i %s %f %% %q %'), "\n";
    echo json_encode($interval->__serialize()), "\n";
} catch (Throwable $error) {
    echo get_class($error), " ", $error->getMessage(), "\n";
}
