<?php
/** @return non-empty-string */
function nonEmptyString(string $str): string {
    return mb_strlen($str) === 1 ? $str : "string";
}
