<?php
$arr = [];

foreach ([1, 2, 3] as $foo) {
    if (!isset($arr["foo"])) {
        $arr["foo"] = 0;
    }

    if (!isset($arr["bar"])) {
        $arr["bar"] = 0;
    }

    echo $arr["bar"];
}