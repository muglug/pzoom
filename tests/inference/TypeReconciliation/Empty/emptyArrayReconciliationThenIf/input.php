<?php
/**
 * @param string|string[] $a
 */
function foo($a): string {
    if (is_string($a)) {
        return $a;
    } elseif (empty($a)) {
        return "goodbye";
    }

    if (isset($a[0])) {
        return $a[0];
    };

    return "not found";
}
