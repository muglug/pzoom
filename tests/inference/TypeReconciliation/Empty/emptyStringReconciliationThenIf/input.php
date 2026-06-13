<?php
/**
 * @param Exception|string|string[] $a
 */
function foo($a): string {
    if (is_array($a)) {
        return "hello";
    } elseif (empty($a)) {
        return "goodbye";
    }

    if (is_string($a)) {
        return $a;
    };

    return "an exception";
}
