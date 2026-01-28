<?php
$arr = [];
while (rand(0, 1)) {
    if (rand(0, 1)) {
        if (!isset($arr["a"]["b"])) {
            $arr["a"]["b"] = "foo";
        }
        echo $arr["a"]["b"];
    } else {
        $arr["c"] = "foo";
    }
}