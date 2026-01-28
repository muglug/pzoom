<?php
function testRef() : array {
    $result = [];
    foreach ([1, 2, 1] as $v) {
        $x = &$result;
        if (!isset($x[$v])) {
            $x[$v] = 0;
        }
        $x[$v] ++;
    }
    return $result;
}
