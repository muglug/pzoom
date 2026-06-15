<?php
/** @psalm-suppress MissingParamType */
function foo($a) : array {
    $arr = ["a" => "foo"];

    /**
     * @psalm-suppress MixedAssignment
     */
    foreach ($a as $k => $v) {
        $arr[$k] = $v;
    }

    return $arr;
}
