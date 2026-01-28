<?php
/**
 * @param string[] $arr
 */
function foo(array $arr) : void {
    if (!$arr) {
        return;
    }

    foreach ($arr as $i => $_) {}

    echo (string) $i;
}
