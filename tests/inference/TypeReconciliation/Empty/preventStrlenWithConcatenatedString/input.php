<?php
/** @return non-empty-string */
function nonEmptyString(string $str): string {
    return strlen("a" . $str . "b") > 2 ? $str : "string";
}
