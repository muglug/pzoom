<?php
/**
 * @param string|int $s
 */
function foo($s, int $f = 1) : void {
    if ($f === 1
        && (string) $s === $s
        && \strpos($s, "foo") !== false
    ) {}
}