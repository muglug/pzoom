<?php
function fetchRow() : array {
    return ["c" => "UK"];
}

$arr = [];

foreach ([1, 2, 3] as $i) {
    $row = fetchRow();

    if (!isset($arr[$row["c"]])) {
        $arr[$row["c"]] = 0;
    }

    $arr[$row["c"]] = 1;
}