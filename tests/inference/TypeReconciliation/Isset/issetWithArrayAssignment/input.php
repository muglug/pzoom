<?php

/**
 * @param array{0?: 0} $arr
 * @param 0 $i
 * @return array{0: 0|1}
 */
function t2(array $arr, int $i): array {
    if (!isset($arr[$i])) {
        $arr[$i] = 1;
    }
    return $arr;
}
