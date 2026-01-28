<?php
/** @param string[] $arr */
function getOrderings(array $arr): int {
    if ($arr) {
        $next = null;
        foreach (array_reverse($arr) as $v) {
            $next = 1;
        }
        return $next;
    }

    return 2;
}
