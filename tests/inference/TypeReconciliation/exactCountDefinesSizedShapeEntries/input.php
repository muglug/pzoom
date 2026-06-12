<?php
/** @param list<string> $refs */
function f(array $refs): void {
    if (count($refs) > 1) {
        echo 'many';
    } elseif (count($refs) === 1) {
        echo $refs[0];
    }
}
