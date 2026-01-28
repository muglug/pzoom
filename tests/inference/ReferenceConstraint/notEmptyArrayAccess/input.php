<?php
/**
 * @param mixed $value
 * @param-out int $value
 */
function addValue(&$value) : void {
    $value = 5;
}

$foo = [];

addValue($foo["a"]);
