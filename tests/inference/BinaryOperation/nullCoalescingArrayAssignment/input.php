<?php
/**
 * @param array<string> $arr
 */
function foo(array $arr) : void {
    $b = [];

    foreach ($arr as $a) {
        $b[0] ??= $a;
    }
}
