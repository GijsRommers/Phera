<?php
foreach ([1,2,4,8,16,32,64,128,256,512,1024,2047,4095,-1,0,2048,4160,4096] as $group) {
    echo json_encode(DateTimeZone::listIdentifiers($group, 'BE')), "\n";
}
foreach (['BE','US','AU','GB','NZ','NP','??','us','UK','ZZ'] as $country) {
    echo json_encode(timezone_identifiers_list(4096, $country)), "\n";
}
foreach (DateTimeZone::listIdentifiers(4095) as $name) {
    $zone = new DateTimeZone($name);
    echo json_encode([$name,$zone->getName(),$zone->getLocation()]), "\n";
    echo json_encode($zone->getTransitions()), "\n";
    echo json_encode($zone->getTransitions(-2208988800, 946684800)), "\n";
    echo json_encode($zone->getTransitions(4081459200, 4133980800)), "\n";
}
echo json_encode(DateTimeZone::listAbbreviations()), "\n";
foreach (['GMT','UTC','z','BST','est','eSt','XYZ','', 'AEDT'] as $abbr) {
    foreach ([-1,0,3600,-18000,39600] as $offset) {
        foreach ([-1, 0, 1] as $dst) {
            echo json_encode(timezone_name_from_abbr($abbr, $offset, $dst)), "\n";
        }
    }
}
echo json_encode(timezone_name_from_abbr("", 3600, 4294967296)), "\n";
