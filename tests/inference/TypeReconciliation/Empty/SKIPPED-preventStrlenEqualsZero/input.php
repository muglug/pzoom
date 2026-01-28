<?php
/** @return non-empty-string */
function nonEmptyString(string $str): string {
    return strlen($str) === 0 ? $str : "string";
}
