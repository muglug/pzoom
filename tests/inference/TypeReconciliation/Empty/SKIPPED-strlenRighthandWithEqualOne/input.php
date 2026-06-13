<?php
/** @return non-empty-string */
function nonEmptyString(string $str): string {
    return 1 === strlen($str) ? $str : "string";
}
