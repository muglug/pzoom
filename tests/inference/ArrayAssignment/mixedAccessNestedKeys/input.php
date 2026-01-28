<?php
function takesString(string $s) : string { return "hello"; }
function updateArray(array $arr) : array {
    foreach ($arr as $i => $item) {
        $arr[$i]["a"]["b"] = 5;
        $arr[$i]["a"]["c"] = takesString($arr[$i]["a"]["c"]);
    }

    return $arr;
}
