<?php
/** @param array<string> $arr */
function foo(array $arr) : void {
    foreach ($arr as $k => $_) {
        if (is_numeric($k)) {}
        if (!is_numeric($k)) {}
    }
}