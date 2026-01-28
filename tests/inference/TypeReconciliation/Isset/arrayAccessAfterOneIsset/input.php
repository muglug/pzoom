<?php
$arr = [];

foreach ([1, 2, 3] as $foo) {
    if (!isset($arr["bar"])) {
        $arr["bar"] = 0;
    }

    echo $arr["bar"];
}