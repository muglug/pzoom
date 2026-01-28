<?php
$arr = ["a", "b"];
array_map("mapdef", $arr, array_fill(0, count($arr), 1));
function mapdef(string $_a, int $_b = 0): string {
    return "a";
}
