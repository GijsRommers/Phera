<?php
$date = new DateTimeImmutable("2024-04-07 01:45:00 Australia/Lord_Howe");
$interval = new DateInterval("PT30M");
$added = $date->add($interval);
$subtracted = $date->sub($interval);
echo $added->format('Y-m-d\TH:i:s.uP e I'), "\n";
echo $subtracted->format('Y-m-d\TH:i:s.uP e I'), "\n";
echo $date->diff($added)->format('%R%Y-%M-%D %H:%I:%S.%F %a %r %y %m %d %h %i %s %f %% %q %'), "\n";
echo $added->diff($date)->format('%R%Y-%M-%D %H:%I:%S.%F %a %r %y %m %d %h %i %s %f %% %q %'), "\n";
