<?php
/**
 * @param list<string> $slugParts
 */
function foo(array $slugParts) : void {
    if (!$slugParts) {
        $slugParts = [""];
    }
    array_shift($slugParts);
    if (!empty($slugParts)) {}
}
