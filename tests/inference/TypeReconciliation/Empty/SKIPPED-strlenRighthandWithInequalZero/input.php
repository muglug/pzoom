<?php
/** @return non-empty-string */
function nonEmptyString(string $str): string {
    return 0 !== strlen($str) ? $str : "string";
}
