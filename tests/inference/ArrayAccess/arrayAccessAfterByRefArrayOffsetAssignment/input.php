<?php
/**
 * @param array{param1: array} $params
 */
function dispatch(array $params) : void {
    $params["param1"]["foo"] = "bar";
}

$ar = [];
dispatch(["param1" => &$ar]);
$value = "foo";
if (isset($ar[$value])) {
    echo (string) $ar[$value];
}
