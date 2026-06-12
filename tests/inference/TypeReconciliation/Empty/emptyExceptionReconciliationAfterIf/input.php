<?php
/**
 * @param Exception|null $a
 */
function foo($a): string {
    if ($a && $a->getMessage() === "hello") {
        return "hello";
    } elseif (empty($a)) {
        return "goodbye";
    }

    return $a->getMessage();
}
