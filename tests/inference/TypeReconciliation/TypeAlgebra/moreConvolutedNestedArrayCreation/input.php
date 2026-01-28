<?php
function fetchRow() : array {
    return ["c" => "UK"];
}

$arr = [];

foreach ([1, 2, 3] as $i) {
    $row = fetchRow();

    if (!isset($arr[$row["c"]]["foo"])) {
        $arr[$row["c"]]["foo"] = 0;
    }

    $arr[$row["c"]]["foo"] = 1;
}