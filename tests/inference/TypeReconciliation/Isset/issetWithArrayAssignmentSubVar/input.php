<?php

/**
 * @param array{0?: 0, v: 0} $arr
 * @return array{0: 0|1, v: 0}
 */
function t2(array $arr): array {
    if (!isset($arr[$arr["v"]])) {
        $arr[$arr["v"]] = 1;
    }
    return $arr;
}
