<?php
/** @return non-empty-string */
function nonEmptyString(string $str): string {
    return strlen($str) >= 1 ? $str : "string";
}
