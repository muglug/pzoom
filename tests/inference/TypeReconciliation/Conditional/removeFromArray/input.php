<?php
/**
 * @param array<string> $v
 */
function foo(array $v) : void {
    if (!isset($v[0])) {
        return;
    }

    if ($v[0] === " ") {
        array_shift($v);
    }

    if (!isset($v[0])) {}
}