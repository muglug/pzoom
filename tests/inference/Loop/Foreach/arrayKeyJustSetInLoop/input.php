<?php
$a = null;
$arr = [];

foreach ([1, 2, 3] as $_) {
    if (rand(0, 1)) {
        $arr["a"]["c"] = "foo";
        $a = $arr["a"]["c"];
    } else {
        $arr["b"]["c"] = "bar";
        $a = $arr["b"]["c"];
    }
}
