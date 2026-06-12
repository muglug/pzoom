<?php
/**
 * @param int|string $param
 * @param float|string $param2
 * @return void
 */
function foo($param, $param2) {
    if ($param === $param2) {
        takesString($param);
        takesString($param2);
    }
}

function takesString(string $arg): void {}
