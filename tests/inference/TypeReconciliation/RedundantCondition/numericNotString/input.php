<?php
/** @param mixed $value */
function test($value) : void {
    if (!is_numeric($value)) {
        throw new Exception("Invalid $value");
    }
    if (!is_string($value)) {}
}